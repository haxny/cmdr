/**
 * What the dialog's keys and row menu DO (`sync-keys.ts` says what a key means): the cursor, the selection, the
 * row actions, and the one-line notice that says why something didn't happen. Never silent: rows a key left alone
 * (the backend says which and why), a refusal, a side with no file, or a key with no row to act on each leave a
 * notice.
 *
 * Bulk keys act on the selected rows, else the cursor row. The selection holds plan indices; the backend keeps
 * only the ones the show-filters show (hidden rows never change, and are counted). Every row this acts on is read
 * from the CURRENT view (`currentRow` / `rowAt`), never a row kept on screen from an older one.
 */
import type { BulkAction, ShowFilters, Side } from '#lib/tauri-commands/sync.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { actionsNotice } from './action-refusal'
import { ACTION_GLYPH, type SyncItemRow } from './row-label'
import { syncProblemText } from './sync-errors'
import type { SyncKeyAction } from './sync-keys'
import { isSideFile, sideFileOf, type SideFile } from './sync-row-files'
import type { SyncSession } from './sync-session.svelte'

/** Typed letters within this long of each other build one name to jump to (Cmdr's type-to-jump). */
const TYPE_TO_JUMP_MS = 1000

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
  now?: () => number
}

export class SyncGridController {
  cursor = $state(-1)
  notice = $state<string | null>(null)
  /** Where ⇧-click and ⇧↑↓ extend from. */
  private anchor = -1
  /** The anchor before the latest cursor move: a click moves the cursor (`ColumnList`) before it reports itself. */
  private anchorBeforeMove = -1
  private typed = { text: '', at: 0 }

  constructor(private readonly deps: SyncGridControllerDeps) {}

  private get session(): SyncSession {
    return this.deps.session
  }

  /** The item row under the cursor in the current view, if it's an item and has arrived. */
  get cursorRow(): SyncItemRow | null {
    const row = this.session.currentRow(this.cursor)
    return row?.type === 'item' ? row : null
  }

  /** The list moved the cursor (a cursor key or a click). */
  moveCursor(index: number): void {
    this.anchorBeforeMove = this.anchor
    this.cursor = index
    this.anchor = index
  }

  /** A click: plain drops the selection, ⌘ toggles the row, ⇧ selects from the anchor the click found. */
  async click(index: number, event: Pick<MouseEvent, 'metaKey' | 'shiftKey'>): Promise<void> {
    // `ColumnList` reports the cursor move first; the anchor that counts is the one from before it.
    const anchor = this.cursor === index && this.anchor === index ? this.anchorBeforeMove : this.anchor
    this.cursor = index
    if (event.shiftKey && anchor >= 0) {
      this.anchor = anchor
      await this.selectRange(anchor, index)
      return
    }
    this.anchor = index
    const row = this.session.currentRow(index)
    if (row?.type !== 'item') return
    if (event.metaKey) this.toggle(row.index)
    else this.session.selected.clear()
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
      case 'selectAll':
        return this.selectAll()
      default:
        return undefined
    }
  }

  toggleFilter(key: keyof ShowFilters): void {
    this.session.setFilters({ ...this.session.filters, [key]: !this.session.filters[key] })
  }

  /** The cursor row, or a notice saying there's no row to act on (a folder header, a row still loading). */
  private rowOrNotice(): SyncItemRow | null {
    const row = this.cursorRow
    if (!row) this.notice = tString('sync.refusal.noRow')
    return row
  }

  private async bulk(bulk: BulkAction): Promise<void> {
    let items = [...this.session.selected]
    if (items.length === 0) {
      const row = this.rowOrNotice()
      if (!row) return
      items = [row.index]
    }
    const answer = await this.session.setActions(items, bulk)
    this.notice =
      answer.type === 'refused' ? syncProblemText(answer.problem) : actionsNotice(answer.skipped, answer.hidden)
  }

  private sideFile(row: SyncItemRow, side: Side): SideFile | null {
    const answer = sideFileOf(this.session.roots, row, side)
    if (isSideFile(answer)) return answer
    this.notice = tString(answer.type === 'noFile' ? 'sync.refusal.noFile' : 'sync.refusal.notAFile')
    return null
  }

  private async openSide(kind: 'view' | 'edit', side: Side): Promise<void> {
    const row = this.rowOrNotice()
    const file = row && this.sideFile(row, side)
    if (file) await this.deps.files[kind](file)
  }

  private async quickLook(): Promise<void> {
    const row = this.rowOrNotice()
    if (!row) return
    const file = this.sideFile(row, row.leftRelPath !== null && row.leftKind !== 'folder' ? 'left' : 'right')
    if (file && !(await this.deps.files.quickLook(file))) this.notice = tString('sync.refusal.quickLookUnavailable')
  }

  private async compareContent(): Promise<void> {
    const row = this.rowOrNotice()
    if (!row) return
    const problem = await this.session.compareContent(row.index)
    if (problem) this.notice = syncProblemText(problem)
  }

  private land(target: number | null): void {
    if (target === null) return
    this.moveCursor(target)
    this.deps.reveal(target)
  }

  /** ⌘PageDown / ⌘PageUp: the first row of the next or previous folder (the backend walks its own view). */
  private async jumpFolder(step: 1 | -1): Promise<void> {
    const from = Math.max(this.cursor, 0)
    this.land(await this.session.find(step === 1 ? { type: 'nextFolder', from } : { type: 'previousFolder', from }))
  }

  /** A letter typed on the grid: the next row whose name starts with what was typed lately (TC's quick search). */
  async typeToJump(char: string): Promise<void> {
    const now = (this.deps.now ?? Date.now)()
    const fresh = now - this.typed.at > TYPE_TO_JUMP_MS
    this.typed = { text: fresh ? char : this.typed.text + char, at: now }
    // A new search starts below the cursor; a longer one keeps matching the row it's on.
    const from = Math.max(this.cursor, 0) + (fresh ? 1 : 0)
    this.land(await this.session.find({ type: 'name', from, prefix: this.typed.text }))
  }

  /** ⌘C: the selected rows (else the cursor row) as tab-separated text, TC's columns. */
  private async copyRows(): Promise<void> {
    const rows =
      this.session.selected.size > 0
        ? (await this.session.allRows()).filter(
            (row): row is SyncItemRow => row.type === 'item' && this.session.selected.has(row.index),
          )
        : [this.rowOrNotice()].filter((row): row is SyncItemRow => row !== null)
    if (rows.length === 0) return
    const line = (row: SyncItemRow) =>
      [row.leftName ?? '', row.leftSize ?? '', ACTION_GLYPH[row.action], row.rightSize ?? '', row.rightName ?? '']
        .map(String)
        .join('\t')
    await this.deps.files.copyText(rows.map(line).join('\n'))
  }
}
