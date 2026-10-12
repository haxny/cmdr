import * as privateEnv from '$app/env/private'
import type { DashboardEnv } from './types.js'

/**
 * The env every data source reads, as declared in `src/env.ts`. A separate module from `fetch-all.ts`
 * so the composers stay importable (and testable with a hand-built env) outside SvelteKit.
 */
export function dashboardEnv(): DashboardEnv {
  return {
    UMAMI_API_URL: privateEnv.UMAMI_API_URL,
    UMAMI_USERNAME: privateEnv.UMAMI_USERNAME,
    UMAMI_PASSWORD: privateEnv.UMAMI_PASSWORD,
    UMAMI_WEBSITE_ID: privateEnv.UMAMI_WEBSITE_ID,
    UMAMI_BLOG_WEBSITE_ID: privateEnv.UMAMI_BLOG_WEBSITE_ID,
    UMAMI_PRVW_WEBSITE_ID: privateEnv.UMAMI_PRVW_WEBSITE_ID,
    PADDLE_API_KEY_LIVE: privateEnv.PADDLE_API_KEY_LIVE,
    POSTHOG_API_KEY: privateEnv.POSTHOG_API_KEY,
    POSTHOG_PROJECT_ID: privateEnv.POSTHOG_PROJECT_ID,
    POSTHOG_API_URL: privateEnv.POSTHOG_API_URL,
    GITHUB_TOKEN: privateEnv.GITHUB_TOKEN,
    LICENSE_SERVER_ADMIN_TOKEN: privateEnv.LICENSE_SERVER_ADMIN_TOKEN,
    WORKER_BASE_URL: privateEnv.WORKER_BASE_URL,
  }
}
