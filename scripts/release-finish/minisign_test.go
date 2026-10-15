package main

import (
	"bytes"
	"encoding/base64"
	"errors"
	"os"
	"slices"
	"strings"
	"testing"
)

// The fixture is a throwaway key made with `tauri signer generate`, and a signature made with
// `tauri signer sign`, so these tests pin the verifier to the real signer's output format.
func readFixture(t *testing.T, name string) string {
	t.Helper()
	b, err := os.ReadFile("testdata/" + name)
	if err != nil {
		t.Fatal(err)
	}
	return string(b)
}

// cmdrPubkey is the app's real updater key (`tauri.conf.json`), which didn't sign the fixture.
const cmdrPubkey = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEEyNjAxRjM2QkIxNjhDMEEKUldRS2pCYTdOaDlnb2lwTE5wZUNVZE5FTTBScXRrTWJPb1EvN3J2Q0FVcC9JNzJkSmphUTl3V3MK"

func TestVerifyAcceptsTheSignersOwnOutput(t *testing.T) {
	_, err := verifyTauriSignature([]string{readFixture(t, "fixture.pub")}, readFixture(t, "payload.txt.sig"),
		strings.NewReader(readFixture(t, "payload.txt")))
	if err != nil {
		t.Fatalf("want a valid signature, got %v", err)
	}
}

func TestVerifyRefusesAChangedFile(t *testing.T) {
	_, err := verifyTauriSignature([]string{readFixture(t, "fixture.pub")}, readFixture(t, "payload.txt.sig"),
		strings.NewReader(readFixture(t, "payload.txt")+"x"))
	if !errors.Is(err, errBadSignature) {
		t.Fatalf("want errBadSignature, got %v", err)
	}
}

func TestVerifyRefusesAnotherKeysSignature(t *testing.T) {
	_, err := verifyTauriSignature([]string{cmdrPubkey}, readFixture(t, "payload.txt.sig"),
		strings.NewReader(readFixture(t, "payload.txt")))
	if !errors.Is(err, errKeyMismatch) {
		t.Fatalf("want errKeyMismatch, got %v", err)
	}
}

func TestVerifyRefusesAnEditedTrustedComment(t *testing.T) {
	text, err := base64.StdEncoding.DecodeString(readFixture(t, "payload.txt.sig"))
	if err != nil {
		t.Fatal(err)
	}
	edited := bytes.Replace(text, []byte("file:payload.txt"), []byte("file:other.txt"), 1)
	if bytes.Equal(edited, text) {
		t.Fatal("fixture has no file: comment to edit")
	}
	_, err = verifyTauriSignature([]string{readFixture(t, "fixture.pub")}, base64.StdEncoding.EncodeToString(edited),
		strings.NewReader(readFixture(t, "payload.txt")))
	if !errors.Is(err, errBadTrustedComments) {
		t.Fatalf("want errBadTrustedComments, got %v", err)
	}
}

func TestVerifyRefusesGarbage(t *testing.T) {
	for name, sig := range map[string]string{
		"not base64":   "%%%",
		"wrong shape":  base64.StdEncoding.EncodeToString([]byte("one line only\n")),
		"empty string": "",
	} {
		t.Run(name, func(t *testing.T) {
			_, err := verifyTauriSignature([]string{readFixture(t, "fixture.pub")}, sig, strings.NewReader("x"))
			if !errors.Is(err, errMalformed) {
				t.Fatalf("want errMalformed, got %v", err)
			}
		})
	}
}

func TestCmdrPubkeyParses(t *testing.T) {
	pk, err := parsePublicKey(cmdrPubkey)
	if err != nil {
		t.Fatal(err)
	}
	// minisign prints key IDs little-endian: the comment's A2601F36BB168C0A.
	if got := pk.id(); got != "A2601F36BB168C0A" {
		t.Fatalf("key ID %s, want A2601F36BB168C0A", got)
	}
}

// The backup key signs only a rotation release, and every verifier must take it, or that
// release is refused before it ships. Its position in the list mustn't matter either.
func TestVerifyAcceptsASignatureFromAnyTrustedKey(t *testing.T) {
	for name, keys := range map[string][]string{
		"signer first":  {readFixture(t, "fixture.pub"), cmdrPubkey},
		"signer second": {cmdrPubkey, readFixture(t, "fixture.pub")},
	} {
		t.Run(name, func(t *testing.T) {
			keyID, err := verifyTauriSignature(keys, readFixture(t, "payload.txt.sig"),
				strings.NewReader(readFixture(t, "payload.txt")))
			if err != nil {
				t.Fatalf("want a valid signature, got %v", err)
			}
			if keyID != "F8B9280503EFB206" {
				t.Fatalf("signed by %s, want the fixture key F8B9280503EFB206", keyID)
			}
		})
	}
}

func TestVerifyRefusesASignatureFromAKeyOutsideTheList(t *testing.T) {
	keys, err := readTrustedPubkeys("../..")
	if err != nil {
		t.Fatal(err)
	}
	_, err = verifyTauriSignature(keys, readFixture(t, "payload.txt.sig"), strings.NewReader(readFixture(t, "payload.txt")))
	if !errors.Is(err, errKeyMismatch) {
		t.Fatalf("want errKeyMismatch, got %v", err)
	}
}

// The repo's trust list is the main key from tauri.conf.json, then the offline backup.
func TestTrustedPubkeysAreTheMainKeyThenTheBackup(t *testing.T) {
	keys, err := readTrustedPubkeys("../..")
	if err != nil {
		t.Fatal(err)
	}
	var ids []string
	for _, k := range keys {
		pk, err := parsePublicKey(k)
		if err != nil {
			t.Fatal(err)
		}
		ids = append(ids, pk.id())
	}
	if want := []string{"A2601F36BB168C0A", "B72B6B32B7D71B58"}; !slices.Equal(ids, want) {
		t.Fatalf("trusted key IDs %v, want %v", ids, want)
	}
}

func TestSigningEnvReadsTheBackupKeyOnlyFromTheEnvironment(t *testing.T) {
	env := map[string]string{backupKeyEnv: "backup-key", backupPasswordEnv: "backup-password"}
	sopsRead := false
	readSecret := func(string) (string, error) { sopsRead = true; return "main", nil }

	got, err := signingEnv(true, func(name string) string { return env[name] }, readSecret)
	if err != nil {
		t.Fatal(err)
	}
	want := []string{"TAURI_SIGNING_PRIVATE_KEY=backup-key", "TAURI_SIGNING_PRIVATE_KEY_PASSWORD=backup-password"}
	if !slices.Equal(got, want) || sopsRead {
		t.Fatalf("got %v (sops read: %v), want %v and no sops read", got, sopsRead, want)
	}

	if _, err := signingEnv(true, func(string) string { return "" }, readSecret); err == nil {
		t.Fatal("want a refusal when the backup key isn't in the environment")
	}

	got, err = signingEnv(false, func(name string) string { return env[name] }, readSecret)
	if err != nil {
		t.Fatal(err)
	}
	if got[0] != "TAURI_SIGNING_PRIVATE_KEY=main" {
		t.Fatalf("without -backup-key, want the sops key, got %v", got)
	}
}
