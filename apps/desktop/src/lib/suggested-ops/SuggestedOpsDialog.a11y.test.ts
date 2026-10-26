/**
 * Tier 3 a11y and disclosure coverage for the AI suggestions dialog.
 *
 * The store is mocked, so nothing here reads a database or queues an operation. What these
 * tests actually pin is the disclosure: the button names the action, the AI's reason reaches
 * the screen LABELLED as the AI's words beside the facts Cmdr holds by itself, the undo pill
 * says why, an irreversible group says so, a folder that will be created says so, and a file's
 * size and date say what the disk says now, or say plainly that they're from suggestion time.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount } from 'svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

const { actions, store } = vi.hoisted(() => ({
  actions: {
    close: vi.fn(),
    approve: vi.fn<(id: number) => Promise<void>>(),
    reject: vi.fn<(id: number) => Promise<void>>(),
    reload: vi.fn<(id: number) => Promise<void>>(),
    ensure: vi.fn<(payload: { groupId: number; startIndex: number }) => Promise<void>>(),
    range: vi.fn<(payload: { groupId: number; start: number; end: number }) => Promise<void>>(),
    toggle: vi.fn<(payload: { groupId: number; opId: number }) => void>(),
    showMore: vi.fn<(id: number) => void>(),
    showAll: vi.fn<(id: number) => void>(),
  },
  // A plain object rather than `$state`: each test mounts a fresh dialog, so the values only
  // have to be right at mount time.
  store: {
    state: {
      open: true,
      loading: false,
      loadError: false,
      sweeps: [] as Array<{ groups: Array<{ groupId: number; liveOpCount: number }> }>,
      reviews: new Map<number, unknown>(),
      busyGroupId: null as number | null,
      decisionNotice: null as string | null,
      refusals: new Map<number, unknown>(),
    },
    ops: [] as Array<{ opId: number }>,
    review: {
      window: null as { offset: number; ops: unknown[]; total: number } | null,
      windowLoading: false,
      windowError: false,
      shown: 10,
      deselected: new Set<number>(),
      changedUnderReview: false,
      liveFacts: new Map<number, unknown>(),
      sourceUnreachable: null as string | null,
      range: { start: 0, end: 10 },
    },
  },
}))

// `ModalDialog` tells the MCP dialog registry it opened, which is a real Tauri call.
vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))
vi.mock('#lib/icon-cache.js', async () => {
  const { writable } = await import('svelte/store')
  return { getCachedIcon: () => undefined, iconCacheVersion: writable(0), prefetchIcons: () => Promise.resolve() }
})

vi.mock('./suggested-ops-trigger.svelte', () => ({
  INITIAL_ROWS: 10,
  SHOW_MORE_STEP: 20,
  suggestedOpsState: store.state,
  approvableCount: (id: number) => store.state.sweeps[0]?.groups.find((g) => g.groupId === id)?.liveOpCount ?? 0,
  approveGroup: actions.approve,
  closeSuggestedOps: actions.close,
  ensureOpWindow: (groupId: number, startIndex: number) => actions.ensure({ groupId, startIndex }),
  opAt: (_groupId: number, index: number) => store.ops[index] ?? null,
  rejectGroup: actions.reject,
  reloadGroup: actions.reload,
  reviewOf: () => store.review,
  showAllRows: actions.showAll,
  showMoreRows: actions.showMore,
  showRange: (groupId: number, start: number, end: number) => actions.range({ groupId, start, end }),
  shownRows: (group: { liveOpCount: number }) => Math.min(store.review.shown, group.liveOpCount),
  toggleOp: (groupId: number, opId: number) => {
    actions.toggle({ groupId, opId })
  },
  totalRows: (group: { liveOpCount: number }) => group.liveOpCount,
}))

const SuggestedOpsDialog = (await import('./SuggestedOpsDialog.svelte')).default

function mountDialog(): HTMLElement {
  const host = document.createElement('div')
  document.body.appendChild(host)
  mount(SuggestedOpsDialog, { target: host })
  flushSync()
  return host
}

function group(overrides: Record<string, unknown> = {}) {
  return {
    groupId: 7,
    sweepId: 1,
    verb: 'move',
    status: 'pending',
    displayName: 'Installers to the NAS',
    rationale: 'They all look like installers to me.',
    sourceVolumeId: 'root',
    sourceFolder: '/Users/someone/Downloads',
    destination: '/backups/Installers',
    destinationVolumeName: 'Naspolya',
    reversible: 'restoreMove',
    destinationState: 'exists',
    liveOpCount: 3,
    totalOpCount: 3,
    fromSelector: false,
    ...overrides,
  }
}

function seed(groupOverrides: Record<string, unknown> = {}) {
  store.state.sweeps = [
    {
      sweepId: 1,
      createdAt: 1_780_000_000,
      rationale: 'Ten new files in Downloads.',
      groups: [group(groupOverrides)],
    } as unknown as { groups: Array<{ groupId: number; liveOpCount: number }> },
  ]
}

beforeEach(() => {
  vi.clearAllMocks()
  document.body.innerHTML = ''
  store.state.loading = false
  store.state.loadError = false
  store.state.busyGroupId = null
  store.state.decisionNotice = null
  store.state.refusals = new Map<number, unknown>()
  store.ops = [
    {
      opId: 1,
      sourcePath: '/Users/someone/Downloads/invoice-jan.pdf',
      newName: null,
      status: 'pending',
      snapshotSize: 20_480,
      snapshotModified: 1_780_000_000,
    },
    {
      opId: 2,
      sourcePath: '/Users/someone/Downloads/unrecorded.pdf',
      newName: null,
      status: 'pending',
      snapshotSize: null,
      snapshotModified: null,
    },
    {
      opId: 3,
      sourcePath: '/Users/someone/Downloads/gone.dmg',
      newName: null,
      status: 'pending',
      snapshotSize: 1_000,
      snapshotModified: 1_780_000_000,
    },
  ] as unknown as Array<{ opId: number }>
  store.review = {
    window: { offset: 0, ops: store.ops, total: store.ops.length },
    windowLoading: false,
    windowError: false,
    shown: 10,
    deselected: new Set<number>(),
    changedUnderReview: false,
    liveFacts: new Map<number, unknown>(),
    sourceUnreachable: null,
    range: { start: 0, end: 10 },
  }
  seed()
})

describe('accessibility', () => {
  it('has no violations with a group and its file list', async () => {
    const host = mountDialog()
    await expectNoA11yViolations(host)
  })

  it('has no violations with every row state at once', async () => {
    store.review.liveFacts = new Map<number, unknown>([
      [1, { kind: 'present', size: 99_999, modified: 1_790_000_000 }],
      [3, { kind: 'missing' }],
    ])
    const host = mountDialog()
    await expectNoA11yViolations(host)
  })

  it('names every per-file checkbox after its file', () => {
    const host = mountDialog()

    const names = [...host.querySelectorAll('input[type="checkbox"]')].map((box) => box.getAttribute('aria-label'))
    expect(names).toContain('Include invoice-jan.pdf')
  })
})

describe('the action', () => {
  it('names the verb and the count on the button', () => {
    const host = mountDialog()

    const buttons = [...host.querySelectorAll('button')].map((b) => b.textContent.trim())
    expect(buttons).toContain('Move 3 files')
    expect(buttons).toContain('Reject')
  })

  it('names the Trash on a trash group', () => {
    seed({ verb: 'trash', destination: null, destinationVolumeName: null, destinationState: 'notApplicable' })
    const host = mountDialog()

    const buttons = [...host.querySelectorAll('button')].map((b) => b.textContent.trim())
    expect(buttons).toContain('Move 3 files to Trash')
    expect(host.querySelector('.facts')?.textContent).toContain('The Trash')
  })

  it('shows where the files are and the full destination, volume included', () => {
    const host = mountDialog()

    const facts = host.querySelector('.facts')?.textContent ?? ''
    expect(facts).toContain('/Users/someone/Downloads')
    expect(facts).toContain('Naspolya')
    expect(facts).toContain('/backups/Installers')
  })

  it('shows the files right away, with no button to reveal them', () => {
    const host = mountDialog()

    expect(host.textContent).toContain('invoice-jan.pdf')
    const buttons = [...host.querySelectorAll('button')].map((b) => b.textContent.trim())
    expect(buttons).not.toContain('Review the files')
  })

  it('offers more rows, then all of them, under a long list', () => {
    seed({ liveOpCount: 60_000, totalOpCount: 60_000 })
    const host = mountDialog()

    const buttons = [...host.querySelectorAll('button')].map((b) => b.textContent.trim())
    expect(buttons).toContain('Show 20 more')
    expect(buttons).toContain('Show all 60,000')
  })
})

/**
 * The AI's reason is LABELLED as its words, beside facts Cmdr holds by itself. Without the label,
 * a rationale reads as something Cmdr verified, and a hallucinated claim arrives looking like a
 * finding.
 */
