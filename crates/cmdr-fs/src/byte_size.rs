//! A byte count as text for a model or a log to read.
//!
//! Like [`pluralize`](crate::pluralize), it lives here because it's a dependency-free leaf that more than one crate's
//! log lines want, not because product copy belongs in a filesystem crate: a pane's sizes are worded by the frontend
//! (`#lib/units`).

/// A byte count for a model or a log to read ("1.5 KiB", "214.3 GiB"). Base 1024, so the symbols are IEC
/// binary (KiB, MiB, GiB, TiB): a symbol always names the base its number was divided by.
pub fn format_size(bytes: u64) -> String {
    const KIB: u64 = 1_024;
    const MIB: u64 = 1_024 * KIB;
    const GIB: u64 = 1_024 * MIB;
    const TIB: u64 = 1_024 * GIB;
    const UNITS: &[(u64, &str)] = &[(TIB, "TiB"), (GIB, "GiB"), (MIB, "MiB"), (KIB, "KiB")];

    for &(threshold, unit) in UNITS {
        if bytes >= threshold {
            let val = bytes as f64 / threshold as f64;
            return if val.fract() == 0.0 {
                format!("{} {unit}", val as u64)
            } else {
                format!("{val:.1} {unit}")
            };
        }
    }
    format!("{bytes} B")
}
