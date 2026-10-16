# cmdr-collation details

The one answer to "how do two names rank". It wraps an ICU4X `Collator` built for one locale, with numeric ordering on
and tertiary strength. Which locale is the caller's question: `cmdr-listing` asks its host's `ReadingLocale` seam
(Cmdr's answer: `intl::active_locale()`) and caches one collator per tag. How the listing sorts with it, and the sort's
measurements: `crates/cmdr-listing/DETAILS.md` § "Collating names".

**Why its own crate.** The listing engine sorts panes with it, and the write engine orders a transfer's tree the same
way (`TreeOrder`, `apps/desktop/src-tauri/src/file_system/write_operations/transfer_tree/order.rs`). Neither should pull
in the other for one type, so the collator sits below both and depends on ICU alone.

## Why a collator and not code-point order

macOS stores whatever bytes a writer hands it, so a single folder holds both spellings of an accented name: Cocoa
writers emit NFD (`a` + U+0301), browsers and most others NFC (U+00E1). Comparing code points decides at that character
and orders by the SPELLING, which put `…Dávid…-signed.pdf` above `…Dáviad…-araw.pdf` in a real `~/Downloads` (5 of its
names were NFD, 7 NFC). Code points also rank by number rather than by category, which stranded every accented letter
after `z` and every `_name` below every `2026-…` name, where Finder, Total Commander, and Explorer all put `_` first.

## The options

- **Numeric ordering on**: digit runs compare by value, so `img_9` precedes `img_10`.
- **Tertiary strength**: case breaks ties between otherwise equal names rather than driving the order, so `apple` sorts
  beside `Apple` and nowhere near `Zebra`.
- **The locale decides accents**: Swedish seats `ö` after `z`; Hungarian and English seat it beside `o`.
- **An unparseable tag falls back to root** (`und`), so a broken setting still sorts. A well-formed but unknown tag
  needs no handling: ICU4X falls back through the locale chain on its own.

## One order, two readings

`NameCollator::compare` collates a pair live; `NameCollator::key` serializes one name's weights to bytes that sort the
same way. The two agree by construction (a sort key IS the collation weights, serialized), and
`tests::a_key_ranks_names_the_way_compare_does` pins it. A bulk sort builds a key per row and compares those; an
incremental insert, which probes a handful of rows, compares live.

**Both readings end with a raw-bytes tiebreak**, because the NFC and NFD spellings of one name carry identical collation
weights and would otherwise compare `Equal`. `compare` applies it itself; a caller comparing keys applies it after
`NameKey::compare`. Without a total order, two sorts of the same names can disagree; what that costs the listing's
diffs: `crates/cmdr-listing/DETAILS.md` § "Collating names".

**❌ Never persist a `NameKey`.** The bytes are ICU4X's internal encoding, and a library or CLDR update may re-tune
them. That is harmless while every key in a comparison came from the running binary, and silently wrong the moment a
stored key meets a fresh one.

## On the `unstable` feature

`write_sort_key_to` sits behind `icu_collator`'s `unstable` flag, which ICU4X exempts from semver (it may change in a
minor release). Every shape that change can take is a compile error, since the only surface used is that one call and
`Vec<u8>`'s upstream `CollationKeySink` impl; `Cargo.lock` is committed, so a break arrives as a red Renovate PR rather
than on `main`. What type checking cannot see is a change to the key BYTES, which is what the never-persist rule and the
ordering tests cover.

## No state

The crate keeps nothing: no cache, no static. Building a collator loads ICU's compiled data, so a caller that sorts
often keeps one per locale (`cmdr-listing`'s `Inner::collator`, a map from tag to `Arc<NameCollator>`). The ICU data
itself is process-wide and read-only.

## Testing

The unit tests in `lib.rs` pin the crate's own promises: `key` ranks the way `compare` does, distinct names never tie, a
broken tag falls back to root, and the locale reaches the collator. The golden name orders a user sees (NFC beside NFD,
`_` first, accents, case) live in `cmdr-listing`'s `collation_test`, because they run through the listing's sort.

## The public surface is capped

`index-crate-isolation` holds this crate to two root promises, no public module, and four public items, exactly what it
exposes:

- `NameCollator`, with `new` (the listing's per-locale cache, and any caller that needs one locale's order), `compare`
  (an incremental insert, a one-off comparison), and `key` (a bulk sort).
- `NameKey`, with `compare` (the bulk sort's comparator).
