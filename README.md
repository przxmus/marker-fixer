# marker-fixer

`marker-fixer` to narzedzie CLI w Rust, ktore konwertuje chaptery/markery z OBS (`MP4`) na markery czytelne dla Adobe Premiere Pro zapisane w `XMP` (`uuid` box) bez reenkodowania audio/video.

## Co robi

- Czyta chaptery z `ffprobe` (`-show_chapters`)
- Wylicza `xmpDM:startTime` jako `floor(start_seconds * fps)`
- Tworzy lub aktualizuje root-level `uuid` box Adobe XMP (`be7acfcb-97a9-42e8-9c71-999491e3afac`)
- Scala z istniejacymi markerami `Markers` i deduplikuje po czasie
- Przy konflikcie marker z trescia (`name/comment`) ma priorytet nad pustym
- Nie usuwa chapterow z MP4

## Uzycie

```bash
marker-fixer [PATH ...] [FLAGS]
```

`PATH` moze byc:
- plikiem MP4,
- folderem (tylko biezacy poziom, bez rekursji),
- lista wielu sciezek (np. drag-and-drop kilku plikow na executable).

### Flagi

- `--in-place <true|false>` (domyslnie `true`)
- `--output-suffix <suffix>` (domyslnie `_fixed`, tylko dla `--in-place false`)
- `--force` (nadpisuje uszkodzony istniejacy XMP)
- `--ffprobe <path>` (wymusza konkretna binarke ffprobe)
- `--ffmpeg <path>` (zarezerwowane pod przyszle etapy pipeline)
- `--verbose`
- `--dry-run`

### Przyklady

```bash
# Nadpisz oryginal
marker-fixer recording.mp4

# Zapisz obok pliku
marker-fixer recording.mp4 --in-place false --output-suffix _fixed

# Przetworz folder (bez podfolderow)
marker-fixer ./captures

# Tylko raport, bez zapisu
marker-fixer recording.mp4 --dry-run
```

## Kody wyjscia

- `0` - wszystkie pliki przetworzone lub pominiete bez bledu krytycznego
- `1` - co najmniej jeden plik zakonczyl sie bledem (`failed(...)`)

## Raport per plik

- `converted`
- `skipped(no_chapters)`
- `skipped(not_mp4)`
- `failed(<powod>)`

## ffprobe / ffmpeg i bundling

Resolver `ffprobe` dziala w kolejnosci:
1. `--ffprobe <path>`
2. bundled obok executable: `fftools/<os>/<arch>/ffprobe[.exe]`
3. fallback do `ffprobe` z `PATH`

Dla release zalecany layout:

```text
marker-fixer
fftools/
  macos/arm64/ffprobe
  linux/x86_64/ffprobe
  windows/x86_64/ffprobe.exe
```

`ffmpeg` jest przewidziany w CLI pod przyszle rozszerzenia workflow, aktualny pipeline konwersji markerow korzysta z `ffprobe`.

## Walidacja w Premiere Pro

1. Uruchom konwersje na pliku z OBS.
2. Zaimportuj wynikowy MP4 do Premiere Pro.
3. Zweryfikuj markery w panelu markerow klipu.

## Testy

```bash
cargo test
```

Testy obejmuja:
- unit testy (fps, mapowanie czasu, merge, XML),
- integracje end-to-end na probkach MP4 (konwersja, dedupe, `--force`, folder mode bez rekursji).
