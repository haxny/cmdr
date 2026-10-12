/**
 * A simulated transfer that answers tree views the way the backend does, with
 * no IPC: for the Debug catalog (live, on a timer) and the component tests
 * (stepped by hand).
 *
 * It mirrors `transfer_tree/view.rs`'s contract (effective expansion, compact
 * folders keyed by their deepest folder, the Follow and At windows, the
 * ancestors above the window, derived folder states) in plain O(rows) code,
 * which is fine for a fixture and ❌ never a model to reuse in the app: there
 * the backend owns the tree.
 */

import type {
  ProgressAxis,
  TransferTreeAnswer,
  TransferTreeAncestor,
  TransferTreeRow,
  TransferTreeViewRequest,
  TreeRowState,
} from '$lib/tauri-commands'
import type { WriteOperationError } from '$lib/ipc/bindings'
import type { TransferTreeSource } from '../transfer-tree-source'

/** A file or folder of a fixture. A folder has `children`; a file has a `size`. */
export interface FixtureNode {
  name: string
  size?: number
  children?: FixtureNode[]
  /** What happens when the transfer reaches this file. Default: it lands. */
  outcome?: 'skip' | 'fail'
}

export interface ScriptedTransferOptions {
  /** The selection: the top-level rows. */
  top: FixtureNode[]
  /** Files in flight at once (the SMB concurrent driver runs up to 32). */
  parallel?: number
  /** Bytes each active file moves per step. */
  bytesPerStep: number
  /** Steps answering `ready: false` first, like a scan. */
  preparingSteps?: number
  axis?: ProgressAxis
}

/** A scripted source you step: `advance()` is one progress tick. */
export interface ScriptedTransfer extends TransferTreeSource {
  advance: () => void
  readonly finished: boolean
}

type LeafState = 'waiting' | 'active' | 'done' | 'skipped' | 'failed'

interface Node {
  parent: number
  name: string
  folder: boolean
  size: number
  children: number[]
  outcome: FixtureNode['outcome']
}

const ROOT = 0

function failureFor(path: string): WriteOperationError {
  return { type: 'source_not_found', path }
}