describe('disclosure', () => {
  it('labels the AI’s words as the AI’s, never as something Cmdr checked', () => {
    const host = mountDialog()

    expect(host.textContent).toContain('They all look like installers to me.')
    expect(host.textContent).toContain('The AI’s reason')
  })

  it('says why a move can be undone', () => {
    const host = mountDialog()

    expect(host.textContent).toContain('You can undo this: Cmdr can move them back')
  })

  it('says a permanent delete cannot be undone, and still offers it', () => {
    seed({ verb: 'delete', reversible: 'irreversible', destination: null, destinationState: 'notApplicable' })
    const host = mountDialog()

    expect(host.textContent).toContain('This can’t be undone')
    const buttons = [...host.querySelectorAll('button')].map((b) => b.textContent.trim())
    expect(buttons).toContain('Delete 3 files')
  })

  it('never promises a Trash on a drive that has none', () => {
    seed({ verb: 'trash', reversible: 'irreversible', destination: null, destinationState: 'notApplicable' })
    const host = mountDialog()

    expect(host.textContent).toContain('This can’t be undone: this drive has no Trash')
  })

  it('says when approving would create the target folder', () => {
    seed({ destinationState: 'willBeCreated' })
    const host = mountDialog()

    expect(host.textContent).toContain('Cmdr will create this folder')
  })

  it('admits when it could not check the target folder, rather than guessing', () => {
    seed({ destinationState: 'unknown' })
    const host = mountDialog()

    expect(host.textContent).toContain('Cmdr couldn’t check the target folder')
  })

  it('marks a group a pattern produced', () => {
    seed({ fromSelector: true })
    const host = mountDialog()

    expect(host.textContent).toContain('Matched by a pattern')
  })
})

