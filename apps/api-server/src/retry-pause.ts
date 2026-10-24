/**
 * Wait `ms` between retries. Its own module so the node test project can make it instant for every
 * test (`vitest-setup.ts`) while the retry logic itself runs for real.
 */
export function pause(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}
