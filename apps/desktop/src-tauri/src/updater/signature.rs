//! Minisign signature verification for update tarballs.
//!
//! Both the public keys and per-release signatures are double-encoded:
//! base64(minisign-text-format). Decode the outer base64 first, then parse
//! with the minisign-verify crate. This matches Tauri's internal behavior.

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use minisign_verify::{PublicKey, Signature};

/// The main updater key, base64-encoded: the one every release is signed with. A copy of
/// `plugins.updater.pubkey` in `tauri.conf.json` (the bundler needs it there), pinned equal by a test.
const MAIN_PUBKEY_BASE64: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEEyNjAxRjM2QkIxNjhDMEEK\
     UldRS2pCYTdOaDlnb2lwTE5wZUNVZE5FTTBScXRrTWJPb1EvN3J2Q0FVcC9JNzJkSmphUTl3V3MK";

/// The offline backup updater key, base64-encoded. Its private half never goes online; it signs
/// exactly one release, the one that rotates a leaked main key out. `updater-backup.pub` is its one
/// home, read here and by `scripts/release-finish` (`DETAILS.md` § Two updater keys).
const BACKUP_PUBKEY_BASE64: &str = include_str!("../../updater-backup.pub");

/// Every key an update may be signed with.
const TRUSTED_PUBKEYS: [&str; 2] = [MAIN_PUBKEY_BASE64, BACKUP_PUBKEY_BASE64];

/// Verifies that `data` was signed with one of the trusted keys.
///
/// `signature_base64` is the base64-encoded minisign signature string from `latest.json`.
pub fn verify(data: &[u8], signature_base64: &str) -> Result<(), String> {
    verify_with(&TRUSTED_PUBKEYS, data, signature_base64)
}

/// The check against an explicit key list, so tests can sign with a throwaway key.
fn verify_with(pubkeys: &[&str], data: &[u8], signature_base64: &str) -> Result<(), String> {
    let sig_text = decode_base64_text(signature_base64).map_err(|e| format!("Couldn't decode signature: {e}"))?;
    let signature = Signature::decode(&sig_text).map_err(|e| format!("Couldn't parse signature: {e}"))?;

    let mut refusals = Vec::with_capacity(pubkeys.len());
    for pubkey in pubkeys {
        let pubkey_text = decode_base64_text(pubkey).map_err(|e| format!("Couldn't decode public key: {e}"))?;
        let public_key = PublicKey::decode(&pubkey_text).map_err(|e| format!("Couldn't parse public key: {e}"))?;
        // allow_legacy=true matches Tauri's behavior. A key whose ID isn't the signature's refuses
        // it up front, so trying each key costs one real check, not two.
        match public_key.verify(data, &signature, true) {
            Ok(()) => return Ok(()),
            Err(e) => refusals.push(e.to_string()),
        }
    }
    Err(format!(
        "Signature verification didn't pass against any trusted key: {}",
        refusals.join("; ")
    ))
}

/// Decodes a base64 string into a UTF-8 string (the inner minisign text format).
fn decode_base64_text(b64: &str) -> Result<String, String> {
    let bytes = BASE64_STANDARD
        .decode(b64.trim())
        .map_err(|e| format!("base64 decode: {e}"))?;
    String::from_utf8(bytes).map_err(|e| format!("UTF-8 decode: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_PUB: &str = include_str!("testdata/fixture.pub");
    const PAYLOAD: &[u8] = include_bytes!("testdata/payload.txt");
    const PAYLOAD_SIG: &str = include_str!("testdata/payload.txt.sig");

    /// A key's ID the way minisign prints it (little-endian hex), read from the key itself rather
    /// than from its untrusted comment.
    fn key_id(pubkey_base64: &str) -> String {
        let text = decode_base64_text(pubkey_base64).unwrap();
        let body = BASE64_STANDARD.decode(text.lines().nth(1).unwrap()).unwrap();
        body[2..10].iter().rev().map(|b| format!("{b:02X}")).collect()
    }

    #[test]
    fn the_trusted_keys_are_the_main_key_then_the_backup() {
        let ids: Vec<String> = TRUSTED_PUBKEYS.iter().map(|k| key_id(k)).collect();
        assert_eq!(ids, ["A2601F36BB168C0A", "B72B6B32B7D71B58"]);
        for key in TRUSTED_PUBKEYS {
            assert!(PublicKey::decode(&decode_base64_text(key).unwrap()).is_ok());
        }
    }

    /// The bundler reads the main key from `tauri.conf.json`; the updater must trust the same one.
    #[test]
    fn the_main_key_is_the_one_in_tauri_conf() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
        assert_eq!(conf["plugins"]["updater"]["pubkey"].as_str(), Some(MAIN_PUBKEY_BASE64));
    }

    #[test]
    fn a_signature_from_any_trusted_key_verifies() {
        assert_eq!(verify_with(&[FIXTURE_PUB], PAYLOAD, PAYLOAD_SIG), Ok(()));
        // The signing key in second place, as the backup sits.
        assert_eq!(
            verify_with(&[MAIN_PUBKEY_BASE64, FIXTURE_PUB], PAYLOAD, PAYLOAD_SIG),
            Ok(())
        );
    }

    #[test]
    fn a_signature_from_an_untrusted_key_is_refused() {
        assert!(verify_with(&TRUSTED_PUBKEYS, PAYLOAD, PAYLOAD_SIG).is_err());
        assert!(verify(PAYLOAD, PAYLOAD_SIG).is_err());
    }

    #[test]
    fn a_changed_payload_is_refused() {
        assert!(verify_with(&[FIXTURE_PUB], b"Cmdr updater test payload, edited\n", PAYLOAD_SIG).is_err());
    }

    #[test]
    fn garbage_is_refused() {
        for sig in ["", "%%%", &BASE64_STANDARD.encode("one line only\n")] {
            assert!(verify_with(&[FIXTURE_PUB], PAYLOAD, sig).is_err(), "{sig:?}");
        }
    }
}
