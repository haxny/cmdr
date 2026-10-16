//! A leading `~` in a path a person typed, expanded to their home directory.
//!
//! Only for a path on THIS machine's filesystem: on a share or a phone, `~` is an
//! ordinary folder name, so every caller decides first whether the path is local.

/// Expands a leading `~` (`~` alone or `~/…`) to the user's home directory. Any
/// other path, and any path when the home directory can't be found, comes back
/// unchanged.
pub fn expand_tilde(path: &str) -> String {
    if (path.starts_with("~/") || path == "~")
        && let Some(home) = dirs::home_dir()
    {
        return path.replacen("~", &home.to_string_lossy(), 1);
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_tilde() {
        let expanded = expand_tilde("~/Documents");
        assert!(expanded.starts_with('/'));
        assert!(expanded.contains("Documents"));
        assert!(!expanded.contains('~'));
    }

    #[test]
    fn test_expand_tilde_alone() {
        let expanded = expand_tilde("~");
        assert!(expanded.starts_with('/'));
        assert!(!expanded.contains('~'));
    }

    #[test]
    fn test_no_tilde() {
        let path = "/usr/local/bin";
        assert_eq!(expand_tilde(path), path);
    }
}
