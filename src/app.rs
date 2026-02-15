use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use clap::{ArgAction, Parser};

use crate::error::{IoResultExt, MarkerFixerError, Result};
use crate::{ffprobe, mp4, xmp};

#[derive(Debug, Clone, Parser)]
#[command(name = "marker-fixer", version, about = "Convert OBS MP4 chapters into Premiere XMP markers")]
pub struct Cli {
    #[arg(value_name = "PATH", required = true)]
    pub paths: Vec<PathBuf>,

    #[arg(long = "in-place", action = ArgAction::Set, default_value_t = true)]
    pub in_place: bool,

    #[arg(long = "output-suffix", default_value = "_fixed")]
    pub output_suffix: String,

    #[arg(long = "force", default_value_t = false)]
    pub force: bool,

    #[arg(long = "ffprobe")]
    pub ffprobe: Option<PathBuf>,

    #[arg(long = "ffmpeg")]
    pub ffmpeg: Option<PathBuf>,

    #[arg(long = "verbose", default_value_t = false)]
    pub verbose: bool,

    #[arg(long = "dry-run", default_value_t = false)]
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub enum FileStatus {
    Converted,
    SkippedNoChapters,
    SkippedNotMp4,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct FileReport {
    pub path: PathBuf,
    pub status: FileStatus,
}

pub struct App;

impl App {
    pub fn run() -> i32 {
        match Self::run_inner() {
            Ok(exit_code) => exit_code,
            Err(err) => {
                eprintln!("error: {err}");
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

        if cli.ffmpeg.is_some() && cli.verbose {
            eprintln!("Note: --ffmpeg is reserved for future use; current pipeline uses ffprobe only.");
        }

        let reports = files.iter().map(|path| process_file(path, &cli)).collect::<Vec<_>>();

        let mut has_failed = false;
        for report in &reports {
            match &report.status {
                FileStatus::Converted => println!("{}: converted", report.path.display()),
                FileStatus::SkippedNoChapters => {
                    println!("{}: skipped(no_chapters)", report.path.display())
                }
                FileStatus::SkippedNotMp4 => {
                    println!("{}: skipped(not_mp4)", report.path.display())
                }
                FileStatus::Failed(message) => {
                    has_failed = true;
                    println!("{}: failed({message})", report.path.display());
                }
            }
        }

        Ok(if has_failed { 1 } else { 0 })
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
        .map(|chapter| xmp::marker_from_chapter(chapter.start_seconds, chapter.title.as_deref(), probe.fps))
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
                Ok(parsed) => (parsed.markers, parsed.frame_rate.unwrap_or_else(|| probe.frame_rate_expr.clone())),
                Err(err) if cli.force => (Vec::new(), probe.frame_rate_expr.clone()),
                Err(err) => {
                    return FileReport {
                        path: path.to_path_buf(),
                        status: FileStatus::Failed(err.to_string()),
                    };
                }
            },
            Err(err) if cli.force => {
                if cli.verbose {
                    eprintln!("Ignoring malformed UTF-8 XMP in {} due to --force: {err}", path.display());
                }
                (Vec::new(), probe.frame_rate_expr.clone())
            }
            Err(err) => {
                return FileReport {
                    path: path.to_path_buf(),
                    status: FileStatus::Failed(format!("existing XMP is not UTF-8: {err}")),
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
            status: FileStatus::Converted,
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
