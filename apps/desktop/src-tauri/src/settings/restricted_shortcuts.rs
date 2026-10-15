//! The custom keyboard shortcuts a restricted-capability window (the viewer) reads.
//!
//! The viewer has no `tauri-plugin-store` capability by security design (see
//! `capabilities/CLAUDE.md` § viewer), so it can't load `shortcuts.json` itself. Its keys
//! are in the shortcuts registry (scope `Viewer window`) so they can be remapped or
//! turned off (WCAG 2.1.4), and this read-only snapshot is how it learns a rebinding.
//! Live changes after open arrive through the cross-window `shortcuts:changed` event,
//! which the viewer may listen to. There is no write path: rebinding happens in
//! Settings, which owns the store.

use std::collections::BTreeMap;
use std::fs;

/// Store keys holding a customization look like `shortcut:<commandId>`.
const SHORTCUT_KEY_PREFIX: &str = "shortcut:";

/// Every customized command in `shortcuts.json`, as command id → its shortcuts. Read
/// fresh on every call. Missing or unreadable file reads as "nothing customized", so the
/// window falls back to the registry defaults.
pub fn load_custom_shortcuts<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> BTreeMap<String, Vec<String>> {
    let Ok(data_dir) = crate::config::resolved_app_data_dir(app) else {
        return BTreeMap::new();
    };
    let Ok(contents) = fs::read_to_string(data_dir.join("shortcuts.json")) else {
        return BTreeMap::new();
    };
    parse_custom_shortcuts(&contents)
}

/// Pure parse step for [`load_custom_shortcuts`]. Keeps only `shortcut:*` keys whose value
/// is an array, and only the strings in it. An empty array is kept: it means "the user
/// removed every shortcut", which differs from no entry (the registry default). The
/// frontend's `seedCustomShortcuts` heals the rest, as it does for its own store reads.
pub(crate) fn parse_custom_shortcuts(contents: &str) -> BTreeMap<String, Vec<String>> {
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(contents) else {
        return BTreeMap::new();
    };
    map.into_iter()
        .filter_map(|(key, value)| {
            let command_id = key.strip_prefix(SHORTCUT_KEY_PREFIX)?.to_string();
            let shortcuts = value
                .as_array()?
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            Some((command_id, shortcuts))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_customized_commands_and_skips_other_keys() {
        let json = r#"{
            "_schemaVersion": 1,
            "shortcut:viewer.toggleWordWrap": ["⌥W"],
            "shortcut:viewer.toggleTail": [],
            "shortcut:tab.new": ["⌘T", "⌘N"]
        }"#;
        let parsed = parse_custom_shortcuts(json);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed["viewer.toggleWordWrap"], vec!["⌥W"]);
        assert_eq!(parsed["tab.new"], vec!["⌘T", "⌘N"]);
    }

    #[test]
    fn keeps_an_empty_list_because_it_means_turned_off() {
        let parsed = parse_custom_shortcuts(r#"{ "shortcut:viewer.toggleTail": [] }"#);
        assert_eq!(parsed.get("viewer.toggleTail"), Some(&Vec::new()));
    }

    #[test]
    fn drops_values_that_are_not_string_lists() {
        let json = r#"{
            "shortcut:a": "W",
            "shortcut:b": ["W", 3, null],
            "shortcut:c": { "x": 1 }
        }"#;
        let parsed = parse_custom_shortcuts(json);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed["b"], vec!["W"]);
    }

    #[test]
    fn unreadable_json_reads_as_nothing_customized() {
        assert!(parse_custom_shortcuts("not json").is_empty());
        assert!(parse_custom_shortcuts("[1, 2]").is_empty());
    }
}
