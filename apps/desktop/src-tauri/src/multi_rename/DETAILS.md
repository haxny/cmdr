# Multi-Rename Tool: details

Total Commander's Multi-Rename Tool, the semantics taken from its help (TOTALCMD.CHM, TC 11.58) and wiki. Proposed
upstream in vdavid/cmdr#372.

## Pipeline

Per row, in rename order (`position` counts from 0 and is what `[C]` counts):

1. `Mask::render` the name mask and the extension mask over `RowFacts` (name, extension, parent, grandparent, modified
   time in local time, position).
2. `Transform::apply`: search & replace on the name (and the extension with `include_extension`), then the case step on
   both, then Greek to Latin on both, then `remove_diacritics` on both.
3. `name` + `.` + `extension`, or just `name` when the extension renders empty.
4. A row named by hand in Results takes that name instead of steps 1-3.
5. `Compiled::finish`: composed (NFC) when the spec normalizes, a typed name too.

## Placeholders (`mask.rs`)

- Fields with ranges: `[N]` `[E]` `[P]` `[G]`, and a bare range (`[2-5]`) on the full name. `[N1]` one character,
  `[N2-5]`, `[N2,5]` (start, length), `[N2-]`, negative starts count from the end; with a negative start a positive end
  counts from the end too (`[N-8-5]` is 8th-last to 5th-last), as TC documents.
- Counter `[C]` with the sheet's start / step / digits, or inline `[C10+5:3]`, `[C10]`, `[C+5]`, `[C:3]`, `[C100-10]`.
- Date and time of the last modification: `[Y]` `[y]` `[M]` `[D]` `[h]` `[m]` `[s]`, combinable (`[YMD]`, `[hms]`),
  `[d]` ISO date, `[t]` `hh.mm.ss` (TC's country-specific forms would put `:` in names, which macOS shows as `/`).
- Case switches `[U]` `[L]` `[F]` `[n]` apply from where they stand.
- `[[` is a literal `[`.

Not in v1, all planned in #372: `[T4]` EXIF date, alphabetic counters, `[=plugin.field]` / tag fields, `\` to move into
subfolders (the executor's one-parent rule refuses it today), and "next step" chaining.

## Search & replace (`transform.rs`)

- Plain search: case-insensitive unless `case_sensitive`, `*` / `?` wildcards (`*` is lazy), `a|b|c` lists paired with
  `x|y|z` (one replacement serves them all). **Decision: one alternation regex, ONE pass.** Why: chaining the pairs ran
  `a` → `b` → `c` and made a swap a no-op; TC replaces each match once.
- Regex: `$1` groups; `substitute` makes the whole name the expanded replacement when the search matches.
- A broken regex is a spec error (`SpecError::BadRegex`), checked once in `Compiled::new`, never per row.
- `remove_diacritics`: NFD with the combining marks dropped, a table for the letters that don't decompose (`ł` `đ` `ø`
  `ß` `æ` `œ` `þ` `ð` `ı` `ħ` `ŧ`), then NFC. Like foobar2000's `$ascii()`.

## Greek to Latin (`transliterate.rs`)

ELOT 743 (Greek passports and road signs, close to ISO 843 type 2): letter by letter with the digraphs that read as one
sound (`ου` → `ou`; `αυ` / `ευ` / `ηυ` → `av` / `ev` / `iv` before a vowel or voiced consonant, else `af` / `ef` /
`if`; `γγ` `γξ` `γχ` → `ng` `nx` `nch`). Tonos goes; a dialytika splits a pair (`Ταΰγετος` → `Taygetos`). A capital
digraph is capitalized (`Θ` → `Th`), or all caps when the next letter is a capital too. **Decision: ELOT over ISO 843
type 1** (`η` → `ī`): file names want plain ASCII, and ELOT is what Greeks themselves write.

## Unicode normalization

**Decision: names are composed before the mask anyway, so `normalize_unicode` only changes what counts as unchanged**:
off, a name equal to its new one in another Unicode form is `Unchanged` (macOS treats them as one name); on, only the
exact spelling is, so a decomposed name renames to its composed form. That's what a share read from Windows or Linux
needs: macOS and SMB often store `Ž` as `Z` + a combining caron. Renaming between the two forms works on APFS (verified
on macOS 27, `normalizing_renames_a_decomposed_name_to_its_composed_spelling_on_disk`, 2026-10-09).

## Results (`names_file.rs`)

TC's "Edit names": the preview written as one `old<TAB>new` line per row to `$TMPDIR/cmdr-multi-rename/
multi-rename-names.txt`, which the sheet opens in the user's editor and reads back when its window gets focus.

- **Decision: matched by old name, not by line.** Why: a file appearing in the folder between writing and reading
  would shift every line onto the next row. A line whose old name isn't in the batch does nothing; one without a tab
  is skipped. The new name is what follows the LAST tab (an old name may hold one), trimmed.
- **Decision: the backend reads only the file it wrote** (`WRITTEN`). Why: a read-a-path command would read any file
  the frontend names.
- **Decision: plain text over CSV or JSON.** Why: one name per line edits well in any editor, column selection
  included; CSV opens in a spreadsheet that may reformat names, JSON needs escaping.
- Typed names go through the same statuses as computed ones, and the edits travel with apply, so the
  `ExpectedRename` check covers them.

## Preview and apply (`plan.rs`, `run.rs`)

- Row statuses: `Ready`, `Unchanged` (same name), `InvalidName` (via `validate_filename`, plus `.` / `..`),
  `Duplicate` (two rows get one folded name), `TargetExists` (a sibling that STAYS holds the folded name; siblings
  include hidden entries). A batch row renaming away frees its name, so chains and swaps preview as ready.
- **Decision: apply recomputes, then requires the user's preview.** The frontend sends listing id + row numbers + spec
  + the ready rows it showed; apply recomputes from the listing and refuses with `PreviewOutOfDate` unless its ready
  rows are exactly those. Why: row numbers shift when a file appears above them, and renaming "row 7" would then rename
  a file the user never saw; names come from the backend, the frontend's only confirm them.
- Plain search: a `*` is lazy (`IMG_*_` ends at the first `_`) except a trailing one, which runs to the end. Search
  and names are composed (NFC) first, so `é` typed finds the decomposed `é` an SMB share stores.
- A single search takes its replacement literally (`|` included); only a list pairs with a list. A list of nothing
  (`|`) searches for nothing.
- Apply captures a `SourceFingerprint` per ready row (local on `root`, the volume's elsewhere) and calls
  `start_renames(.., Initiator::User)`: the executor orders chains, swaps through temp names, rechecks fingerprints,
  journals every hop (so `undo_operations` reverses it), and runs a copying rename (S3) as one move.
- The preview runs off the IPC thread with a 5 s deadline.

## Presets (`presets.rs`)

`RecentsFile<MultiRenamePreset>` in `multi-rename-presets.json`, keyed by the trimmed, lowercased name: saving under a
taken name replaces it. Built-in presets (No change, Remove diacritics, Greek to Latin, Normalize Unicode) live in the
frontend (`spec.ts`) so their names are translated. A field added later carries `#[serde(default)]`, so an older
preset still loads.

The field history is `RecentsFile<FieldHistoryEntry>` in `multi-rename-history.json`, all four fields in one list
(200 entries), deduped by field and value. `apply_multi_rename` adds the spec's fields after a successful start
(`history_entries`: a field at its no-change default or empty is skipped, and a replacement only beside a search).

The last settings are a one-entry `RecentsFile<LastSpec>` in `multi-rename-last.json`: the sheet saves them when it
closes and opens on them, as TC does. "No change" in the presets menu resets.
