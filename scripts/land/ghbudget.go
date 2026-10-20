package main

import (
	"bufio"
	"io"
	"net/http"
	"strconv"
	"strings"
	"time"
)

// The decisions behind retrying a failed gh call. Many parallel lands poll one account, so its
// API budget does run out, and failing then would throw away a land whose CI already went green.

// apiBudget is what GitHub says about the budget a failed call ran into.
type apiBudget struct {
	spent bool
	// reset is when a spent budget refills.
	reset time.Time
	// retryAfter is a secondary rate limit's `Retry-After`.
	retryAfter time.Duration
}

type ghRetry int

const (
	ghGiveUp ghRetry = iota
	// ghWaitForReset: the API budget is spent; wait until it refills, then retry.
	ghWaitForReset
	// ghRetryTransient: budget left, so likely a network blip; retry shortly, a few times.
	ghRetryTransient
)

const (
	resetSlack       = 5 * time.Second
	transientRetries = 2
	transientWait    = 10 * time.Second
)

// afterGHFailure decides what follows a failed gh call, given GitHub's word on the budget (nil
// when unknown) and how many short retries the call has left. A spent budget always waits,
// even for a write with no retries: the call never reached GitHub's handler.
func afterGHFailure(b *apiBudget, retriesLeft int, now time.Time) (ghRetry, time.Duration) {
	switch {
	case b != nil && b.retryAfter > 0:
		return ghWaitForReset, b.retryAfter + resetSlack
	case b != nil && b.spent:
		return ghWaitForReset, max(b.reset.Sub(now), 0) + resetSlack
	case retriesLeft > 0:
		return ghRetryTransient, transientWait
	default:
		return ghGiveUp, 0
	}
}

// parseAPIResponse splits `gh api --include` output into the status, the budget headers, and the
// body. ok is false when there's no HTTP response in it (gh never reached GitHub).
func parseAPIResponse(out string) (status int, budget apiBudget, body string, ok bool) {
	resp, err := http.ReadResponse(bufio.NewReader(strings.NewReader(out)), nil)
	if err != nil {
		return 0, apiBudget{}, "", false
	}
	// gh prints no Content-Length, so the body runs to the end.
	raw, err := io.ReadAll(resp.Body)
	_ = resp.Body.Close()
	if err != nil {
		return 0, apiBudget{}, "", false
	}
	body = string(raw)
	h := resp.Header
	if remaining, err := strconv.Atoi(h.Get("X-RateLimit-Remaining")); err == nil && remaining == 0 {
		budget.spent = true
		if reset, err := strconv.ParseInt(h.Get("X-RateLimit-Reset"), 10, 64); err == nil {
			budget.reset = time.Unix(reset, 0)
		}
	}
	if secs, err := strconv.Atoi(h.Get("Retry-After")); err == nil && secs > 0 {
		budget.retryAfter = time.Duration(secs) * time.Second
	}
	return resp.StatusCode, budget, body, true
}

// rateLimit is `gh api rate_limit`, a call that costs no budget itself. `gh pr` spends the
// GraphQL budget and `gh run` the REST (core) one. It can disagree with the headers of the call
// that failed (2026-10-20: `core` showed 2,862 left while the Actions API answered remaining 0),
// so it's only the fallback for calls with no headers of their own.
type rateLimit struct {
	Resources struct {
		Core    rateBucket `json:"core"`
		GraphQL rateBucket `json:"graphql"`
	} `json:"resources"`
}

type rateBucket struct {
	Remaining int   `json:"remaining"`
	Reset     int64 `json:"reset"` // Unix seconds
}

// budget is the latest-refilling spent bucket, or an unspent budget.
func (l rateLimit) budget() apiBudget {
	var b apiBudget
	for _, bucket := range []rateBucket{l.Resources.Core, l.Resources.GraphQL} {
		if reset := time.Unix(bucket.Reset, 0); bucket.Remaining == 0 && (!b.spent || reset.After(b.reset)) {
			b = apiBudget{spent: true, reset: reset}
		}
	}
	return b
}
