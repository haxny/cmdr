//! Which bundle identifier this binary was built for, shared by `build.rs` (which picks it) and
//! `config` (which hands it to every handle-less path).
//!
//! `tauri build --config <file>` passes the merged override to cargo as the `TAURI_CONFIG` JSON
//! (`tauri-build` reads the same variable). A fork that builds under its own identity
//! (`.github/haxny/build.conf.json`: `com.haxny.cmdr`) must move every handle-less store with it,
//! or its favorites, secrets file, drive index, and logs land in official Cmdr's folders while
//! Tauri's own `app_data_dir()` (and the instance lock) sit in the fork's. Identifiers inside the
//! `com.veszelovszki.cmdr` family (dev, worktree, E2E, `.test`) keep the upstream id, as they always
//! did: those runs name their folders through `CMDR_DATA_DIR` / `CMDR_CACHE_DIR` instead.
//!
//! Included by `build.rs` through `#[path]`, so it may use only `std` and `serde_json`.

/// The identifier official Cmdr ships under (`tauri.conf.json` § `identifier`).
pub const UPSTREAM_BUNDLE_ID: &str = "com.veszelovszki.cmdr";

/// The bundle id for `config::BUNDLE_ID`, from the `TAURI_CONFIG` override (if any): its
/// top-level `identifier` when that names a different product, else [`UPSTREAM_BUNDLE_ID`].
/// Garbled JSON or a missing key reads as "no override", the same as `tauri.conf.json` alone.
#[cfg_attr(
    not(test),
    allow(dead_code, reason = "called from build.rs, which includes this file")
)]
pub fn pick_bundle_id(tauri_config_json: Option<&str>) -> String {
    tauri_config_json
        .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
        .and_then(|config| config.get("identifier")?.as_str().map(str::to_owned))
        .filter(|id| is_valid_identifier(id) && !id.starts_with(UPSTREAM_BUNDLE_ID))
        .unwrap_or_else(|| UPSTREAM_BUNDLE_ID.to_owned())
}

/// Tauri's own rule for `identifier`: alphanumerics, hyphens, and periods. Anything else would
/// end up in a path, so it never gets that far.
fn is_valid_identifier(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_override_keeps_upstream() {
        assert_eq!(pick_bundle_id(None), UPSTREAM_BUNDLE_ID);
    }

    #[test]
    fn a_fork_identifier_moves_the_bundle_id() {
        let json = r#"{"productName":"Cmdr haxny","identifier":"com.haxny.cmdr"}"#;
        assert_eq!(pick_bundle_id(Some(json)), "com.haxny.cmdr");
    }

    #[test]
    fn the_upstream_family_keeps_upstream() {
        for id in [
            "com.veszelovszki.cmdr",
            "com.veszelovszki.cmdr-dev-foo",
            "com.veszelovszki.cmdr.test",
        ] {
            let json = format!(r#"{{"identifier":"{id}"}}"#);
            assert_eq!(pick_bundle_id(Some(&json)), UPSTREAM_BUNDLE_ID, "{id}");
        }
    }

    #[test]
    fn an_override_without_identifier_keeps_upstream() {
        let json = r#"{"bundle":{"createUpdaterArtifacts":false}}"#;
        assert_eq!(pick_bundle_id(Some(json)), UPSTREAM_BUNDLE_ID);
    }

    #[test]
    fn a_nested_identifier_is_not_the_bundle_id() {
        let json = r#"{"plugins":{"x":{"identifier":"com.other.app"}}}"#;
        assert_eq!(pick_bundle_id(Some(json)), UPSTREAM_BUNDLE_ID);
    }

    #[test]
    fn garbled_or_unsafe_input_keeps_upstream() {
        assert_eq!(pick_bundle_id(Some("not json")), UPSTREAM_BUNDLE_ID);
        assert_eq!(
            pick_bundle_id(Some(r#"{"identifier":"../../etc"}"#)),
            UPSTREAM_BUNDLE_ID
        );
        assert_eq!(pick_bundle_id(Some(r#"{"identifier":""}"#)), UPSTREAM_BUNDLE_ID);
        assert_eq!(pick_bundle_id(Some(r#"{"identifier":42}"#)), UPSTREAM_BUNDLE_ID);
    }

    /// Test builds never see a `TAURI_CONFIG`, so the compiled-in id is upstream's: the managed
    /// policy and PPPC profile tests compare it with the published documents.
    #[test]
    fn test_builds_carry_the_upstream_id() {
        assert_eq!(crate::config::BUNDLE_ID, UPSTREAM_BUNDLE_ID);
        assert!(!crate::config::is_fork_build());
    }
}
