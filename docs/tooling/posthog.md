# PostHog (Cmdr-specific)

Used for session replay, heatmaps, and click tracking on getcmdr.com. Only the website talks to PostHog.

- **Project**: https://eu.posthog.com/project/136072
- **Project settings**: https://eu.posthog.com/project/136072/settings/project-details
- **Host**: EU cloud (`https://eu.i.posthog.com`).
- **Project API key** (`phc_...`): public ingest token. Safe to include in client-side code (PostHog designed it this
  way). The website bakes it via `PUBLIC_POSTHOG_KEY`.

## Desktop feature events (not here)

The desktop app's anonymous, PII-free product events (the beta usage analytics) don't reach PostHog: they ride the
heartbeat to `api.getcmdr.com` and stay in D1 (`analytics_event`). Older PostHog data with `source: "desktop"` is from
before that. Desktop side: `apps/desktop/src-tauri/src/analytics/CLAUDE.md`; server side:
`apps/api-server/src/telemetry/DETAILS.md`.
