/**
 * Tier 3 a11y tests for the transfer tree: axe over every fixture while it's
 * preparing, mid-transfer, with following paused (the Follow button showing),
 * and finished.
 *
 * Contrast is theme-dependent and axe's is off here (`#lib/test-a11y`): the
 * rows' color pairs are checked in both themes by `pnpm check a11y-contrast`,
 * which reads `TransferTreeRow.svelte`'s and `TransferTree.svelte`'s styles.
 */

import { describe, it, vi, afterEach } from 'vitest'
import { mount, flushSync } from 'svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import TransferTree from './TransferTree.svelte'
import TransferTreeRow from './TransferTreeRow.svelte'
import { TRANSFER_TREE_FIXTURES } from './fixtures/catalog'
import { createFixturePlayer } from './fixtures/fixture-player.svelte'

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  getFileSizeFormat: () => 'binary',
}))

afterEach(() => {
  document.body.innerHTML = ''
})

async function settle(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 20))
  for (let i = 0; i < 10; i++) {
    await Promise.resolve()
    flushSync()
  }
}

describe('TransferTreeRow a11y', () => {
  it.each(['waiting', 'active', 'done', 'skipped', 'failed'] as const)(
    'has no violations as a %s row',
    async (state) => {
      const host = document.createElement('div')
      host.setAttribute('role', 'tree')
      host.setAttribute('aria-label', 'Files in this transfer')
      document.body.appendChild(host)
      mount(TransferTreeRow, {
        target: host,
        props: {
          row: {
            id: 3,
            depth: 1,
            label: 'IMG_0001.jpg',
            kind: 'file',
            state,
            expanded: false,
            hasChildren: false,
            posInSet: 1,
            setSize: 4,
            filesDone: state === 'done' ? 1 : 0,
            filesTotal: 1,
            bytesDone: 1_000,
            bytesTotal: 4_000,
            skipped: 0,
            failed: 0,
            failure: state === 'failed' ? { type: 'source_not_found', path: '/src/IMG_0001.jpg' } : null,
          },
          index: 0,
          axis: 'bytes',
          operation: 'copy',
          path: 'Photos/IMG_0001.jpg',
          tabStop: true,
        },
      })
      flushSync()
      await expectNoA11yViolations(host)
    },
  )
})

describe.each(TRANSFER_TREE_FIXTURES.map((fixture) => [fixture.title, fixture] as const))(
  'TransferTree a11y: %s',
  (_title, fixture) => {
    it('has no violations while preparing, mid-transfer, paused, and finished', async () => {
      const player = createFixturePlayer(fixture)
      const transfer = player.transfer
      const target = document.createElement('div')
      document.body.appendChild(target)
      mount(TransferTree, { target, props: player.props })
      await settle()
      await expectNoA11yViolations(target)

      const advance = async (steps: number) => {
        player.step(steps)
        await settle()
      }

      await advance(12)
      await expectNoA11yViolations(target)

      target.querySelector<HTMLElement>('[role="treeitem"][tabindex="0"]')?.focus()
      document.activeElement?.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
      await settle()
      await expectNoA11yViolations(target)

      for (let guard = 0; guard < 400 && !transfer.finished; guard++) transfer.advance()
      await advance(1)
      await expectNoA11yViolations(target)
    })
  },
)
