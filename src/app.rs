use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use clap::{ArgAction, Parser};

use crate::error::{IoResultExt, MarkerFixerError, Result};
use crate::ffprobe;

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

    if cli.verbose {
        eprintln!(
            "Detected {} chapter(s) at {:.6} fps in {}",
            probe.chapters.len(),
            probe.fps,
            path.display()
        );
    }

    if cli.dry_run {
        return FileReport {
            path: path.to_path_buf(),
            status: FileStatus::Converted,
        };
    }

    FileReport {
        path: path.to_path_buf(),
        status: FileStatus::Failed("writer not implemented yet".to_string()),
    }
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
}