export function createScriptedTransfer(options: ScriptedTransferOptions): ScriptedTransfer {
  const { parallel = 1, bytesPerStep, axis = 'bytes' } = options
  const nodes: Node[] = [{ parent: -1, name: '', folder: true, size: 0, children: [], outcome: undefined }]

  function add(parent: number, fixture: FixtureNode): void {
    const id = nodes.length
    nodes.push({
      parent,
      name: fixture.name,
      folder: fixture.children !== undefined,
      size: fixture.size ?? 0,
      children: [],
      outcome: fixture.outcome,
    })
    nodes[parent].children.push(id)
    for (const child of fixture.children ?? []) add(id, child)
  }
  for (const top of options.top) add(ROOT, top)

  // Tree order: folders first, then files, each by name (fixture names sort
  // bytewise the way the panes' collator would).
  const byName = (a: string, b: string): number => (a < b ? -1 : a > b ? 1 : 0)
  for (const node of nodes) {
    node.children.sort(
      (a, b) => Number(nodes[b].folder) - Number(nodes[a].folder) || byName(nodes[a].name, nodes[b].name),
    )
  }

  const queue: number[] = []
  const walk = (id: number): void => {
    for (const child of nodes[id].children) {
      if (nodes[child].folder) walk(child)
      else queue.push(child)
    }
  }
  walk(ROOT)

  const states = new Map<number, LeafState>()
  const bytesDone = new Map<number, number>()
  const active: number[] = []
  let next = 0
  let lastStarted: number | null = null
  let preparing = options.preparingSteps ?? 0
  let revision = 0

  function pathOf(id: number): string {
    const names: string[] = []
    for (let at = id; at !== ROOT; at = nodes[at].parent) names.push(nodes[at].name)
    return names.reverse().join('/')
  }

  function advance(): void {
    revision++
    if (preparing > 0) {
      preparing--
      return
    }
    for (const id of [...active]) {
      const node = nodes[id]
      const moved = Math.min(node.size, (bytesDone.get(id) ?? 0) + bytesPerStep)
      bytesDone.set(id, moved)
      const settled: LeafState | null =
        node.outcome === 'fail' && moved >= node.size / 2 ? 'failed' : moved >= node.size ? 'done' : null
      if (settled !== null) {
        states.set(id, settled)
        active.splice(active.indexOf(id), 1)
      }
    }
    while (active.length < parallel && next < queue.length) {
      const id = queue[next++]
      if (nodes[id].outcome === 'skip') {
        states.set(id, 'skipped')
        continue
      }
      states.set(id, 'active')
      bytesDone.set(id, 0)
      active.push(id)
      lastStarted = id
    }
  }

  // ---- The view, as `view.rs` answers it ----

  function onlyFolderChild(id: number): number | null {
    const node = nodes[id]
    if (id === ROOT || !node.folder || node.children.length !== 1) return null
    const only = node.children[0]
    return nodes[only].folder ? only : null
  }

  function rowKey(id: number): number {
    let at = id
    for (let only = onlyFolderChild(at); only !== null; only = onlyFolderChild(at)) at = only
    return at
  }

  function chainHead(key: number): number {
    let head = key
    while (nodes[head].parent !== ROOT && onlyFolderChild(nodes[head].parent) === head) head = nodes[head].parent
    return head
  }

  function label(key: number): string {
    const names = [nodes[key].name]
    for (let at = key, head = chainHead(key); at !== head;) {
      at = nodes[at].parent
      names.push(nodes[at].name)
    }
    return names.reverse().join('/')
  }

  interface Totals {
    filesTotal: number
    filesDone: number
    bytesTotal: number
    bytesDone: number
    skipped: number
    failed: number
    active: number
  }

  function totals(): Map<number, Totals> {
    const out = new Map<number, Totals>()
    const sum = (id: number): Totals => {
      const node = nodes[id]
      let result: Totals
      if (!node.folder) {
        const state = states.get(id) ?? 'waiting'
        const landed = state === 'done' || state === 'skipped'
        result = {
          filesTotal: 1,
          filesDone: landed ? 1 : 0,
          bytesTotal: node.size,
          bytesDone: landed ? node.size : state === 'active' ? (bytesDone.get(id) ?? 0) : 0,
          skipped: state === 'skipped' ? 1 : 0,
          failed: state === 'failed' ? 1 : 0,
          active: state === 'active' ? 1 : 0,
        }
      } else {
        result = { filesTotal: 0, filesDone: 0, bytesTotal: 0, bytesDone: 0, skipped: 0, failed: 0, active: 0 }
        for (const child of node.children) {
          const below = sum(child)
          result.filesTotal += below.filesTotal
          result.filesDone += below.filesDone
          result.bytesTotal += below.bytesTotal
          result.bytesDone += below.bytesDone
          result.skipped += below.skipped
          result.failed += below.failed
          result.active += below.active
        }
      }
      out.set(id, result)
      return result
    }
    sum(ROOT)
    return out
  }

  function folderState(sums: Totals): TreeRowState {
    if (sums.failed > 0) return 'failed'
    if (sums.filesDone === sums.filesTotal) return sums.skipped === sums.filesTotal ? 'skipped' : 'done'
    return sums.active > 0 || sums.filesDone > 0 ? 'active' : 'waiting'
  }

  /** The folders the work is in, plus the hand-expanded, minus the hand-collapsed. */
  function effectiveExpansion(request: TransferTreeViewRequest, focus: number[]): Set<number> {
    const known = (id: number): boolean => id > ROOT && id < nodes.length
    const collapsed = new Set(request.collapsed.filter(known).map(rowKey))
    const open = new Set(request.expanded.filter(known).map(rowKey))
    for (const file of focus) {
      for (let at = nodes[file].parent; at !== ROOT; at = nodes[at].parent) open.add(rowKey(at))
    }
    for (const key of open) if (collapsed.has(key) || nodes[key].children.length === 0) open.delete(key)
    return open
  }

  function toRow(
    { key, depth, position, parent }: { key: number; depth: number; position: number; parent: number },
    open: Set<number>,
    sums: Map<number, Totals>,
  ): TransferTreeRow {
    const node = nodes[key]
    const own = sums.get(key) as Totals
    const leaf = states.get(key) ?? 'waiting'
    return {
      id: key,
      depth,
      label: label(key),
      kind: node.folder ? 'folder' : 'file',
      state: node.folder ? folderState(own) : leaf,
      expanded: open.has(key),
      hasChildren: node.children.length > 0,
      posInSet: position + 1,
      setSize: nodes[parent].children.length,
      filesDone: node.folder ? own.filesDone : leaf === 'done' || leaf === 'skipped' ? 1 : 0,
      filesTotal: own.filesTotal,
      bytesDone: node.folder ? own.bytesDone : leaf === 'failed' ? 0 : own.bytesDone,
      bytesTotal: own.bytesTotal,
      skipped: node.folder ? own.skipped : 0,
      failed: node.folder ? own.failed : 0,
      failure: !node.folder && leaf === 'failed' ? failureFor(`/Users/you/Transfers/${pathOf(key)}`) : null,
    }
  }

  function view(request: TransferTreeViewRequest): TransferTreeAnswer {
    if (preparing > 0) {
      return { ready: false, revision, totalRows: 0, offset: 0, rows: [], ancestors: [], focusRow: null, axis }
    }
    const focus = active.length > 0 ? [...active] : lastStarted === null ? [] : [lastStarted]
    const open = effectiveExpansion(request, focus)

    const sums = totals()
    const flat: { key: number; depth: number; position: number; parent: number }[] = []
    const indexOf = new Map<number, number>()
    const list = (parent: number, depth: number): void => {
      nodes[parent].children.forEach((child, position) => {
        const key = rowKey(child)
        indexOf.set(key, flat.length)
        flat.push({ key, depth, position, parent })
        if (open.has(key)) list(key, depth + 1)
      })
    }
    list(ROOT, 0)

    const visibleIndex = (file: number): number => {
      const path: number[] = []
      for (let at: number = file; at !== ROOT; at = nodes[chainHead(at)].parent) path.push(rowKey(at))
      path.reverse()
      for (const key of path) if (key !== path.at(-1) && !open.has(key)) return indexOf.get(key) ?? 0
      return indexOf.get(path.at(-1) ?? ROOT) ?? 0
    }
    const focusRows = focus.map(visibleIndex)
    const focusRow = focusRows.length > 0 ? Math.min(...focusRows) : null

    const total = flat.length
    const limit = Math.min(request.limit, 200)
    const offset =
      request.window.kind === 'at'
        ? Math.min(request.window.offset, total)
        : Math.max(0, Math.min((focusRow ?? 0) - request.window.lead, total - limit))

    const rows = flat.slice(offset, offset + limit).map((entry) => toRow(entry, open, sums))

    const ancestors: TransferTreeAncestor[] = []
    const first = flat.at(offset)
    if (first !== undefined) {
      for (let depth = first.depth - 1, at = offset - 1; depth >= 0 && at >= 0; at--) {
        if (flat[at].depth === depth) {
          ancestors.unshift({ index: at, row: toRow(flat[at], open, sums) })
          depth--
        }
      }
    }

    return { ready: true, revision, totalRows: total, offset, rows, ancestors, focusRow, axis }
  }

  return {
    advance,
    get finished() {
      return preparing === 0 && active.length === 0 && next >= queue.length
    },
    view: (request) => Promise.resolve(view(request)),
  }
}
