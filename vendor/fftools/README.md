# Runtime Download Directory

`marker-fixer` no longer ships FFmpeg binaries in this repository.

At runtime, if `ffmpeg`/`ffprobe` are not found via:
1. CLI override,
2. local `fftools/<os>/<arch>/` next to executable,
3. system PATH,

the app downloads platform binaries automatically into that local `fftools/<os>/<arch>/` directory.

This repository keeps only this placeholder directory.
