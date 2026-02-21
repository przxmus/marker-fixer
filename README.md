# marker-fixer

`marker-fixer` is a command-line tool that converts OBS MP4 chapter markers into Adobe Premiere Pro-compatible XMP markers.

It updates MP4 metadata only (no video/audio re-encode).

## Quick Start

1. Download a prebuilt package from GitHub Releases (recommended for most users).
2. Unzip the archive for your OS.
3. Run:

```bash
./marker-fixer recording.mp4
```

## Automatic FFmpeg/FFprobe Download

`marker-fixer` needs `ffprobe` (and validates `ffmpeg` availability).

At runtime it resolves tools in this order:
1. CLI override (`--ffprobe` / `--ffmpeg`)
2. Local bundled path next to executable (`fftools/<os>/<arch>/...`)
3. System PATH
4. Automatic download (only if still missing)

Automatic download happens only when all previous options are unavailable.
Downloaded files are stored next to the executable in `fftools/<os>/<arch>/`.

## Usage

```bash
marker-fixer [PATH ...] [OPTIONS]
```

`PATH` can be:
- one MP4 file,
- multiple files,
- a directory (non-recursive; only files in that directory level).

### Common Examples

```bash
# Export to sibling file (default suffix)
marker-fixer recording.mp4

# Write to a sibling file with custom suffix
marker-fixer recording.mp4 --output-suffix _fixed

# Process a directory (non-recursive)
marker-fixer ./captures

# Preview only (no changes written)
marker-fixer ./captures --dry-run

# Replace malformed existing XMP metadata
marker-fixer recording.mp4 --force

# Explicit tool paths (skip auto-download)
marker-fixer recording.mp4 --ffprobe /custom/ffprobe --ffmpeg /custom/ffmpeg
```

### Options

- `--output-suffix <suffix>`: suffix for output sibling file (default `_fixed`, cannot be empty)
- `--force`: replace malformed existing XMP metadata
- `--ffprobe <path>`: override ffprobe path
- `--ffmpeg <path>`: override ffmpeg path
- `-v, --verbose`: show additional diagnostics
- `-n, --dry-run`: analyze only, do not write files

## Build from Source

If you prefer local builds:

```bash
./build.sh
```

Build outputs are written to `build/` as clean zipped artifacts.

## License and Third-Party Notice (FFmpeg)

FFmpeg is licensed under GNU LGPL v2.1 (or later) for supported build configurations.

- FFmpeg project: [https://ffmpeg.org](https://ffmpeg.org)
- FFmpeg legal/license page: [https://ffmpeg.org/legal.html](https://ffmpeg.org/legal.html)
- LGPL v2.1 text: [https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html](https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html)

See `THIRD_PARTY_NOTICES.md` for additional details.
