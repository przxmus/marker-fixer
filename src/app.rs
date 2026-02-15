use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use clap::{ArgAction, Parser};

use crate::error::{IoResultExt, MarkerFixerError, Result};
use crate::tools;
use crate::{ffprobe, mp4, xmp};

#[derive(Debug, Clone, Parser)]
#[command(
    name = "marker-fixer",
    version,
    about = "Convert OBS MP4 chapters into Adobe Premiere Pro markers.",
    long_about = "marker-fixer reads chapter markers from OBS MP4 files and writes Premiere-compatible XMP markers into the MP4 metadata.\n\nYou can pass files, directories (non-recursive), or drag-and-drop multiple files onto the executable.",
    after_help = "Examples:\n  marker-fixer recording.mp4\n  marker-fixer recording.mp4 --in-place false --output-suffix _fixed\n  marker-fixer ./captures --dry-run\n\nTip: If a file has malformed existing XMP, rerun with --force to replace it."
)]
pub struct Cli {
    #[arg(
        value_name = "PATH",
        required = true,
        help = "Input MP4 file(s) or directories"
    )]
    pub paths: Vec<PathBuf>,

    #[arg(
        long = "in-place",
        action = ArgAction::Set,
        default_value_t = true,
        help = "Overwrite the source file",
        long_help = "If true (default), marker-fixer updates each source file directly. If false, it writes a sibling file using --output-suffix."
    )]
    pub in_place: bool,

    #[arg(
        long = "output-suffix",
        default_value = "_fixed",
        help = "Suffix for output filename when --in-place=false"
    )]
    pub output_suffix: String,

    #[arg(
        long = "force",
        default_value_t = false,
        help = "Replace malformed existing XMP instead of failing"
    )]
    pub force: bool,

    #[arg(
        long = "ffprobe",
        help = "Path to ffprobe binary (overrides bundled and PATH lookup)"
    )]
    pub ffprobe: Option<PathBuf>,

    #[arg(
        long = "ffmpeg",
        help = "Path to ffmpeg binary (overrides bundled/PATH/auto-download lookup)"
    )]
    pub ffmpeg: Option<PathBuf>,

    #[arg(
        short,
        long = "verbose",
        default_value_t = false,
        help = "Show extra diagnostics"
    )]
    pub verbose: bool,

    #[arg(
        short = 'n',
        long = "dry-run",
        default_value_t = false,
        help = "Analyze and report without writing files"
    )]
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub enum FileStatus {
    Converted,
    ConvertedDryRun,
    SkippedNoChapters,
    SkippedNotMp4,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct FileReport {
    pub path: PathBuf,
    pub status: FileStatus,
}

#[derive(Default)]
struct Summary {
    converted: usize,
    converted_dry_run: usize,
    skipped_no_chapters: usize,
    skipped_not_mp4: usize,
    failed: usize,
}

pub struct App;

impl App {
    pub fn run() -> i32 {
        match Self::run_inner() {
            Ok(exit_code) => exit_code,
            Err(err) => {
                eprintln!("ERROR: {err}");
                eprintln!("Tip: run with --help for usage details.");
                1
            }
        }
    }

    fn run_inner() -> Result<i32> {
        let cli = Cli::parse();
        let files = collect_input_files(&cli.paths)?;
        if files.is_empty() {
            return Err(MarkerFixerError::NoInputPaths);
        }

        print_preflight_summary(&cli, &files);

        if std::env::var("MARKER_FIXER_SKIP_RUNTIME_TOOL_BOOTSTRAP").as_deref() != Ok("1") {
            if let Err(err) = tools::ensure_runtime_tools(
                cli.ffprobe.as_deref(),
                cli.ffmpeg.as_deref(),
                cli.verbose,
            ) {
                return Err(MarkerFixerError::ExternalTool {
                    tool: "runtime-tools",
                    message: format!(
                        "{err}. Ensure internet access or provide --ffprobe/--ffmpeg paths."
                    ),
                });
            }
        }

        let reports = files
            .iter()
            .map(|path| process_file(path, &cli))
            .collect::<Vec<_>>();

        let summary = print_reports(&reports);
        print_summary(&summary, cli.dry_run);

        Ok(if summary.failed > 0 { 1 } else { 0 })
    }
}

fn print_preflight_summary(cli: &Cli, files: &[PathBuf]) {
    println!("marker-fixer {}", env!("CARGO_PKG_VERSION"));
    println!("- Inputs: {} file(s)", files.len());
    println!(
        "- Mode: {}",
        if cli.dry_run {
            "dry-run (no files will be changed)"
        } else if cli.in_place {
            "in-place overwrite"
        } else {
            "write alongside source files"
        }
    );
    if !cli.in_place {
        println!("- Output suffix: {}", cli.output_suffix);
    }
    if cli.force {
        println!("- Force mode: enabled (malformed existing XMP will be replaced)");
    }
    println!();
}

fn print_reports(reports: &[FileReport]) -> Summary {
    let mut summary = Summary::default();

    for report in reports {
        match &report.status {
            FileStatus::Converted => {
                summary.converted += 1;
                println!("[OK]   {} -> converted", report.path.display());
            }
            FileStatus::ConvertedDryRun => {
                summary.converted_dry_run += 1;
                println!("[PLAN] {} -> would convert", report.path.display());
            }
            FileStatus::SkippedNoChapters => {
                summary.skipped_no_chapters += 1;
                println!(
                    "[SKIP] {} -> no embedded chapters found",
                    report.path.display()
                );
            }
            FileStatus::SkippedNotMp4 => {
                summary.skipped_not_mp4 += 1;
                println!("[SKIP] {} -> not an .mp4 file", report.path.display());
            }
            FileStatus::Failed(message) => {
                summary.failed += 1;
                println!("[ERR]  {} -> {}", report.path.display(), message);
            }
        }
    }

    summary
}

fn print_summary(summary: &Summary, dry_run: bool) {
    println!();
    println!("Summary:");
    if dry_run {
        println!("- Would convert: {}", summary.converted_dry_run);
    } else {
        println!("- Converted: {}", summary.converted);
    }
    println!("- Skipped (no chapters): {}", summary.skipped_no_chapters);
    println!("- Skipped (not MP4): {}", summary.skipped_not_mp4);
    println!("- Failed: {}", summary.failed);

    if summary.failed == 0 {
        println!("\nDone. You can now import the output MP4(s) into Premiere Pro.");
    } else {
        println!(
            "\nCompleted with errors. Re-run with --verbose for diagnostics, or use --force for malformed existing XMP."
        );
    }
}

fn collect_input_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for input_path in paths {
        if !input_path.exists() {
            return Err(MarkerFixerError::PathMissing(input_path.clone()));
        }

        if input_path.is_file() {
            files.push(input_path.clone());
            continue;
        }

        if input_path.is_dir() {
            let entries = fs::read_dir(input_path).at_path(input_path)?;
            for entry in entries {
                let entry = entry.at_path(input_path)?;
                let path = entry.path();
                if path.is_file() {
                    files.push(path);
                }
            }
        }
    }

    files.sort();
    files.dedup();

    Ok(files)
}

