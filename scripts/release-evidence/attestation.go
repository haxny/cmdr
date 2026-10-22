package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"strings"
)

// summarizeAttestations reads `gh attestation verify --format json` output into one line per
// verified attestation: what it attests, who signed it from which source, and the run that did.
// The raw output isn't kept: an SBOM attestation embeds the whole SBOM (2 MB for v0.55.0's
// check), which the evidence folder already holds.
func summarizeAttestations(out []byte) (string, error) {
	var results []struct {
		VerificationResult struct {
			Statement struct {
				PredicateType string            `json:"predicateType"`
				Subject       []json.RawMessage `json:"subject"`
			} `json:"statement"`
			Signature struct {
				Certificate struct {
					BuildSignerURI         string `json:"buildSignerURI"`
					SourceRepositoryRef    string `json:"sourceRepositoryRef"`
					SourceRepositoryDigest string `json:"sourceRepositoryDigest"`
					RunInvocationURI       string `json:"runInvocationURI"`
					RunnerEnvironment      string `json:"runnerEnvironment"`
				} `json:"certificate"`
			} `json:"signature"`
		} `json:"verificationResult"`
	}
	if err := json.Unmarshal(out, &results); err != nil {
		return "", fmt.Errorf("`gh attestation verify --format json` didn't print JSON: %w", err)
	}
	if len(results) == 0 {
		return "", errors.New("`gh attestation verify` exited 0 but verified no attestation")
	}
	lines := make([]string, len(results))
	for i, r := range results {
		s, c := r.VerificationResult.Statement, r.VerificationResult.Signature.Certificate
		lines[i] = fmt.Sprintf("%s over %d subjects, signed by %s from %s at %s, in %s (%s runner)",
			s.PredicateType, len(s.Subject), c.BuildSignerURI, c.SourceRepositoryRef, c.SourceRepositoryDigest, c.RunInvocationURI, c.RunnerEnvironment)
	}
	return strings.Join(lines, "\n"), nil
}
