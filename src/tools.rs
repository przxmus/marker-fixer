use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{MarkerFixerError, Result};

#[derive(Debug, Clone, Copy)]
pub enum ToolKind {
    Ffmpeg,
    Ffprobe,
}

impl ToolKind {
    fn binary_name(self) -> &'static str {
        match self {
            Self::Ffmpeg => "ffmpeg",
            Self::Ffprobe => "ffprobe",
        }
    }

    fn display_name(self) -> &'static str {
        self.binary_name()
    }
}

pub fn ensure_runtime_tools(
    ffprobe_override: Option<&Path>,
    ffmpeg_override: Option<&Path>,
    verbose: bool,
) -> Result<()> {
    let ffprobe = resolve_tool(ToolKind::Ffprobe, ffprobe_override, true, verbose)?;
    let ffmpeg = resolve_tool(ToolKind::Ffmpeg, ffmpeg_override, true, verbose)?;

    if verbose {
        eprintln!(
            "Runtime tools ready: ffprobe={} ffmpeg={}",
            ffprobe.display(),
            ffmpeg.display()
        );
    }

    Ok(())
}

pub fn resolve_tool_for_execution(tool: ToolKind, override_path: Option<&Path>) -> Result<PathBuf> {
    resolve_tool(tool, override_path, false, false)
}

fn resolve_tool(
    tool: ToolKind,
    override_path: Option<&Path>,
    allow_auto_download: bool,
    verbose: bool,
) -> Result<PathBuf> {
    if let Some(path) = override_path {
        if path.exists() {
            return Ok(path.to_path_buf());
        }

        return Err(MarkerFixerError::ExternalTool {
            tool: tool.display_name(),
            message: format!("Provided path does not exist: {}", path.display()),
        });
    }

    let bundled = bundled_tool_path(tool);
    if bundled.exists() {
        return Ok(bundled);
    }

    if command_available_on_path(tool) {
        return Ok(PathBuf::from(tool.binary_name()));
    }

    if allow_auto_download {
        if verbose {
            eprintln!(
                "{} not found locally. Attempting to download runtime tools...",
                tool.display_name()
            );
        }

        download_runtime_bundle(verbose)?;
        if bundled.exists() {
            return Ok(bundled);
        }
    }

    Err(MarkerFixerError::ExternalTool {
        tool: tool.display_name(),
        message: format!(
            "{} not found. Searched override path, bundled location ({}), and system PATH.",
            tool.display_name(),
            bundled.display()
        ),
    })
}

fn command_available_on_path(tool: ToolKind) -> bool {
    match Command::new(tool.binary_name()).arg("-version").output() {
        Ok(_) => true,
        Err(err) => err.kind() != std::io::ErrorKind::NotFound,
    }
}

fn bundled_tool_path(tool: ToolKind) -> PathBuf {
    bundled_tools_dir().join(tool_filename(tool))
}

fn bundled_tools_dir() -> PathBuf {
    let exe_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    let exe_dir = exe_path
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);

    let (os, arch) = platform_and_arch();
    exe_dir.join("fftools").join(os).join(arch)
}

fn platform_and_arch() -> (&'static str, &'static str) {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => ("macos", "arm64"),
        ("macos", "x86_64") => ("macos", "x86_64"),
        ("linux", "x86_64") => ("linux", "x86_64"),
        ("windows", "x86_64") => ("windows", "x86_64"),
        (os, arch) => (os, arch),
    }
}

fn tool_filename(tool: ToolKind) -> String {
    if std::env::consts::OS == "windows" {
        format!("{}.exe", tool.binary_name())
    } else {
        tool.binary_name().to_string()
    }
}

fn download_runtime_bundle(verbose: bool) -> Result<()> {
    let target_dir = bundled_tools_dir();
    fs::create_dir_all(&target_dir).map_err(|source| MarkerFixerError::Io {
        path: target_dir.clone(),
        source,
    })?;

    let specs = download_specs_for_current_platform()?;
    for spec in specs {
        let output_path = target_dir.join(spec.filename);
        if output_path.exists() {
            continue;
        }

        if verbose {
            eprintln!("Downloading {} -> {}", spec.url, output_path.display());
        }

        let response =
            ureq::get(spec.url)
                .call()
                .map_err(|err| MarkerFixerError::ExternalTool {
                    tool: "runtime-downloader",
                    message: format!("failed to download {}: {err}", spec.url),
                })?;

        let mut reader = response.into_reader();
        let temp_path = output_path.with_extension("download");
        let mut file = fs::File::create(&temp_path).map_err(|source| MarkerFixerError::Io {
            path: temp_path.clone(),
            source,
        })?;

        std::io::copy(&mut reader, &mut file).map_err(|source| MarkerFixerError::Io {
            path: temp_path.clone(),
            source,
        })?;
        file.flush().map_err(|source| MarkerFixerError::Io {
            path: temp_path.clone(),
            source,
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o755);
            fs::set_permissions(&temp_path, perms).map_err(|source| MarkerFixerError::Io {
                path: temp_path.clone(),
                source,
            })?;
        }

        fs::rename(&temp_path, &output_path).map_err(|source| MarkerFixerError::Io {
            path: output_path.clone(),
            source,
        })?;
    }

    Ok(())
}

struct DownloadSpec {
    filename: &'static str,
    url: &'static str,
}

fn download_specs_for_current_platform() -> Result<Vec<DownloadSpec>> {
    let specs = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => vec![
            DownloadSpec {
                filename: "ffmpeg",
                url: "https://github.com/eugeneware/ffmpeg-static/releases/download/b6.1.1/ffmpeg-darwin-arm64",
            },
            DownloadSpec {
                filename: "ffprobe",
                url: "https://github.com/eugeneware/ffmpeg-static/releases/download/b6.1.1/ffprobe-darwin-arm64",
            },
        ],
        ("macos", "x86_64") => vec![
            DownloadSpec {
                filename: "ffmpeg",
                url: "https://github.com/eugeneware/ffmpeg-static/releases/download/b6.1.1/ffmpeg-darwin-x64",
            },
            DownloadSpec {
                filename: "ffprobe",
                url: "https://github.com/eugeneware/ffmpeg-static/releases/download/b6.1.1/ffprobe-darwin-x64",
            },
        ],
        ("linux", "x86_64") => vec![
            DownloadSpec {
                filename: "ffmpeg",
                url: "https://github.com/eugeneware/ffmpeg-static/releases/download/b6.1.1/ffmpeg-linux-x64",
            },
            DownloadSpec {
                filename: "ffprobe",
                url: "https://github.com/eugeneware/ffmpeg-static/releases/download/b6.1.1/ffprobe-linux-x64",
            },
        ],
        ("windows", "x86_64") => vec![
            DownloadSpec {
                filename: "ffmpeg.exe",
                url: "https://github.com/eugeneware/ffmpeg-static/releases/download/b6.1.1/ffmpeg-win32-x64",
            },
            DownloadSpec {
                filename: "ffprobe.exe",
                url: "https://github.com/eugeneware/ffmpeg-static/releases/download/b6.1.1/ffprobe-win32-x64",
            },
        ],
        (os, arch) => {
            return Err(MarkerFixerError::ExternalTool {
                tool: "runtime-downloader",
                message: format!("unsupported platform for auto-download: {os}/{arch}"),
            });
        }
    };

    Ok(specs)
}
