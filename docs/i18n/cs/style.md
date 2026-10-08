# Czech (cs) translation style guide

Working notes for translating Cmdr into Czech. Read `../README.md` for how this fits the translation process, and the
app-wide `docs/style-guide.md` for the English voice these notes carry into Czech.

Well-sourced: the pile (`_ignored/i18n/cs/` in the main clone) has all nine sources: macOS (Finder, AppKit, CoreTypes,
System Settings; highest authority for general macOS terms), Microsoft terminology and the Microsoft Czech style guide,
GNOME Nautilus, Xfce Thunar, KDE Dolphin, and the orthodox two-pane family Total Commander, Double Commander, and
Midnight Commander. Total Commander is the primary reference for two-pane concepts (panel, rychlý filtr, porovnat
adresáře…); macOS Finder for general macOS terms (Složka, Koš, Informace…). The pile's Total Commander files are already
UTF-8 here: never apply how-to-mine.md's windows-1252 re-decode.

This Czech locale is built for the fork owner's private build. Its address form is a deliberate override (below).

## Digest

The must-know rules; the rest of this file elaborates them.

- **Address: informal `ty` (tykání), settled** (fork owner's decision, 2026-10-08). Sentences addressed to the user use
  the 2nd person singular, lowercase: „Opravdu chceš smazat tyto soubory?“, „Stiskni Esc“, „Můžeš to změnit v
  Nastavení.“ Never vykání (`chcete`, `Stiskněte`), never uppercase `Ty`/`Tvůj`. This overrides macOS Czech (mostly
  vykání) and Microsoft (neutral plural) on purpose.
- **Gender: restructure, never hedge.** Tykání past tense is gendered (`smazal jsi` / `smazala jsi`), so never put the
  user into a past participle or an agreeing adjective. Name the action or the object instead: `Zkopírováno`,
  `Soubor byl přesunut do koše`, `Kopírování je hotové`, `Přihlášení proběhlo`, or use present/future tense
  (`Zkopíruješ 3 soubory`, `Tímto smažeš…`). No `smazal(a)`, `byl/a`, `jsi přihlášen(a)`.
- **Register by UI slot**:
  - buttons, menu items, commands: infinitive, like Finder (`Kopírovat`, `Přejmenovat`, `Vysunout`, `Zrušit`), object
    after the verb (`Přidat server…`, `Přesunout do koše`);
  - instructions inside a sentence or hint: imperative 2nd sg (`Stiskni`, `Klikni`, `Vyber`, `Zadej`, `Zkus to znovu`);
  - progress lines: verbal noun or reflexive passive (`Kopírování…`, `Načítá se…`, `Prohledává se disk…`);
  - status chips: terse participles/adjectives in the neuter or agreeing with the named thing (`Čeká`, `Běží`,
    `Pozastaveno`, `Hotovo`, `Nedokončeno`).
- **Voice**: friendly, concise, active, calm. Error copy states the problem and a next step; never a bare `Chyba` or
  `Selhalo` as a label. „Couldn't X“ → `X se nepodařilo` / `Nelze X`; „Something went wrong“ → `Něco se pokazilo`.
- **Capitalization**: sentence case everywhere; only the first word and proper nouns are capitalized.
- **Typography** (`mechanics.json`): quotes `„…“` (U+201E, U+201C), nested `‚…‘`; never `"…"`. Ellipsis is the single
  `…`, hugging its word (`Otevřít…`). One-letter prepositions and conjunctions `k s v z o u a i` (and capitals) take a
  no-break space U+00A0 after them, not a plain space (`v panelu`, `s názvem`). Multipliers: `4×` or `4krát`, never
  `4x`.
- **No hedged grammar**: Czech tempts `soubor(y)`, `smazán(a)`, `byl/a`, `-l(a)`. Use ICU `plural`/`select` when Cmdr
  knows the value, otherwise rephrase.
- **Plurals**: CLDR `one` / `few` / `many` / `other`, write all four. `one` = 1, `few` = 2–4, `many` = DECIMALS only
  (`1,5 souboru`), `other` = 0 and 5+ (`5 souborů`). Keep agreement inside each branch.
- **Placeholders**: a `{name}`, `{path}`, `{volumeName}` has unknown gender and cannot be declined. Keep it in the
  nominative behind a declinable head noun or a colon: `soubor „{name}“`, `ve složce {path}`, `na svazku {volumeName}`,
  `Cíl: {path}`. Never decline the placeholder itself or make a verb agree with it.
- **Brand**: `Cmdr` stays verbatim and may inflect where natural (`v Cmdru`, `Cmdr ti ukáže`); prefer constructions
  where it stays nominative. Keep `macOS`, `GitHub`, `SMB`, `MTP`, Finder, Spotlight verbatim. Apple's localized names
  follow Czech macOS (`Rychlý náhled` for Quick Look, `Informace` for Get Info, `Koš`, `Nastavení systému`).
- **Two-pane terms follow Total Commander** (pane = `panel`, quick filter = `rychlý filtr`), general macOS terms follow
  Finder. Rulings live in `terms.json`; the brief shows the ones in play.
- **Aria labels** must contain the visible label verbatim and in order; pick the label's case form to be the one the
  aria sentence uses.

## Formality: tykání, settled (fork owner's decision, 2026-10-08)

**Address the user as `ty`** (informal, lowercase) throughout. This was an open flag in the first draft of this guide,
which recommended the neutral 2nd-person plural. The fork owner decided for tykání on 2026-10-08, for this private fork
build; the decision is final for this locale.

Evidence it overrides, recorded so nobody relitigates it from the sources: macOS Czech is mostly vykání (in the pile,
`Chcete` 266× vs `Chceš` 9×, `můžete` 237× vs `můžeš` 29×, but `Zadej` 64× vs `Zadejte` 61×: Apple itself is mixed), and
the Microsoft Czech style guide prescribes the neutral plural. Cmdr's English voice is warm and informal, which is the
reason for the override.

Mechanics:

- **Standalone labels (buttons, menu items, commands, setting names): infinitive**, as Finder and Total Commander do
  ("Kopírovat", "Uložit", "Smazat", "Otevřít", "Zrušit", "Odpojit"). The infinitive is address-neutral, so tykání
  doesn't change labels. Avoid bare imperatives as button labels ("Kopíruj", "Ulož").
- **Sentences to the user: 2nd person singular.** "Opravdu chceš smazat tyto soubory?", "Tuto akci nelze vrátit.",
  "Můžeš to kdykoli změnit v Nastavení."
- **Instructions in prose and hints: imperative 2nd sg.** "Stiskni ⏎ pro otevření", "Klikni sem", "Přetáhni soubory
  sem", "Zkus to znovu".
- Possessives lowercase: `tvůj`, `tvoje`, `tvá`, `tvůj Mac`. Never the epistolary uppercase `Ty`/`Tvůj`.

## Voice and tone

Friendly, concise, active, calm; informal tykání carries the warmth, so phrasing can stay plain and everyday without
stiffness. Prefer a verb to a verbal-noun chain ("Hledat", not "Provést vyhledávání"). Error messages stay calm and
actionable: phrase the problem and the next step, and don't use "chyba" (error) or "selhalo" (failed) as a bare status
label the way English avoids "error"/"failed". No apologies where Cmdr made a deliberate choice; where regret is due,
"Bohužel…" or "Promiň" (tykání), never "Sorry".

## Decision points

- **Script: Latin, no decision.** Czech is written in the Latin alphabet with diacritics (á, č, ď, é, ě, í, ň, ó, ř, š,
  ť, ú, ů, ý, ž). Confidence: high.
- **Regional variant: one, `cs` (`cs-CZ`).** Czech is standardized only in Czechia. (Slovak is a separate language,
  `sk`, not a variant.) Confidence: high.
- **Gender / inclusive language: restructure (high).** Czech past tense uses gendered l-participles (-l masc, -la fem),
  and with tykání the 2nd-person singular past ("smazal jsi" / "smazala jsi") forces a gender guess; so do agreeing
  adjectives and passive participles about the user ("jsi přihlášen/přihlášena"). Never guess and never hedge. Instead:
  - impersonal participle or verbal noun: "Zkopírováno", "Přesunuto do koše", "Kopírování dokončeno";
  - the object as subject: "Soubor byl přejmenován", "3 soubory byly smazány" (the object's gender is known from the
    noun, or handled by ICU plural);
  - present or future tense, which is gender-neutral in the 2nd sg: "Smažeš 3 soubory", "Tímto přepíšeš cíl", "Teď
    vidíš…";
  - a state of the thing, not the person: "Přihlášení proběhlo", "Účet je připojený". If no natural restructuring
    exists, flag the key in the report and in `review-queue.md` instead of shipping a hedge.
- **Capitalization: sentence case everywhere (high).** Czech capitalizes only the first word and proper nouns in titles,
  menu items, labels, and buttons ("Zobrazit skryté soubory", not "Zobrazit Skryté Soubory").

## Terminology

Rulings live in `terms.json` (one per concept from `../concepts.json`; schema in `../termbase.md`), with the rationale
in `decisions.md`. Source every ruling from the pile, never guess. Precedence: Total Commander (then Double Commander)
for two-pane concepts Finder lacks; macOS Finder/AppKit for general macOS and file terms; Microsoft as a tiebreak; the
explorer family (Nautilus, Thunar, Dolphin) for general file operations where the first two are silent.

Cross-term notes:

- Czech compounds rarely; English noun stacks become a head noun plus genitive or adjective ("historie operací", "rychlý
  filtr", "nastavení panelu").
- Short labels drop articles naturally (Czech has none) and keep the verb first ("Přidat do oblíbených").

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Finder, Spotlight, AirDrop, plus the
`{system_settings}`-style tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by
`desktop-i18n-dont-translate`; see `apps/desktop/scripts/i18n-catalog-lib.ts`. Quick Look is NOT kept English: Czech
macOS calls it "Rychlý náhled", and Cmdr follows what the user sees. macOS UI names Cmdr opens into must match Czech
macOS ("Koš", "Nastavení systému", "Soukromí a zabezpečení").

## Plurals

CLDR categories for `cs`: `one`, `few`, `many`, `other` (verified with `new Intl.PluralRules('cs')`). Write all four.

- **one**: integer 1 only. "1 soubor".
- **few**: integers 2–4. "2 soubory".
- **many**: any number with a decimal fraction. "1,5 souboru". This is the decimal bucket, not the large-number bucket.
- **other**: everything else, including 0 and 5+ ("5 souborů", "0 souborů").
- **Trap: `many` is the decimal form, not "lots".** Translators from a Polish/Russian background get this backwards.
- Forms map to cases: 1 = nominative sg, 2–4 = nominative pl, 5+/0 = genitive pl, decimals = genitive sg. Keep adjective
  and verb agreement inside each branch ("byl smazán 1 soubor", "byly smazány 2 soubory", "bylo smazáno 5 souborů").

## Notes and decisions

- **Quotation marks: `„…“`** (U+201E low-9 opening, U+201C high-6 closing; macOS cs writes this pair, 839 hits in the
  pile), nested `‚…‘`. Never straight `"` or English `“…”`.
- **No-break space after one-letter prepositions and conjunctions** (`k s v z o u a i`, any case): write U+00A0, not a
  plain space, so the word never ends a line ("v panelu"). Czech typographic norm (ČSN 01 6910); declared in
  `mechanics.json` so the check flags misses. In JSON you may write it as the escape ` ` or the raw character.
- **Numbers and dates come from the formatter layer.** Czech uses a comma decimal and a space thousands separator (1
  000); `formatNumber()` / `formatByteSize()` produce these. Never hardcode separators.
- **Length.** Czech runs longer than English; overflow-check against the pseudolocale (`en-XA`).
- **ICU mechanics**: double every apostrophe in an ICU value (`'` → `''`) and keep every `{placeholder}` and `<tag>`
  verbatim. Full rules: `docs/i18n/translator-instructions.md` and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- Case-by-case rulings go in `decisions.md` under a heading citing their keys.

## Termbase files

`terms.json` (rulings), `decisions.md` (distilled rationale), `mechanics.json` (typography), `review-queue.md` (open
flags for a native reviewer). Schema and tooling: `../termbase.md`.
