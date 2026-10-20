package main

import (
	"testing"
	"time"
)

func TestAfterGHFailure(t *testing.T) {
	now := time.Date(2026, 10, 20, 10, 49, 0, 0, time.UTC)
	spentUntil := func(reset time.Time) *apiBudget { return &apiBudget{spent: true, reset: reset} }
	inTenMin := now.Add(10 * time.Minute)
	cases := []struct {
		name     string
		budget   *apiBudget
		retries  int
		want     ghRetry
		wantWait time.Duration
	}{
		{"budget spent: wait for the reset", spentUntil(inTenMin), 2, ghWaitForReset, 10*time.Minute + resetSlack},
		{"a write with no retries still waits out a spent budget", spentUntil(inTenMin), 0, ghWaitForReset, 10*time.Minute + resetSlack},
		{"reset already passed: a short wait", spentUntil(now.Add(-time.Minute)), 2, ghWaitForReset, resetSlack},
		{"a secondary limit's Retry-After", &apiBudget{retryAfter: time.Minute}, 0, ghWaitForReset, time.Minute + resetSlack},
		{"budget left: a short retry", &apiBudget{}, 2, ghRetryTransient, transientWait},
		{"budget unknown: a short retry", nil, 1, ghRetryTransient, transientWait},
		{"out of retries", &apiBudget{}, 0, ghGiveUp, 0},
	}
	for _, c := range cases {
		got, wait := afterGHFailure(c.budget, c.retries, now)
		if got != c.want || wait != c.wantWait {
			t.Errorf("%s: got %d after %s, want %d after %s", c.name, got, wait, c.want, c.wantWait)
		}
	}
}

// The shape `gh api --include` printed on 2026-10-20 (gh 2.x): a bare `\n` after the status line,
// `\r\n` after each header, and no Content-Length.
func TestParseAPIResponse(t *testing.T) {
	limited := "HTTP/2.0 403 Forbidden\n" +
		"Content-Type: application/json; charset=utf-8\r\n" +
		"X-Ratelimit-Limit: 5000\r\n" +
		"X-Ratelimit-Remaining: 0\r\n" +
		"X-Ratelimit-Reset: 1792493537\r\n" +
		"X-Ratelimit-Resource: core\r\n" +
		"\r\n" +
		`{"message":"API rate limit exceeded","status":"403"}`
	status, budget, body, ok := parseAPIResponse(limited)
	if !ok || status != 403 || !budget.spent || budget.reset.Unix() != 1792493537 {
		t.Errorf("rate limited: got %d %+v ok=%v", status, budget, ok)
	}
	if body != `{"message":"API rate limit exceeded","status":"403"}` {
		t.Errorf("rate limited: body %q", body)
	}

	fine := "HTTP/2.0 200 OK\nX-Ratelimit-Remaining: 4999\r\n\r\n{\"total_count\":0}\n"
	if status, budget, body, ok := parseAPIResponse(fine); !ok || status != 200 || budget.spent || body != "{\"total_count\":0}\n" {
		t.Errorf("fine: got %d %+v %q ok=%v", status, budget, body, ok)
	}

	secondary := "HTTP/2.0 403 Forbidden\nRetry-After: 60\r\nX-Ratelimit-Remaining: 4000\r\n\r\n{}"
	if _, budget, _, _ := parseAPIResponse(secondary); budget.spent || budget.retryAfter != time.Minute {
		t.Errorf("secondary: got %+v", budget)
	}

	if _, _, _, ok := parseAPIResponse(""); ok {
		t.Error("no response: ok")
	}
}

func TestRateLimitBudget(t *testing.T) {
	var l rateLimit
	l.Resources.Core = rateBucket{Remaining: 0, Reset: 100}
	l.Resources.GraphQL = rateBucket{Remaining: 0, Reset: 200}
	if b := l.budget(); !b.spent || b.reset.Unix() != 200 {
		t.Errorf("both spent: got %+v, want the later reset", b)
	}
	l.Resources.Core.Remaining = 10
	l.Resources.GraphQL.Remaining = 10
	if b := l.budget(); b.spent {
		t.Errorf("budget left: got %+v", b)
	}
}
