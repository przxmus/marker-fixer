# marker-fixer

`marker-fixer` is a command-line tool that converts OBS MP4 chapter markers into Adobe Premiere Pro-compatible XMP markers.

It updates MP4 metadata only (no video/audio re-encode).

## Quick Start

1. Build the project:

```bash
./build.sh
```

2. Run it on a file:

```bash
./marker-fixer recording.mp4
```

3. Import the MP4 into Premiere Pro and check clip markers.

## What It Does

- Reads embedded chapters from MP4 via `ffprobe`
- Converts chapter start times into frame-based Premiere marker time
- Writes/updates XMP marker metadata in MP4 (`uuid` XMP box)
- Keeps original MP4 chapters intact
- Merges with existing XMP markers and deduplicates by marker time

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
# Overwrite source file (default)
marker-fixer recording.mp4

# Write to a sibling file
marker-fixer recording.mp4 --in-place false --output-suffix _fixed

# Process a directory (non-recursive)
marker-fixer ./captures

# Preview only (no changes written)
marker-fixer ./captures --dry-run

# Replace malformed existing XMP metadata
marker-fixer recording.mp4 --force
```

### Options

- `--in-place <true|false>`: overwrite source file (default `true`)
- `--output-suffix <suffix>`: suffix when `--in-place=false` (default `_fixed`)
- `--force`: replace malformed existing XMP metadata
- `--ffprobe <path>`: override ffprobe binary path
- `--ffmpeg <path>`: reserved for future workflows
- `-v, --verbose`: show additional diagnostics
- `-n, --dry-run`: analyze only, do not write files

## Output Statuses

The tool prints one line per file:

- `[OK] ... -> converted`
- `[PLAN] ... -> would convert` (dry-run)
- `[SKIP] ... -> no embedded chapters found`
- `[SKIP] ... -> not an .mp4 file`
- `[ERR] ... -> <error details + hint>`

At the end, it prints a summary with totals.

## Bundled FFmpeg/FFprobe Runtime Files

This project is packaged with standalone FFmpeg runtime files (`ffmpeg`, `ffprobe`, and on Windows required `.dll` files).

Runtime lookup order:
1. explicit CLI override (`--ffprobe`)
2. bundled files near executable: `fftools/<os>/<arch>/...`
3. system `PATH`

If bundled files are missing and `PATH` does not provide ffprobe, `marker-fixer` prints the exact expected location.

## Build Artifacts

`./build.sh` creates clean release packages in `build/` for:
- `aarch64-apple-darwin`
- `x86_64-apple-darwin`
- `x86_64-unknown-linux-musl`
- `x86_64-pc-windows-gnu`

Each archive contains only required runtime files:
- `marker-fixer` binary,
- bundled `fftools` directory,
- user docs and third-party notices.

## License and Third-Party Notice (FFmpeg)

FFmpeg is licensed under GNU LGPL v2.1 (or later) for the build configuration used here.

- FFmpeg project: [https://ffmpeg.org](https://ffmpeg.org)
- FFmpeg legal/license page: [https://ffmpeg.org/legal.html](https://ffmpeg.org/legal.html)
- LGPL v2.1 text: [https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html](https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html)

See `THIRD_PARTY_NOTICES.md` for additional details.
