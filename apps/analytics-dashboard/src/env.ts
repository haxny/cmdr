import { defineEnvVars } from '@sveltejs/kit/env'

/**
 * A var the sources always read. A missing one becomes `''`, which their env guards treat as unset, so a
 * dashboard missing one key still renders every other source.
 */
const required = { schema: (value: string | undefined) => value ?? '' }

/** A var the sources check for presence: an empty value means "not set", so it collapses to `undefined`. */
const optional = { schema: (value: string | undefined) => value || undefined }

/**
 * Every variable the dashboard reads, all private (`$app/env/private`). On Cloudflare they come from the
 * Pages project's secrets; under `vite dev`, from `.env`. The list and where each value lives:
 * `DETAILS.md` § Deployment and env vars.
 */
export const variables = defineEnvVars({
  UMAMI_API_URL: required,
  UMAMI_USERNAME: required,
  UMAMI_PASSWORD: required,
  UMAMI_WEBSITE_ID: required,
  UMAMI_BLOG_WEBSITE_ID: required,
  UMAMI_PRVW_WEBSITE_ID: required,
  PADDLE_API_KEY_LIVE: required,
  POSTHOG_API_KEY: required,
  POSTHOG_PROJECT_ID: required,
  POSTHOG_API_URL: required,
  GITHUB_TOKEN: optional,
  LICENSE_SERVER_ADMIN_TOKEN: required,
  /** Optional local-QA override for the api-server base URL (defaults to production). */
  WORKER_BASE_URL: optional,
})
