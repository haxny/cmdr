/**
 * What the dialog's keys and row menu DO (`sync-keys.ts` says what a key means): the cursor, the selection, the
 * row actions, and the one-line notice that says why something didn't happen. Never silent: a key the cursor row
 * can't take, a refusal from the backend, or a side with no file each leave a notice.
 *
 * Bulk keys act on the selected rows, else the cursor row. The selection holds plan indices; the backend keeps
 * only the ones the show-filters show (hidden rows never change).
 */
import type { BulkAction, ShowFilters, Side } from '#lib/tauri-commands/index.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { rowRefusal, rowRefusalText } from './action-refusal'
import { ACTION_GLYPH, type SyncItemRow } from './row-label'
import { syncProblemText } from './sync-errors'
import type { SyncKeyAction } from './sync-keys'
import { isSideFile, sideFileOf, type SideFile } from './sync-row-files'
import type { SyncSession } from './sync-session.svelte'

export interface SyncFileOpeners {
  view: (file: SideFile) => Promise<void>
  edit: (file: SideFile) => Promise<void>
  /** Whether the panel opened (`false` off this Mac). */
  quickLook: (file: SideFile) => Promise<boolean>
  copyText: (text: string) => Promise<void>
}

export interface SyncGridControllerDeps {
  session: SyncSession
  files: SyncFileOpeners
  /** Brings a view row into view after a key moved the cursor there. */
  reveal: (index: number) => void
  /** The parts of the dialog this controller can't do itself. */
  dialog: {
    close: () => void
    confirm: () => void
    openRowMenu: () => void
  }
}

export class SyncGridController {
  cursor = $state(-1)
  notice = $state<string | null>(null)
  private anchor = -1

  constructor(private readonly deps: SyncGridControllerDeps) {}

  private get session(): SyncSession {
    return this.deps.session
  }

  /** The item row under the cursor, if it's loaded and not a folder header. */
  get cursorRow(): SyncItemRow | null {
    const row = this.session.getRow(this.cursor)
    return row?.type === 'item' ? row : null
  }

  moveCursor(index: number): void {
    this.cursor = index
    this.anchor = index
  }

  /** A click: plain moves the cursor and drops the selection, ⌘ toggles the row, ⇧ selects from the anchor. */
  async click(index: number, event: Pick<MouseEvent, 'metaKey' | 'shiftKey'>): Promise<void> {
    const row = this.session.getRow(index)
    if (row?.type !== 'item') return
    if (event.metaKey) {
      this.toggle(row.index)
    } else if (event.shiftKey && this.anchor >= 0) {
      await this.selectRange(this.anchor, index)
    } else {
      this.session.selected.clear()
    }
    this.cursor = index
    if (!event.shiftKey) this.anchor = index
  }

  private toggle(item: number): void {
    if (this.session.selected.has(item)) this.session.selected.delete(item)
    else this.session.selected.add(item)
  }

  private async selectRange(from: number, to: number): Promise<void> {
    const [start, end] = from <= to ? [from, to] : [to, from]
    for (let index = start; index <= end; index++) {
      const row = await this.session.rowAt(index)
      if (row?.type === 'item') this.session.selected.add(row.index)
    }
  }

  /** ⇧↑ / ⇧↓: extend the selection over the rows the cursor passes. */
  async extend(next: number): Promise<void> {
    if (next < 0) return
    await this.selectRange(this.cursor < 0 ? next : this.cursor, next)
    this.cursor = next
    this.deps.reveal(next)
  }

  /** ⌘A: every row of the view. */
  async selectAll(): Promise<void> {
    for (const row of await this.session.allRows()) if (row.type === 'item') this.session.selected.add(row.index)
  }

  async run(action: SyncKeyAction): Promise<void> {
    this.notice = null
    switch (action.kind) {
      case 'close':
        this.deps.dialog.close()
        return
      case 'confirm':
        this.deps.dialog.confirm()
        return
      case 'contextMenu':
        this.deps.dialog.openRowMenu()
        return
      case 'profiles':
      case 'saveProfile':
        // Profiles arrive with the profiles menu (F2, ⌘S); until then the keys do nothing.
        return
      case 'filter':
        this.toggleFilter(action.filter)
        return
      default:
        await this.runAsync(action)
    }
  }