fn process_file(path: &Path, cli: &Cli) -> FileReport {
    if !is_mp4(path) {
        return FileReport {
            path: path.to_path_buf(),
            status: FileStatus::SkippedNotMp4,
        };
    }

    if cli.verbose {
        eprintln!("Processing {}", path.display());
    }

    let probe = match ffprobe::probe_media(path, cli.ffprobe.as_deref()) {
        Ok(probe) => probe,
        Err(err) => {
            return FileReport {
                path: path.to_path_buf(),
                status: FileStatus::Failed(err.to_string()),
            };
        }
    };

    if probe.chapters.is_empty() {
        return FileReport {
            path: path.to_path_buf(),
            status: FileStatus::SkippedNoChapters,
        };
    }

    let incoming_markers = probe
        .chapters
        .iter()
        .map(|chapter| {
            xmp::marker_from_chapter(chapter.start_seconds, chapter.title.as_deref(), probe.fps)
        })
        .collect::<Vec<_>>();

    let existing_payload = match mp4::read_xmp_payload(path) {
        Ok(payload) => payload,
        Err(err) => {
            return FileReport {
                path: path.to_path_buf(),
                status: FileStatus::Failed(err.to_string()),
            };
        }
    };

    let (existing_markers, frame_rate) = if let Some(payload) = existing_payload {
        match String::from_utf8(payload) {
            Ok(xml_data) => match xmp::parse_markers(&xml_data) {
                Ok(parsed) => (
                    parsed.markers,
                    parsed
                        .frame_rate
                        .unwrap_or_else(|| probe.frame_rate_expr.clone()),
                ),
                Err(err) if cli.force => {
                    if cli.verbose {
                        eprintln!(
                            "Replacing malformed existing XMP in {} due to --force: {}",
                            path.display(),
                            err
                        );
                    }
                    (Vec::new(), probe.frame_rate_expr.clone())
                }
                Err(err) => {
                    return FileReport {
                        path: path.to_path_buf(),
                        status: FileStatus::Failed(format!(
                            "Malformed existing XMP: {err}. Re-run with --force to replace it."
                        )),
                    };
                }
            },
            Err(err) if cli.force => {
                if cli.verbose {
                    eprintln!(
                        "Replacing non-UTF8 existing XMP in {} due to --force: {}",
                        path.display(),
                        err
                    );
                }
                (Vec::new(), probe.frame_rate_expr.clone())
            }
            Err(err) => {
                return FileReport {
                    path: path.to_path_buf(),
                    status: FileStatus::Failed(format!(
                        "Existing XMP is not UTF-8: {err}. Re-run with --force to replace it."
                    )),
                };
            }
        }
    } else {
        (Vec::new(), probe.frame_rate_expr.clone())
    };

    let merged = xmp::merge_markers(existing_markers, incoming_markers);
    let xmp_xml = xmp::generate_xmp(&frame_rate, &merged);

    if cli.dry_run {
        return FileReport {
            path: path.to_path_buf(),
            status: FileStatus::ConvertedDryRun,
        };
    }

    let output_path = output_path_for(path, cli.in_place, &cli.output_suffix);
    if let Err(err) = mp4::write_xmp_payload(path, &output_path, xmp_xml.as_bytes()) {
        return FileReport {
            path: path.to_path_buf(),
            status: FileStatus::Failed(err.to_string()),
        };
    }

    FileReport {
        path: path.to_path_buf(),
        status: FileStatus::Converted,
    }
}

fn output_path_for(input: &Path, in_place: bool, output_suffix: &str) -> PathBuf {
    if in_place {
        return input.to_path_buf();
    }

    let stem = input
        .file_stem()
        .and_then(OsStr::to_str)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "output".to_string());

    let extension = input
        .extension()
        .and_then(OsStr::to_str)
        .map(|value| format!(".{value}"))
        .unwrap_or_default();

    input.with_file_name(format!("{stem}{output_suffix}{extension}"))
}

fn is_mp4(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|ext| ext.eq_ignore_ascii_case("mp4"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_mp4_extensions_case_insensitively() {
        assert!(is_mp4(Path::new("video.mp4")));
        assert!(is_mp4(Path::new("video.MP4")));
        assert!(!is_mp4(Path::new("video.mov")));
    }

    #[test]
    fn computes_output_path_when_not_in_place() {
        let output = output_path_for(Path::new("/tmp/video.mp4"), false, "_fixed");
        assert_eq!(output, Path::new("/tmp/video_fixed.mp4"));
    }
}