describe('honest file facts', () => {
  it('labels a value Cmdr couldn’t check now as the suggestion-time one, and never shows a zero', () => {
    const host = mountDialog()

    expect(host.querySelector('[aria-label="From when the AI suggested this"]')).not.toBeNull()
    expect(host.textContent).toContain('Unknown')
    expect(host.textContent).not.toContain('0 B')
    expect(host.textContent).not.toContain('Not in the index')
  })

  it('flags a live value that moved since the suggestion, with both values', () => {
    store.review.liveFacts = new Map<number, unknown>([[1, { kind: 'present', size: 99_999, modified: 1_780_000_000 }]])
    const host = mountDialog()

    const glyph = host.querySelector('[aria-label="Changed since the suggestion"]')
    expect(glyph).not.toBeNull()
  })

  it('says a file is gone', () => {
    store.review.liveFacts = new Map<number, unknown>([[3, { kind: 'missing' }]])
    const host = mountDialog()

    expect(host.textContent).toContain('Moved or deleted')
  })

  it('says once, above the list, when the files’ drive isn’t connected', () => {
    store.review.sourceUnreachable = 'notConnected'
    const host = mountDialog()

    expect(host.textContent).toContain('Cmdr isn’t connected to the drive these files are on')
  })

  it('shows each file’s folder only when the files don’t share one', () => {
    const shared = mountDialog()
    expect(shared.querySelector('[data-column="folder"]')).toBeNull()

    document.body.innerHTML = ''
    seed({ sourceFolder: null })
    const mixed = mountDialog()
    expect(mixed.querySelector('[data-column="folder"]')).not.toBeNull()
    expect(mixed.textContent).toContain('Several folders')
  })
})

