# ADB `sendrecv_v2` compression, measured

Why `cmdr-adb` turns on LZ4 for `RCV2` and `SND2` and leaves brotli and zstd off (#191). What the crate does with it:
`crates/cmdr-adb/DETAILS.md` § "The wire contract".

## Setup

Pixel 9 Pro XL, Android 17 (`CP3A.261005.005`), on AC power and idle, USB 3, Mac load around 11–30 (not idle), adb
37.0.0, 2026-10-30. Timed `adb pull -z <algo>` and `adb push -z <algo>`, which use the same `sendrecv_v2` compression
Cmdr turns on. Three interleaved runs per cell; each number is the median MB/s, with min–max in parentheses.

## Numbers

- **Compressible** (290 MB, six copies of a Rust and TS source tarball):
  - Pull: none 177 (124–215), **lz4 232 (146–245)**, zstd 242 (221–245), brotli 231 (229–232).
  - Push: none 138 (109–180), **lz4 318 (294–382)**, zstd 343 (329–391), brotli 262 (258–306).
- **Photos** (215 MB, 60 camera JPEGs, already compressed):
  - Pull: none 150 (132–157), **lz4 155 (135–163)**, zstd 113 (95–121), brotli 90 (68–94).
  - Push: none 106 (106–114), **lz4 114 (113–126)**, zstd 126 (98–149), brotli 115 (115–121).

## Verdict

Turn on LZ4 both ways. It's +30% pulling and 2.3× pushing compressible data, and neutral on photos. zstd is a hair
faster on compressible data but costs about 25% pulling photos, and brotli costs up to 40%.

## Caveats

- Short runs (1–3 s each), on one phone that was idle and on AC power. A busy or battery-saving phone might weaken LZ4's
  pull gain, but LZ4 is the cheapest of the three for the phone's CPU.
- The numbers above time the reference client. Cmdr's own codec is `lz4_flex`, and it holds up in a release build (256
  MB, three runs, quiet Mac, same phone): compressible data pushes at 300 MB/s against 197 uncompressed and pulls at 346
  against 298, and random data pushes at 198 against 202 and pulls at 312 against 203. ❗ A debug build is far slower
  (compressible push 32 MB/s), so don't time copies on a dev build. Its ratio barely differs from `liblz4`'s: on the
  same inputs with the same frame settings (linked, 64 KiB), 5 MiB of source text came to 2,315,375 bytes against
  `liblz4`'s 2,314,936, and 20 MiB of zeros to 87,209 against 86,412 (`lz4` 1.10.0 CLI, 2026-10-30). Each codec also
  read the other's frames byte for byte.