  private async runAsync(action: SyncKeyAction): Promise<void> {
    switch (action.kind) {
      case 'compare':
        return this.session.compare()
      case 'swap':
        return this.session.swap()
      case 'bulk':
        return this.bulk(action.bulk)
      case 'view':
      case 'edit':
        return this.openSide(action.kind, action.side)
      case 'quickLook':
        return this.quickLook()
      case 'compareContent':
        return this.compareContent()
      case 'folder':
        return this.jumpFolder(action.step)
      case 'copyRows':
        return this.copyRows()
      default:
        return undefined
    }
  }

  toggleFilter(key: keyof ShowFilters): void {
    this.session.setFilters({ ...this.session.filters, [key]: !this.session.filters[key] })
  }

  private async bulk(bulk: BulkAction): Promise<void> {
    const selected = [...this.session.selected]
    const row = this.cursorRow
    if (selected.length === 0) {
      if (!row) return
      const refusal = rowRefusal(row, bulk, this.session.comparedMode)
      if (refusal) {
        this.notice = rowRefusalText(refusal)
        return
      }
    }
    const problem = await this.session.setActions(selected.length > 0 ? selected : [row?.index ?? -1], bulk)
    if (problem) this.notice = syncProblemText(problem)
  }

  private async sideFile(side: Side): Promise<SideFile | null> {
    const row = this.cursorRow
    if (!row) return null
    const answer = await sideFileOf((index) => this.session.rowAt(index), this.session.roots, {
      viewIndex: this.cursor,
      row,
      side,
    })
    if (isSideFile(answer)) return answer
    this.notice = tString(answer.type === 'noFile' ? 'sync.refusal.noFile' : 'sync.refusal.notAFile')
    return null
  }

  private async openSide(kind: 'view' | 'edit', side: Side): Promise<void> {
    const file = await this.sideFile(side)
    if (file) await this.deps.files[kind](file)
  }

  private async quickLook(): Promise<void> {
    const row = this.cursorRow
    if (!row) return
    const file = await this.sideFile(row.leftName !== null && row.leftKind !== 'folder' ? 'left' : 'right')
    if (file && !(await this.deps.files.quickLook(file))) this.notice = tString('sync.refusal.quickLookUnavailable')
  }

  private async compareContent(): Promise<void> {
    const row = this.cursorRow
    if (!row) return
    const problem = await this.session.compareContent(row.index)
    if (problem) this.notice = syncProblemText(problem)
  }

  /** ⌘PageDown / ⌘PageUp: the first row of the next or previous folder. */
  private async jumpFolder(step: 1 | -1): Promise<void> {
    const total = this.session.total
    const isHeader = async (index: number) => (await this.session.rowAt(index))?.type === 'folder'
    let target = -1
    if (step === 1) {
      for (let index = Math.max(this.cursor, 0) + 1; index < total && target < 0; index++) {
        if (await isHeader(index)) target = index + 1
      }
    } else {
      let own = -1
      for (let index = this.cursor - 1; index >= 0 && own < 0; index--) if (await isHeader(index)) own = index
      for (let index = own - 1; index >= 0 && target < 0; index--) if (await isHeader(index)) target = index + 1
      if (own > 0 && target < 0) target = 0
    }
    if (target < 0 || target >= total) return
    this.moveCursor(target)
    this.deps.reveal(target)
  }

  /** ⌘C: the selected rows (else the cursor row) as tab-separated text, TC's columns. */
  private async copyRows(): Promise<void> {
    const rows =
      this.session.selected.size > 0
        ? (await this.session.allRows()).filter(
            (row): row is SyncItemRow => row.type === 'item' && this.session.selected.has(row.index),
          )
        : [this.cursorRow].filter((row): row is SyncItemRow => row !== null)
    if (rows.length === 0) return
    const line = (row: SyncItemRow) =>
      [row.leftName ?? '', row.leftSize ?? '', ACTION_GLYPH[row.action], row.rightSize ?? '', row.rightName ?? '']
        .map(String)
        .join('\t')
    await this.deps.files.copyText(rows.map(line).join('\n'))
  }
}
