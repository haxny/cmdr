package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// unusedKeyDynamicPrefixes is the closed, documented allowlist of message-key
// prefixes that are assembled at runtime, so their keys never appear as a literal
// in source and a naive scan would wrongly flag them as orphans. EVERY entry must
// map to a real runtime-construction site; a stale entry (no catalog key under
// it) fails the check (see unusedDynamicPrefixesWithoutKeys). Don't widen this to
// silence an orphan: a genuine orphan is dead translation work to delete, not to
// allowlist.
//
// Construction sites (apps/desktop/src/lib/), each building keys under one prefix
// from an enum/reason variable spliced into the dotted path:
//   - errors.git.: git-error-messages.ts builds getMessage(`errors.git.${kind}.{title,message,suggestion}`)
//   - errors.listing.: listing-error-messages.ts builds `errors.listing.${reason}.${part}`
//   - errors.provider.: provider-error-messages.ts builds getMessage(`errors.provider.${p}.…`) and `…appBased.${cat}`
//   - errors.write.: transfer-error-messages.ts builds getMessage(`errors.write.${key}`)
//
// The same dynamic-key set is the codegen's sanctioned non-fatal dead-key case
// (see docs/guides/i18n.md and apps/desktop/src/lib/intl/messages/DETAILS.md).
var unusedKeyDynamicPrefixes = []string{
	"errors.git.",
	"errors.listing.",
	"errors.provider.",
	"errors.write.",
}

// RunDesktopMessageKeysUnused fails if any key in the `messages/en/*.json`
// catalogs is never referenced anywhere in the app's source: an orphan key is
// dead translation work that costs real money once human translators are
// involved. A key counts as referenced if its literal dotted string appears in
// any `.ts`/`.svelte` file under `apps/desktop/src/` (covering direct accessor
// calls, `<Trans key>`, and indirection like registry `*Key` fields and Record
// maps that store the literal) OR in any `.rs` file under
// `apps/desktop/src-tauri/src/`. Keys under an allowlisted dynamic prefix
// (`unusedKeyDynamicPrefixes`) are treated as referenced, since they're built at
// runtime and never appear verbatim.
//
// The Rust half exists because three surfaces resolve their copy in Rust, not
// the webview: the native menu bar (built during `setup`), the main window's
// title, and the already-running alert (which fires before a webview exists).
// Their keys reach `menu_t` as literals, so a scanner that reads only TypeScript
// sees ~130 orphans. ❌ Allowlisting `menu.` instead would have been wrong: these
// keys aren't built at runtime, they're used from a language the scanner
// couldn't read, and the allowlist would then hide a genuinely dead menu key
// forever.
//
// Test references count as references on purpose: a key used only in a test is
// rare, and counting it avoids false positives that would block the build on a
// key that does have a (test) call site. The companion `desktop-message-keys-fresh`
// check owns the inverse direction (keys referenced in code but missing from the
// catalog).
func RunDesktopMessageKeysUnused(ctx *CheckContext) (CheckResult, error) {
	messagesDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src", "lib", "intl", "messages", "en")
	srcDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src")
	rustSrcDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src-tauri", "src")

	catalogKeys, err := collectUnusedCheckCatalogKeys(messagesDir)
	if err != nil {
		if os.IsNotExist(err) {
			return Success("no message catalogs yet"), nil
		}
		return CheckResult{}, err
	}

	// A stale dynamic-prefix allowlist entry (no catalog key under it) means the
	// allowlist drifted from the construction sites: surface it rather than let it
	// silently mask future orphans.
	if stale := unusedDynamicPrefixesWithoutKeys(catalogKeys, unusedKeyDynamicPrefixes); len(stale) > 0 {
		return CheckResult{}, fmt.Errorf(
			"%d dynamic-key %s in `unusedKeyDynamicPrefixes` no longer match any catalog key (stale allowlist; remove or fix):\n  %s",
			len(stale), Pluralize(len(stale), "prefix", "prefixes"), strings.Join(stale, "\n  "),
		)
	}

	sources, scanned, err := readUnusedCheckSources(srcDir, ".ts", ".svelte")
	if err != nil {
		return CheckResult{}, err
	}
	rustSources, rustScanned, err := readUnusedCheckSources(rustSrcDir, ".rs")
	if err != nil {
		return CheckResult{}, err
	}
	sources = append(sources, rustSources...)
	scanned += rustScanned

	unused := findUnusedMessageKeys(catalogKeys, sources, unusedKeyDynamicPrefixes)
	if len(unused) > 0 {
		var sb strings.Builder
		for _, k := range unused {
			sb.WriteString("  ")
			sb.WriteString(k)
			sb.WriteString("\n")
		}
		return CheckResult{}, fmt.Errorf(
			"found %d orphan message %s never referenced in `apps/desktop/src/` (dead translation work). "+
				"Remove the key and its `@key` metadata sibling from the catalog and run `pnpm intl:keys`. "+
				"If a key is intentionally unwired (a feature mid-build) or built at runtime, wire it to a call site "+
				"or add its prefix to `unusedKeyDynamicPrefixes` with a construction-site comment:\n%s",
			len(unused), Pluralize(len(unused), "key", "keys"), strings.TrimRight(sb.String(), "\n"),
		)
	}

	return Success(fmt.Sprintf(
		"%d message %s all referenced (%d source %s scanned)",
		len(catalogKeys), Pluralize(len(catalogKeys), "key", "keys"),
		scanned, Pluralize(scanned, "file", "files"),
	)), nil
}

