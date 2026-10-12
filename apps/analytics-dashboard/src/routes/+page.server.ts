import type { PageServerLoad } from './$types'
import { fetchAcquisitionData } from '#lib/server/fetch-all.js'
import { dashboardEnv } from '#lib/server/env.js'
import { resolveSelection } from '#lib/server/types.js'

export type { AcquisitionData } from '#lib/server/fetch-all.js'

/** Loads only the Acquisition page's sources (funnel, Umami, Cloudflare, GitHub, PostHog). */
export const load: PageServerLoad = async ({ url }) => {
  const selection = resolveSelection(url.searchParams.get('range'), url.searchParams.get('day'))
  return fetchAcquisitionData(dashboardEnv(), selection)
}
