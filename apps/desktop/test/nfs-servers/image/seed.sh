#!/bin/sh
# Materializes one export's tree. Both exports get the same one, so a cell can
# assert on the same names whichever export it's pointed at.
#
# ❗ Owners are part of the fixture. Most entries belong to uid/gid 1000 while
# the export root itself stays root's, which is the shape of a QNAP share:
# `guess_identity` has to answer 1000 from the entries, ❌ not 0 from the root.
set -e
root="$1"

mkdir -p "$root/photos/2024 summer" "$root/nested/deep/deeper" "$root/empty-dir" "$root/oddnames"
printf 'hello from the nfs fixture\n' > "$root/hello.txt"
printf 'sun\n' > "$root/photos/2024 summer/beach.txt"
printf 'deep\n' > "$root/nested/deep/deeper/file.txt"
# Two files that differ in size, the precondition the shared rename conformance
# assertion checks before it trusts its own result.
printf '0123456789' > "$root/ten-bytes.txt"
printf '01234' > "$root/five-bytes.txt"
: > "$root/zero-bytes.txt"

# A name with spaces, and the same accented word twice: composed (NFC, U+00E9)
# and decomposed (NFD, `e` + U+0301). A Linux server stores bytes, so these are
# two different files, and a client that normalizes names on the way in
# collapses them into one.
printf 'composed\n' > "$root/a file with spaces.txt"
printf 'nfc\n' > "$root/caf$(printf '\303\251').txt"
printf 'nfd\n' > "$root/cafe$(printf '\314\201').txt"

# A file dated years back (2021-01-29 08:30:15 UTC, `conformance::SOURCE_DATE_SECS`),
# so a date cell can't mistake "now" for a kept date.
printf 'from 2021\n' > "$root/dated.txt"
touch -d @1611909015 "$root/dated.txt"

# One link that stays inside the export (resolved client-side) and one whose
# target is absolute and leaves it (shown as a link, opening it is `NotSupported`).
ln -s "photos/2024 summer/beach.txt" "$root/link-inside"
ln -s "/etc/hostname" "$root/link-outside"

# 0xE9 is `é` in latin-1 and isn't valid UTF-8 on its own. Listed nowhere,
# counted, per the plan's non-UTF-8 decision. Kept in its own folder so the
# root listing stays all-UTF-8.
python3 -c "open(b'$root/oddnames/caf\xe9.txt', 'w').write('latin-1')"
printf 'ok\n' > "$root/oddnames/fine.txt"

# ── The file the byte path reads ─────────────────────────────────────
#
# Self-describing by construction: every 16-byte line holds its own line number,
# zero-padded, so each position in the file says where it belongs. A reader that
# holes or duplicates a chunk lands bytes at offsets that no longer match their
# own contents, which is what lets a cell assert byte-exactness without shipping
# a copy of the file. Same format as the SFTP and WebDAV fixtures' `large.bin`.
#
# 200 MiB by default: big enough that the pipelined read window and the 8 MiB
# COMMIT cadence both cycle many times.
large_mb="${LARGE_MB:-200}"
python3 - "$root/large.bin" "$large_mb" <<'GENERATE'
import sys

path, mib = sys.argv[1], int(sys.argv[2])
LINES_PER_MIB = 1024 * 1024 // 16
with open(path, "wb") as out:
    for base in range(0, mib * LINES_PER_MIB, LINES_PER_MIB):
        out.write(b"".join(b"%015d\n" % line for line in range(base, base + LINES_PER_MIB)))
GENERATE

chown -R -h 1000:1000 "$root"
# The export root stays root's but world-writable (a QNAP share's usual shape),
# so a cell acting as any uid can write into it. One file is root's too, which a
# uid-1000 client can read (0644) but not overwrite.
chown 0:0 "$root" "$root/hello.txt"
chmod 0777 "$root"
chmod 0644 "$root/hello.txt"
