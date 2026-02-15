use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::error::{MarkerFixerError, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct ChapterInput {
    pub start_seconds: f64,
    pub title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProbeData {
    pub fps: f64,
    pub frame_rate_expr: String,
    pub chapters: Vec<ChapterInput>,
}

#[derive(Debug, Clone)]
struct ToolResolution {
    path: PathBuf,
    source: ToolSource,
    expected_bundle_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolSource {
    Override,
    Bundled,
    PathFallback,
}

#[derive(Debug, Deserialize)]
struct ProbeOutput {
    #[serde(default)]
    streams: Vec<ProbeStream>,
    #[serde(default)]
    chapters: Vec<ProbeChapter>,
}

#[derive(Debug, Deserialize)]
struct ProbeStream {
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    codec_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProbeChapter {
    start_time: String,
    tags: Option<ProbeChapterTags>,
}

#[derive(Debug, Deserialize)]
struct ProbeChapterTags {
    title: Option<String>,
}

pub fn probe_media(file_path: &Path, ffprobe_override: Option<&Path>) -> Result<ProbeData> {
    let resolution = resolve_ffprobe_path(ffprobe_override)?;

    let output = Command::new(&resolution.path)
        .arg("-v")
        .arg("error")
        .arg("-print_format")
        .arg("json")
        .arg("-show_streams")
        .arg("-show_chapters")
        .arg(file_path)
        .output()
        .map_err(|source| {
            let mut message = format!(
                "failed to execute ffprobe at {}: {source}",
                resolution.path.display()
            );

            if source.kind() == std::io::ErrorKind::NotFound
                && resolution.source == ToolSource::PathFallback
            {
                message.push_str(
                    ". ffprobe was not found in PATH and no bundled binary was detected.",
                );
                message.push_str(&format!(
                    " Expected bundled path: {}",
                    resolution.expected_bundle_path.display()
                ));
            }

            MarkerFixerError::ExternalTool {
                tool: "ffprobe",
                message,
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(MarkerFixerError::ExternalTool {
            tool: "ffprobe",
            message: if stderr.is_empty() {
                format!(
                    "{} exited with status {}",
                    resolution.path.display(),
                    output.status
                )
            } else {
                stderr
            },
        });
    }

    let parsed: ProbeOutput = serde_json::from_slice(&output.stdout)
        .map_err(|err| MarkerFixerError::InvalidMetadata(format!("invalid ffprobe JSON: {err}")))?;

    let (fps, frame_rate_expr) = detect_fps(&parsed.streams)?;
    let chapters = parsed
        .chapters
        .into_iter()
        .filter_map(|chapter| {
            let start_seconds = chapter.start_time.parse::<f64>().ok()?;
            Some(ChapterInput {
                start_seconds,
                title: chapter
                    .tags
                    .and_then(|tags| tags.title)
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty()),
            })
        })
        .collect();

    Ok(ProbeData {
        fps,
        frame_rate_expr,
        chapters,
    })
}

fn detect_fps(streams: &[ProbeStream]) -> Result<(f64, String)> {
    let video_stream = streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("video"))
        .ok_or_else(|| {
            MarkerFixerError::InvalidMetadata("No video stream found in ffprobe output".to_string())
        })?;

    if let Some(rate) = &video_stream.avg_frame_rate {
        if let Some(fps) = parse_rate(rate) {
            return Ok((fps, normalize_frame_rate_expr(rate)));
        }
    }

    if let Some(rate) = &video_stream.r_frame_rate {
        if let Some(fps) = parse_rate(rate) {
            return Ok((fps, normalize_frame_rate_expr(rate)));
        }
    }

    Err(MarkerFixerError::InvalidMetadata(
        "Unable to derive FPS from avg_frame_rate/r_frame_rate".to_string(),
    ))
}

fn normalize_frame_rate_expr(value: &str) -> String {
    if let Some((n, d)) = parse_rational_parts(value) {
        if d == 1 {
            return format!("f{n}");
        }
        return format!("f{n}/{d}");
    }

    format!("f{}", value.trim())
}

fn parse_rate(value: &str) -> Option<f64> {
    let (numerator, denominator) = parse_rational_parts(value)?;
    if denominator == 0 {
        return None;
    }

    let fps = numerator as f64 / denominator as f64;
    if fps.is_finite() && fps > 0.0 {
        Some(fps)
    } else {
        None
    }
}

fn parse_rational_parts(value: &str) -> Option<(u64, u64)> {
    let mut parts = value.split('/');
    let numerator = parts.next()?.trim().parse::<u64>().ok()?;
    let denominator = parts.next()?.trim().parse::<u64>().ok()?;
    Some((numerator, denominator))
}

fn resolve_ffprobe_path(ffprobe_override: Option<&Path>) -> Result<ToolResolution> {
    let expected_bundle_path = bundled_tool_path("ffprobe");

    if let Some(path) = ffprobe_override {
        if path.exists() {
            return Ok(ToolResolution {
                path: path.to_path_buf(),
                source: ToolSource::Override,
                expected_bundle_path,
            });
        }
        return Err(MarkerFixerError::ExternalTool {
            tool: "ffprobe",
            message: format!("Provided --ffprobe path does not exist: {}", path.display()),
        });
    }

    if expected_bundle_path.exists() {
        return Ok(ToolResolution {
            path: expected_bundle_path.clone(),
            source: ToolSource::Bundled,
            expected_bundle_path,
        });
    }

    Ok(ToolResolution {
        path: PathBuf::from("ffprobe"),
        source: ToolSource::PathFallback,
        expected_bundle_path,
    })
}

fn bundled_tool_path(tool: &str) -> PathBuf {
    let exe_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    let exe_dir = exe_path
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);

    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let filename = if os == "windows" {
        format!("{tool}.exe")
    } else {
        tool.to_string()
    };

    exe_dir.join("fftools").join(os).join(arch).join(filename)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_fps_values() {
        let pairs = [
            ("24000/1001", 23.976023976023978_f64),
            ("24/1", 24.0),
            ("25/1", 25.0),
            ("30000/1001", 29.97002997002997_f64),
            ("30/1", 30.0),
            ("50/1", 50.0),
            ("60/1", 60.0),
        ];

        for (rate, expected) in pairs {
            let parsed = parse_rate(rate).expect("rate should parse");
            assert!((parsed - expected).abs() < 0.000_001);
        }
    }

    #[test]
    fn normalizes_frame_rate_expr_for_xmp() {
        assert_eq!(normalize_frame_rate_expr("60/1"), "f60");
        assert_eq!(normalize_frame_rate_expr("30000/1001"), "f30000/1001");
    }
}
