# Vendored FFmpeg Runtime Files

Place prebuilt FFmpeg runtime files here so `build.sh` can bundle them into release artifacts.

Required layout:

- `vendor/fftools/macos/arm64/ffmpeg`
- `vendor/fftools/macos/arm64/ffprobe`
- `vendor/fftools/macos/x86_64/ffmpeg`
- `vendor/fftools/macos/x86_64/ffprobe`
- `vendor/fftools/linux/x86_64/ffmpeg`
- `vendor/fftools/linux/x86_64/ffprobe`
- `vendor/fftools/windows/x86_64/ffmpeg.exe`
- `vendor/fftools/windows/x86_64/ffprobe.exe`

For Windows, include any required FFmpeg `.dll` files in:
- `vendor/fftools/windows/x86_64/`

`build.sh` copies the full platform directory so required DLLs remain standalone runtime files.

Current populated sources:
- Linux x86_64 and Windows x86_64 (LGPL shared): BtbN FFmpeg Builds
  - https://github.com/BtbN/FFmpeg-Builds/releases
- macOS arm64 and macOS x86_64 binaries: eugeneware/ffmpeg-static release assets
  - https://github.com/eugeneware/ffmpeg-static/releases
