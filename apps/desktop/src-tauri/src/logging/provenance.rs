//! Which code wrote a log line, decided when the file chain writes it and read back when an
//! error report picks the lines it ships.
//!
//! An error report carries DEBUG and TRACE only from our own crates: a third-party crate's debug
//! output (`mdns_sd`'s service dumps, `smb2`'s positional paths) prints names in shapes no
//! redactor can bound. INFO and above ship from anyone, redacted.
//!
//! The verdict comes from `record.module_path()`, ❗ never from the target: our sites use short
//! custom targets (`copy`, `open_with`) and the file strips `cmdr_lib::`, so a third-party crate's
//! target can read exactly like ours. The file line carries it as [`OWN_LINE_TAG`] in front of the
//! target, on OUR lines. A line without the tag counts as third-party, which is what fails closed
//! for a rotated file an older build wrote, or one a downgraded build appends to. Cost: our own
//! DEBUG lines from before the upgrade stay out of the first few reports.

/// Written right before the target of every line our own code logs:
/// `2026-10-19T10:00:00.000+02:00 DEBUG ·indexing::writer  …`.
pub(crate) const OWN_LINE_TAG: &str = "·";

/// Third-party modules whose lines never reach a report, at any level. `smb2` 0.32.0 logs
/// `durable: {path} reclaimed on the new session…` at INFO, WARN, and ERROR with a share-relative
/// path in prose, which nothing can tell from the words around it. Goes when `smb2` logs its paths
/// as quoted fields (#491).
const WITHHELD_TARGETS: &[&str] = &["smb2::client::durable"];

/// Whether a record's module belongs to our code: the app (`cmdr_lib`, the `Cmdr` binary), every
/// `cmdr_*` workspace crate, and `mtp_rs` (ours, and it logs no path or name). Frontend lines count:
/// `commands/logging.rs` logs them. A record with no module path (a `tracing` bridge, say) doesn't.
pub(crate) fn is_own_module(module_path: Option<&str>) -> bool {
    let Some(path) = module_path else { return false };
    let krate = path.split("::").next().unwrap_or(path);
    krate.starts_with("cmdr_") || krate == "Cmdr" || krate == "mtp_rs"
}

/// The tag the file chain writes in front of a record's target: [`OWN_LINE_TAG`] or nothing.
pub(crate) fn line_tag(module_path: Option<&str>) -> &'static str {
    if is_own_module(module_path) { OWN_LINE_TAG } else { "" }
}

/// Whether an error report may ship the record a file line HEADS (one that starts with the file
/// chain's ISO timestamp). Its continuation lines follow the same verdict. Fails closed: a header
/// whose level doesn't parse drops.
///
/// Reads the shape the file chain writes: `<iso-ts> <LEVEL> [(<ram>) ][·]<target>  <message>`.
pub(crate) fn header_ships_in_report(line: &str) -> bool {
    // The stamp is 29 ASCII bytes (`tail_walker::parse_leading_iso8601`), then one space.
    let Some(rest) = line.get(29..).and_then(|rest| rest.strip_prefix(' ')) else {
        return false;
    };
    let Some((level, rest)) = rest.split_once(' ') else {
        return false;
    };
    let rest = rest.trim_start();
    // The optional RAM gauge, `(374 MB) `.
    let rest = match rest.strip_prefix('(') {
        Some(gauge) => gauge.split_once(") ").map_or(rest, |(_, after)| after),
        None => rest,
    };
    let own = rest.starts_with(OWN_LINE_TAG);
    let target = rest
        .strip_prefix(OWN_LINE_TAG)
        .unwrap_or(rest)
        .split("  ")
        .next()
        .unwrap_or_default();
    let withheld = WITHHELD_TARGETS
        .iter()
        .any(|w| target == *w || target.strip_prefix(w).is_some_and(|tail| tail.starts_with("::")));
    if withheld {
        return false;
    }
    match level {
        "ERROR" | "WARN" | "INFO" => true,
        "DEBUG" | "TRACE" => own,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TS: &str = "2026-10-19T10:00:00.000+02:00";

    #[test]
    fn our_crates_are_ours_and_nothing_else_is() {
        for own in [
            "cmdr_lib::commands::logging",
            "cmdr_index::indexing::writer",
            "cmdr_operations",
            "Cmdr",
            "mtp_rs::ptp",
        ] {
            assert!(is_own_module(Some(own)), "{own}");
        }
        for foreign in [
            "mdns_sd::service_daemon",
            "smb2::client::tree",
            "tracing::span",
            "cmdr",
            "mtp",
        ] {
            assert!(!is_own_module(Some(foreign)), "{foreign}");
        }
        assert!(!is_own_module(None), "no module path is no proof");
    }

    /// Our DEBUG ships under any target, a short custom one and a frontend one included; a
    /// line without the tag doesn't, even when its target reads exactly like ours.
    #[test]
    fn only_tagged_debug_and_trace_ship() {
        let ships = |rest: &str| header_ships_in_report(&format!("{TS} {rest}"));
        assert!(ships("DEBUG ·copy  copied 3 files"));
        assert!(ships("TRACE ·indexing::writer  batch"));
        assert!(ships("DEBUG ·FE:fileExplorer  loaded"));
        assert!(
            ships("DEBUG (374 MB) ·copy  copied 3 files"),
            "the RAM gauge sits before the tag"
        );
        assert!(
            !ships("DEBUG copy  copied 3 files"),
            "untagged: a third party, or an older build"
        );
        assert!(!ships("DEBUG mdns_sd::service_daemon  instance dump"));
        assert!(!ships(
            "TRACE (374 MB) smb2::client::tree  tree: exclusive delete of Photos/x.jpg refused"
        ));
    }

    /// INFO and above ship from anyone; the redactor handles them.
    #[test]
    fn info_and_above_ship_from_anyone() {
        let ships = |rest: &str| header_ships_in_report(&format!("{TS} {rest}"));
        assert!(ships(
            "INFO  smb2::client::tree  tree: connected share=Family Photos, tree_id=7"
        ));
        assert!(ships("WARN  copy  untagged warning from a pre-upgrade file"));
        assert!(ships("ERROR ·copy  copy failed"));
    }

    /// `smb2`'s durable-handle records print a relative path in prose at every level.
    #[test]
    fn smb2_durable_records_never_ship() {
        let ships = |rest: &str| header_ships_in_report(&format!("{TS} {rest}"));
        assert!(!ships(
            "INFO  smb2::client::durable  durable: Photos/Summer (Lake House)/x.jpg reclaimed on the new session"
        ));
        assert!(!ships(
            "ERROR smb2::client::durable  durable: REFUSING a reclaimed handle for Photos/x.jpg"
        ));
        assert!(ships("INFO  smb2::client::durable_extra  a different module"));
    }

    #[test]
    fn a_header_that_does_not_parse_fails_closed() {
        assert!(!header_ships_in_report(&format!("{TS} NEW line with no level")));
        assert!(!header_ships_in_report(&format!("{TS} ")));
        assert!(!header_ships_in_report(TS));
    }
}