// collectUnusedCheckCatalogKeys reads every `en/*.json` catalog and returns the
// sorted, deduped set of renderable message keys, dropping ARB-style `@key`
// metadata entries, matching `gen-message-keys-lib.js`'s collectCatalogKeys.
func collectUnusedCheckCatalogKeys(messagesDir string) ([]string, error) {
	entries, err := os.ReadDir(messagesDir)
	if err != nil {
		return nil, err
	}
	seen := map[string]bool{}
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".json") {
			continue
		}
		path := filepath.Join(messagesDir, entry.Name())
		data, readErr := os.ReadFile(path)
		if readErr != nil {
			return nil, fmt.Errorf("couldn't read %s: %w", path, readErr)
		}
		var parsed map[string]json.RawMessage
		if jsonErr := json.Unmarshal(data, &parsed); jsonErr != nil {
			return nil, fmt.Errorf("couldn't parse %s: %w", path, jsonErr)
		}
		for key := range parsed {
			if strings.HasPrefix(key, "@") {
				continue
			}
			seen[key] = true
		}
	}
	keys := make([]string, 0, len(seen))
	for k := range seen {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys, nil
}

// readUnusedCheckSources reads every file under srcDir with one of `exts`
// (skipping node_modules / build dirs and the generated `keys.gen.ts`, whose
// content lists the catalog keys rather than using them) and returns their
// contents plus the count scanned. Test files ARE included (test references
// count as references).
//
// Called twice: once for the frontend (`.ts` / `.svelte`) and once for the Rust
// crate (`.rs`), whose `menu_t` call sites are the only reference the native
// menu-bar keys have.
func readUnusedCheckSources(srcDir string, exts ...string) ([]string, int, error) {
	wanted := map[string]bool{}
	for _, ext := range exts {
		wanted[ext] = true
	}
	var sources []string
	scanned := 0
	err := corpus.WalkDir(srcDir, func(path string, d os.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if d.IsDir() {
			if d.Name() == "node_modules" || d.Name() == "target" || d.Name() == ".svelte-kit" {
				return filepath.SkipDir
			}
			return nil
		}
		if !wanted[filepath.Ext(d.Name())] {
			return nil
		}
		if d.Name() == "keys.gen.ts" {
			return nil
		}
		data, readErr := corpus.ReadFile(path)
		if readErr != nil {
			return readErr
		}
		sources = append(sources, data)
		scanned++
		return nil
	})
	if err != nil {
		return nil, 0, fmt.Errorf("couldn't scan %s: %w", srcDir, err)
	}
	return sources, scanned, nil
}

// findUnusedMessageKeys returns the sorted catalog keys whose literal text
// appears in no source file and that aren't covered by an allowlisted dynamic
// prefix. Using a substring scan (not a strict accessor-call regex) is
// deliberate: it credits any indirection that stores the literal (registry
// `*Key` fields, Record maps), so the only keys flagged are genuinely absent
// ones.
func findUnusedMessageKeys(catalogKeys, sources, dynamicPrefixes []string) []string {
	var scanned []string
	for _, key := range catalogKeys {
		if !hasAnyPrefix(key, dynamicPrefixes) {
			scanned = append(scanned, key)
		}
	}
	mentioned := newKeyMentionMatcher(scanned).mentioned(sources)
	var unused []string
	for _, key := range scanned {
		if !mentioned[key] {
			unused = append(unused, key)
		}
	}
	sort.Strings(unused)
	return unused
}

