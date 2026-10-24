/**
 * Runs before every node-project test file. Retries pause for real seconds in production
 * (`github-fetch.ts`); here they pause for none, so a test that answers a GitHub read with a 5xx
 * still runs the real retry logic without waiting it out.
 */
import { vi } from 'vitest'

vi.mock('./retry-pause', () => ({ pause: vi.fn(() => Promise.resolve()) }))