describe('honest absence', () => {
  it('announces a group the agent changed instead of swapping the rows', () => {
    store.review.changedUnderReview = true
    const host = mountDialog()

    expect(host.textContent).toContain('The AI changed this suggestion while you were reading it.')
    expect(host.textContent).toContain('invoice-jan.pdf')
    const reload = [...host.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Show the new version')
    reload?.click()
    expect(actions.reload).toHaveBeenCalledWith(7)
  })

  it('says nothing is waiting when nothing is', () => {
    store.state.sweeps = []
    const host = mountDialog()

    expect(host.textContent).toContain('Nothing is waiting for you right now.')
  })

  it('says the files could not load, and offers to try again', async () => {
    store.review.window = null
    store.review.windowError = true
    const host = mountDialog()

    expect(host.textContent).toContain('Cmdr couldn’t load these files.')
    const retry = [...host.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Try again')
    expect(retry).toBeDefined()
    retry?.click()
    expect(actions.ensure).toHaveBeenCalledWith({ groupId: 7, startIndex: 0 })
    await expectNoA11yViolations(host)
  })

  it('says when an approval might not have gone through', () => {
    store.state.decisionNotice = 'suggestedOps.approvalUnsure'
    const host = mountDialog()

    expect(host.textContent).toContain('Cmdr isn’t sure the approval went through.')
  })

  it('distinguishes a read that failed from an empty list', () => {
    store.state.sweeps = []
    store.state.loadError = true
    const host = mountDialog()

    expect(host.textContent).toContain('Cmdr couldn’t read the suggestions.')
    expect(host.textContent).not.toContain('Nothing is waiting for you right now.')
  })

  it('shows why an approval didn’t start under the group it gave back', async () => {
    store.state.refusals = new Map([
      [7, { kind: 'refused', error: { type: 'source_not_connected', path: '/DCIM/one.jpg' } }],
    ])
    const host = mountDialog()

    const refusal = host.querySelector('.group .refusal')
    expect(refusal?.getAttribute('role')).toBe('alert')
    expect(refusal?.textContent).toContain('Why this didn’t start')
    expect(refusal?.textContent).toContain('Cmdr hasn’t connected to the phone or server holding these files yet')
    await expectNoA11yViolations(host)
  })

  it('says nothing under a group whose approval nobody refused', () => {
    const host = mountDialog()

    expect(host.querySelector('.refusal')).toBeNull()
  })
})

/** The dialog's parts, each audited on its own as well, so a part reused elsewhere stays clean. */
describe('the parts on their own', () => {
  async function audit(component: Parameters<typeof mount>[0], props: Record<string, unknown>): Promise<HTMLElement> {
    const host = document.createElement('div')
    document.body.appendChild(host)
    mount(component, { target: host, props })
    flushSync()
    await expectNoA11yViolations(host)
    return host
  }

  it('AgentReason labels the AI’s words', async () => {
    const AgentReason = (await import('./AgentReason.svelte')).default
    const host = await audit(AgentReason, { text: 'They all look like invoices.' })
    expect(host.textContent).toContain('The AI’s reason')
  })

  it('SuggestedOpsGroup renders a group with no violations', async () => {
    const SuggestedOpsGroup = (await import('./SuggestedOpsGroup.svelte')).default
    const host = await audit(SuggestedOpsGroup, { group: group() })
    expect(host.textContent).toContain('Move 3 files')
  })

  it('SuggestedOpFiles renders a file list with no violations', async () => {
    const SuggestedOpFiles = (await import('./SuggestedOpFiles.svelte')).default
    const { createRawSnippet } = await import('svelte')
    const actions = createRawSnippet(() => ({ render: () => '<span></span>' }))
    const host = await audit(SuggestedOpFiles, { group: group(), actions })
    expect(host.textContent).toContain('invoice-jan.pdf')
  })
})