// keyMentionMatcher finds every key that occurs as a substring of a text in ONE pass over the
// text, whatever the key count: an Aho-Corasick automaton compiled to a dense DFA over the byte
// classes the keys use (every other byte is class 0 and leads back to the root). A
// `strings.Contains` per key per file cost ~20 s on the real tree (5,000+ keys × 2,000+ files).
type keyMentionMatcher struct {
	keys    []string
	class   [256]int32 // byte → class; 0 = a byte no key contains
	classes int32
	next    []int32 // next[state*classes+class] → state
	// out holds the key indexes ending exactly at a state; dict links a state to the nearest
	// proper-suffix state that ends a key (-1 for none), so one visit reports every key ending here.
	out  [][]int
	dict []int32
}

func newKeyMentionMatcher(keys []string) *keyMentionMatcher {
	m := &keyMentionMatcher{keys: keys, classes: 1}
	for _, k := range keys {
		for i := 0; i < len(k); i++ {
			if m.class[k[i]] == 0 {
				m.class[k[i]] = m.classes
				m.classes++
			}
		}
	}
	// Build the trie: state 0 is the root, -1 marks a missing edge until the BFS fills it.
	newState := func() int32 {
		for range m.classes {
			m.next = append(m.next, -1)
		}
		m.out = append(m.out, nil)
		m.dict = append(m.dict, -1)
		return int32(len(m.out) - 1)
	}
	newState()
	for idx, k := range keys {
		s := int32(0)
		for i := 0; i < len(k); i++ {
			edge := s*m.classes + m.class[k[i]]
			if m.next[edge] < 0 {
				child := newState()
				m.next[edge] = child
			}
			s = m.next[edge]
		}
		m.out[s] = append(m.out[s], idx)
	}
	// BFS: complete every edge into a DFA transition and compute the dictionary-suffix links.
	fail := make([]int32, len(m.out))
	queue := make([]int32, 0, len(m.out))
	for c := range m.classes {
		edge := m.next[c]
		if edge <= 0 || c == 0 {
			m.next[c] = 0
			continue
		}
		fail[edge] = 0
		queue = append(queue, edge)
	}
	for head := 0; head < len(queue); head++ {
		s := queue[head]
		f := fail[s]
		if len(m.out[f]) > 0 {
			m.dict[s] = f
		} else {
			m.dict[s] = m.dict[f]
		}
		for c := int32(1); c < m.classes; c++ {
			edge := s*m.classes + c
			if child := m.next[edge]; child > 0 {
				fail[child] = m.next[f*m.classes+c]
				queue = append(queue, child)
			} else {
				m.next[edge] = m.next[f*m.classes+c]
			}
		}
		m.next[s*m.classes] = 0
	}
	return m
}

// mentioned reports, for each key, whether it occurs in any of the texts.
func (m *keyMentionMatcher) mentioned(texts []string) map[string]bool {
	seen := make([]bool, len(m.out)) // a state whose whole dict chain is already reported
	found := make(map[string]bool, len(m.keys))
	for _, text := range texts {
		s := int32(0)
		for i := 0; i < len(text); i++ {
			s = m.next[s*m.classes+m.class[text[i]]]
			for t := s; t > 0 && !seen[t]; t = m.dict[t] {
				seen[t] = true
				for _, idx := range m.out[t] {
					found[m.keys[idx]] = true
				}
			}
		}
	}
	return found
}

// unusedDynamicPrefixesWithoutKeys returns the allowlisted prefixes that no
// catalog key starts with: a stale entry that should be removed so the dynamic
// allowlist stays honest (each prefix tied to a live construction site).
func unusedDynamicPrefixesWithoutKeys(catalogKeys, dynamicPrefixes []string) []string {
	var stale []string
	for _, prefix := range dynamicPrefixes {
		covered := false
		for _, key := range catalogKeys {
			if strings.HasPrefix(key, prefix) {
				covered = true
				break
			}
		}
		if !covered {
			stale = append(stale, prefix)
		}
	}
	sort.Strings(stale)
	return stale
}

func hasAnyPrefix(s string, prefixes []string) bool {
	for _, p := range prefixes {
		if strings.HasPrefix(s, p) {
			return true
		}
	}
	return false
}
