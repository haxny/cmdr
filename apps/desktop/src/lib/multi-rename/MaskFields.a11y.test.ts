/**
 * Tier 3 a11y tests for `MaskFields.svelte`: the two mask fields with the name mask's syntax switch, in Cmdr syntax
 * (token markers), in foobar2000 syntax, and with a foobar pattern's error underlined.
 */
import { afterEach, describe, it } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import type { MultiRenameSpec } from '#lib/ipc/bindings.js'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import MaskFields from './MaskFields.svelte'
import type { FieldHistoryMenu } from './field-history-menu.svelte'
import type { MultiRenameState } from './multi-rename-state.svelte'
import { DEFAULT_SPEC } from './spec'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

async function settle(): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await tick()
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

/** Only what the fields read of the sheet's state and its field history. */
async function mountFields(spec: Partial<MultiRenameSpec>, error: unknown = null): Promise<HTMLElement> {
  const tool = { spec: { ...DEFAULT_SPEC, ...spec }, error, pending: false, update: () => {} }
  const fieldHistory = { historyOf: () => ['[N]'], openField: null, open: () => {} }
  const root = document.createElement('div')
  document.body.appendChild(root)
  const fields = mount(MaskFields, {
    target: root,
    props: {
      tool: tool as unknown as MultiRenameState,
      fieldHistory: fieldHistory as unknown as FieldHistoryMenu,
    },
  })
  cleanup = () => {
    void unmount(fields)
  }
  await settle()
  return root
}

describe('MaskFields a11y', () => {
  it('Cmdr syntax with token markers has no violations', async () => {
    await expectNoA11yViolations(await mountFields({ nameMask: '[N]_[C10+2:3]' }))
  })

  it('foobar2000 syntax has no violations', async () => {
    await expectNoA11yViolations(
      await mountFields({ nameMask: "[%album artist% - ]'('%year%')' %album%", maskSyntax: 'foobar' }),
    )
  })

  it('a foobar pattern with its error underlined has no violations', async () => {
    const error = { type: 'spec', error: { type: 'pattern', error: { type: 'unknownField', name: 'nope', at: 6 } } }
    await expectNoA11yViolations(await mountFields({ nameMask: 'Deník %nope%', maskSyntax: 'foobar' }, error))
  })
})
