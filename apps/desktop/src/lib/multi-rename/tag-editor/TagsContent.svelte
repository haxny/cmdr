<script lang="ts">
    /**
     * The Tags window's content: two panes like the main window's file panes, Album (the tags the files share, then
     * the cover) | Tracks (selected like files; the tools work on the selection), a one-line status, and the
     * function-key bar (`TagsFunctionKeyBar`). Every tool and the write open in a dialog over the panes
     * (`TagToolDialogs`, `WriteDialog`), so nothing ever pushes or resizes the panes. `runCommand` runs a `Tags window`
     * command, from the bar or from the window's keydown (`TagsWindow.svelte`). Tab switches the panes
     * (`tags-panes.svelte.ts`). ⌘X / ⌘C / ⌘V from the native menu act on the field under the cursor (`clipboard`).
     * ⌘⏎ and ⌘S write at once and say in the status line how many files (`writeAtOnce`); ⇧⌘S opens the Write dialog,
     * the change list first.
     * It opens the tag session when it mounts.
     */
    import { onDestroy, onMount, untrack } from 'svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import type { ContentKind, TagField, TagTool, TrackSort } from '#lib/ipc/bindings.js'
    import {
        openTagsInPlayer,
        type TagApplyReport,
        type TagEdit,
        type TagEditorView,
    } from '#lib/tauri-commands/index.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import Button from '#lib/ui/Button.svelte'
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import { getFirstShiftShortcutReactive } from '#lib/shortcuts/reactive-shortcuts.svelte.js'
    import { joinSentences } from '#lib/intl/sentence-join.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import AlbumPane from './AlbumPane.svelte'
    import { followViewScope, scopedAlbumEdits } from './album-scope.svelte'
    import { fileNameOf, outcomeOf, refusalOf } from './apply-outcome'
    import { answerWriteAnyway } from './write-anyway'
    import { KIND_LABELS } from './write-kind'
    import CoverPanel from './CoverPanel.svelte'
    import CoverViewer from './CoverViewer.svelte'
    import { COVER_ERROR_KEYS, type CoverShown } from './cover-choices'
    import { createTagsClipboard, readablePathsOf } from './tags-clipboard'
    import TagsFunctionKeyBar from './TagsFunctionKeyBar.svelte'
    import TagToolDialogs from './TagToolDialogs.svelte'
    import TracksPane from './TracksPane.svelte'
    import WouldDropDialog from './WouldDropDialog.svelte'
    import WriteDialog from './WriteDialog.svelte'
    import type { RunScoped, TagToolId } from './tag-tools'
    import {
        createTagsKeyEcho,
        enabledTagsCommands,
        type TagsBarCommandId,
        type TagsCommandId,
        type TagsKeyRoad,
    } from './tags-function-keys'
    import { createTagsPanes, type TagsPaneId } from './tags-panes.svelte'
    import { createTrackSelection } from './track-selection.svelte'
    import { createTrackPlayer } from './track-player.svelte'
    import type { TagEditorState, TagStateError, WriteNowResult } from './tag-editor-state.svelte'

    interface Props {
        state: TagEditorState
    }

    const { state: tags }: Props = $props()

    /** The selected tracks; the tools work on these alone. Created once per mount, synced on every view change. */
    const selection = createTrackSelection()
    const allPaths = $derived(tags.view?.tracks.map((track) => track.path) ?? [])
    /** Changes with the selected tracks and the track list: a file-name preview is only good for the tracks it listed. */
    const scopeKey = $derived(JSON.stringify([[...selection.selected].sort(), allPaths]))

    let selectionGone = $state(false)

    // Converges: `sync` writes only when the list of paths changed.
    $effect(() => {
        selection.sync(allPaths)
    })

    // The Album pane sums up the selected tracks (every track while none is).
    const viewScope = followViewScope({
        selected: () => selection.selected,
        shownCount: () => tags.view?.scopeCount,
        settled: () => tags.settled(),
        setViewScope: (paths) => tags.setViewScope(paths),
    })

    // Selecting again answers the "no longer here" notice.
    $effect(() => {
        if (selection.count > 0) selectionGone = false
    })

    let albumPane = $state<{ focus: () => void; editAtCursor: () => void; settleEditor: () => boolean }>()
    let tracksPane = $state<{
        focus: () => void
        editAtCursor: (field?: 'title' | 'track') => void
        playAtCursor: () => void
        activeField: () => 'title' | 'track'
        settleEditor: () => boolean
    }>()
    /** One key press by both roads (keydown and the native menu's accelerator) acts once. */
    const isEcho = createTagsKeyEcho()
    const panes = createTagsPanes({
        focus: (pane) => {
            ;(pane === 'album' ? albumPane : tracksPane)?.focus()
        },
    })

    // The cursor stays on its track when a sort (or any reorder) moves it, instead of on the same row number.
    let listedPaths: string[] = []
    $effect(() => {
        const paths = allPaths
        const cursorPath = listedPaths.at(untrack(() => panes.tracksCursor))
        listedPaths = paths
        const moved = cursorPath === undefined ? -1 : paths.indexOf(cursorPath)
        if (moved >= 0 && moved !== untrack(() => panes.tracksCursor)) panes.tracksCursor = moved
    })

    // The window opens with the Album pane focused, as the main window opens with a file pane focused.
    let focusedOnce = false
    $effect(() => {
        if (focusedOnce || albumPane === undefined) return
        focusedOnce = true
        albumPane.focus()
    })

    /** ⌘A from the native menu (`TagsWindow`): every track, as in a file pane. */
    export function selectAllTracks(): void {
        selection.selectAll()
    }

    /** ⌘⇧A from the native menu (`TagsWindow`): no track selected. */
    export function deselectAllTracks(): void {
        selection.deselectAll()
    }

    /** Focus is inside one of the panes (not the tools bar, the cover, or Write): only then a pane draws as focused. */
    let inPanes = $state(false)

    onMount(() => {
        void tags.open()
    })

    /**
     * The tracks play here, one at a time. Closing the window unmounts this component: playback stops and its URL is
     * released.
     */
    const player = createTrackPlayer(tags.sessionId)
    onDestroy(() => {
        player.stop()
    })

    /** Why the last Open in foobar2000 failed, when it did. */
    let playerError = $state<MessageKey | null>(null)

    /** Opens the selected tracks (every track when none is selected) in foobar2000, else each in its default app. */
    async function openInPlayer(): Promise<void> {
        playerError = null
        // Unreadable tracks are not the session's tracks for the backend (it refuses the call for one), so they never go.
        const readable = new Set(tags.view?.tracks.filter((track) => track.readError === null).map((track) => track.path))
        const res = await runScoped(async (scope) => {
            const paths = (scope ?? allPaths).filter((path) => readable.has(path))
            if (paths.length === 0) return 'nothingReadable' as const
            return await openTagsInPlayer(tags.sessionId, paths)
        })
        if (res === undefined) return
        if (res === 'nothingReadable') {
            playerError = 'multiRename.tags.play.nothingReadable'
            return
        }
        if (res.ok) return
        playerError =
            res.error.kind === 'playerUnavailable' ? 'multiRename.tags.play.playerUnavailable' : 'multiRename.tags.play.openFailed'
    }

    /** What a failed edit or cover pick says; a failure to read the tags at all is worded where there is no view. */
    function editErrorKey(error: TagStateError): MessageKey | null {
        switch (error.kind) {
            // The text tools' panels word these where the user typed them.
            case 'badRegex':
            case 'badMask':
            case 'badPattern':
                return null
            case 'undo':
            case 'noAppDataDir':
                return 'multiRename.tags.undoStoreUnavailable'
            default:
                return COVER_ERROR_KEYS[error.kind] ?? 'multiRename.tags.editFailed'
        }
    }

    /**
     * The field the user is on: the Album pane's cursor row while that pane is active, Title while the Tracks pane
     * is, none otherwise (focus on the cover, or out of the window). A tool takes it when it opens (Case, Replace,
     * Clear); the bar never takes focus, so a press on it keeps the pane.
     */
    const focusedField = $derived.by((): TagField | null => {
        if (panes.active === 'tracks') return 'title'
        if (panes.active !== 'album') return null
        const shared = tags.view?.shared ?? []
        return shared.at(Math.max(0, Math.min(panes.albumCursor, shared.length - 1)))?.field ?? null
    })

    function followFocus(target: EventTarget | null): void {
        const slot = target instanceof Element ? target.closest<HTMLElement>('[data-pane]') : null
        const inCover = target instanceof Element && target.closest('.cover-group') !== null
        inPanes = slot !== null && !inCover
        panes.noteFocus(inPanes && slot !== null ? (slot.dataset.pane as TagsPaneId) : null)
    }

    /**
     * A tool works on what the files hold now, so an edit a box just sent lands first, and on the selected tracks
     * alone (`null` for all). Resolves `undefined` without calling when the selected tracks are gone.
     */
    const runScoped: RunScoped = async (call) => {
        await tags.settled()
        const scope = selection.scope()
        if (scope.kind === 'gone') {
            // The selected tracks are gone: never widen to every track. The next run takes them all.
            selection.clearGone()
            selectionGone = true
            return undefined
        }
        selectionGone = false
        return await call(scope.kind === 'some' ? scope.paths : null)
    }

    async function runTool(tool: TagTool): Promise<void> {
        await runScoped((scope) => tags.tool(tool, scope))
    }

    /** The paths of the tracks whose tags were read: an edit can name only these. */
    const readablePaths = (): Set<string> => readablePathsOf(tags.view)

    /** An Album pane edit (typing, a pick, Clear, Delete, paste, cut): on the selected readable tracks, else all. */
    async function editAlbum(edits: TagEdit[]): Promise<void> {
        const flushing = viewScope.flush() // The pane shows the scope an edit goes to before it goes.
        if (flushing !== null) await flushing
        await runScoped((scope) => tags.edit(scopedAlbumEdits(edits, scope, readablePaths())))
    }

    /** Sets `field` to `values` on the selected tracks (the readable ones), or on every file when none is selected. */
    async function setFieldScoped(field: TagField, values: string[]): Promise<void> {
        await editAlbum([{ kind: 'setAll', field, values }])
    }

    /** Empties `field` on the selected tracks (the readable ones), or on every file when none is selected. */
    async function clearField(field: TagField): Promise<void> {
        await setFieldScoped(field, [])
    }

    /**
     * Delete / Backspace in a pane, with no confirmation: the Album pane (`path` null) empties the field in Clear's
     * scope; the Tracks pane empties it on the one track.
     */
    async function clearTarget(target: { field: TagField; path: string | null }): Promise<void> {
        if (target.path === null) await clearField(target.field)
        else await tags.edit([{ kind: 'setOne', path: target.path, field: target.field, values: [] }])
    }

    /** ⌘X / ⌘C / ⌘V from the native menu (`TagsWindow`): the field under the cursor (`tags-clipboard.ts`). */
    export const clipboard = createTagsClipboard({
        tags,
        panes,
        activeTrackField: () => tracksPane?.activeField() ?? 'title',
        selection,
        setFieldScoped,
        clearTarget,
        hasDialog: () => hasDialog(),
        say: (key) => {
            notice = key
        },
    })

    /** The tool dialog showing, with the field the user was on when it opened. */
    let tool = $state<{ id: TagToolId; field: TagField | null } | null>(null)
    /** The Write dialog showing: listing and writing (⌘⇧S), or undoing the last write (⌘Z). */
    let writeMode = $state<'write' | 'undo' | null>(null)
    /** Files a write at once would make lose tag data ask first, over the panes (`WouldDropDialog`). */
    let dropChoice = $state.raw<{
        reports: TagApplyReport[]
        kind: ContentKind
        /** The view the choice was asked for: an edit since then lists again. */
        seen: TagEditorView | null
        answering: boolean
    } | null>(null)
    /** The ⌘⏎ applies the Write dialog's list continues (`openReview`). */
    let reviewContinues: TagApplyReport[] = []
    /** The window was asked to close while a write ran: the Write dialog says to wait until it's done. */
    let closeWaits = $state(false)
    /** The cover opened large (`CoverViewer`), from a click on its thumbnail. */
    let viewing = $state<CoverShown | null>(null)
    /** A dialog is over the panes: the cover section takes no paste or drop meanwhile. */
    const dialogOpen = $derived(tool !== null || writeMode !== null || viewing !== null || dropChoice !== null)
    /** A key's answer that has nowhere else to go ("There's no tag write to undo."); the next command clears it. */
    let notice = $state<MessageKey | null>(null)

    /** The bar's commands that can act now (`enabledTagsCommands`). */
    const enabledCommands = $derived(enabledTagsCommands(tags.view))

    function openTool(id: TagToolId): void {
        tool = { id, field: focusedField }
    }

    /**
     * ⌘Z: the last pending edit at once; with none, the last write (asks first). The choice waits for the edits still
     * out, so a Delete just pressed is the edit ⌘Z takes back, never the write.
     */
    async function undoLast(): Promise<void> {
        await tags.settled()
        if (tags.view?.canUndoEdit === true) {
            if (await tags.undoEdit()) notice = 'multiRename.tags.undo.editUndone'
        } else if (tags.undoable === null) notice = 'multiRename.tags.undo.nothing'
        else writeMode = 'undo'
    }

    /** ⇧⌘Z: the last undone edit again. Nothing to redo says nothing. */
    async function redoLast(): Promise<void> {
        await tags.settled()
        if (tags.view?.canRedoEdit !== true) return
        if (await tags.redoEdit()) notice = 'multiRename.tags.undo.editRedone'
    }

    /** How long the status line keeps a write's summary, unless an edit comes first. */
    const SUMMARY_SHOWN_MS = 10_000

    /** One sentence of a write's summary in the status line. */
    interface SummaryPart {
        key: MessageKey
        params?: Record<string, string | number>
    }

    /**
     * What the last write at once (⌘⏎, ⌘S) did, and the view it left: the next edit (a new view) or 10 s ends it,
     * whichever comes first.
     */
    let summary = $state.raw<{ parts: SummaryPart[]; role: 'status' | 'alert'; view: TagEditorView | null } | null>(null)
    let summaryTimer: ReturnType<typeof setTimeout> | undefined
    const shownSummary = $derived(summary !== null && summary.view === tags.view ? summary : null)
    onDestroy(() => {
        clearTimeout(summaryTimer)
    })

    function showSummary(parts: SummaryPart[], role: 'status' | 'alert' = 'status'): void {
        clearTimeout(summaryTimer)
        summary = { parts, role, view: tags.view }
        summaryTimer = setTimeout(() => {
            summary = null
        }, SUMMARY_SHOWN_MS)
    }

    /** A write at once is on its way: a second press does nothing (no queue), and the status line says so. */
    let writingNow = $state(false)

    /**
     * ⌘⏎ / ⌘S: writes at once (`writeNow`, the Write dialog's own write without its list) and says in the status line
     * how many files it wrote. The player stops first, as before the dialog's write.
     */
    async function writeAtOnce(): Promise<void> {
        if (writingNow || tags.writing) return
        writingNow = true
        summary = null
        try {
            const res = await tags.writeNow({
                onWriting: () => {
                    player.stop()
                },
            })
            answerWrite(res)
        } finally {
            writingNow = false
        }
    }

    function answerWrite(res: WriteNowResult): void {
        switch (res.kind) {
            case 'done':
                showDone(res.written, res.failed + res.changedOnDisk)
                break
            case 'wouldDrop':
                dropChoice = { reports: [res.report], kind: res.writtenAs, seen: tags.view, answering: false }
                break
            case 'refused': {
                const { noneWritten, reason } = refusalOf(res.error.kind)
                const parts: SummaryPart[] = [{ key: reason }]
                showSummary(noneWritten ? [{ key: 'multiRename.tags.write.refused' }, ...parts] : parts, 'alert')
                break
            }
            case 'listFailed':
                showSummary([{ key: 'multiRename.tags.write.listFailed' }], 'alert')
                break
            case 'reviewFirst':
                // A removal is written only from the list that names it; the list continues any apply already made.
                openReview(res.after)
                showSummary([
                    {
                        key: 'multiRename.tags.write.reviewRemovals',
                        params: {
                            count: res.removing,
                            countText: formatInteger(res.removing),
                            kind: tString(KIND_LABELS[res.writtenAs]),
                        },
                    },
                ])
                break
            case 'changed':
                // An edit landed between the list and the write: nothing unlisted is written.
                openReview()
                showSummary([{ key: 'multiRename.tags.write.listedAgain' }])
                break
            case 'nothing':
                showSummary([{ key: 'multiRename.tags.write.nothingToWrite' }])
                break
            case 'stillBusy':
                showSummary([{ key: 'multiRename.tags.write.stillBusy' }], 'alert')
                break
        }
    }

    /** How many files were written, and how many weren't (pointing at Review, which lists them). */
    function showDone(written: number, notWritten: number, after: SummaryPart[] = []): void {
        const count = { count: written, countText: formatInteger(written) }
        const done: SummaryPart =
            notWritten === 0
                ? { key: 'multiRename.tags.write.doneSummary', params: count }
                : {
                      key: 'multiRename.tags.write.doneWithProblems',
                      params: {
                          ...count,
                          notWritten,
                          notWrittenText: formatInteger(notWritten),
                          shortcut: getFirstShiftShortcutReactive('tags.review') ?? '',
                      },
                  }
        showSummary([done, ...after], after.length > 0 ? 'alert' : 'status')
    }

    /** The summary of every apply of one press (the first, and a "Write anyway"), each file once. */
    function showOutcome(reports: TagApplyReport[], after: SummaryPart[] = []): void {
        const outcome = outcomeOf(reports)
        showDone(outcome.written, outcome.failed.length + outcome.changedOnDisk.length + outcome.skipped.length, after)
    }

    /** "Skip these files": the would-drop files stay as they are, counted as not written. */
    function skipDropped(choice: NonNullable<typeof dropChoice>): void {
        dropChoice = null
        showOutcome(choice.reports)
    }

    /** "Write anyway" (`answerWriteAnyway`, shared with the Write dialog, so both write and re-list alike). */
    async function writeAnyway(choice: NonNullable<typeof dropChoice>): Promise<void> {
        if (choice.answering) return
        dropChoice = { ...choice, answering: true }
        const answer = await answerWriteAnyway(tags, choice, () => {
            player.stop()
        })
        switch (answer.kind) {
            case 'stillBusy':
                dropChoice = { ...choice, answering: false }
                notice = 'multiRename.tags.write.stillBusy'
                break
            case 'changed':
                // An edit landed while the choice showed: nothing unlisted is written. The list opens, continuing this
                // press, so its write's Undo reverses the first apply too.
                dropChoice = null
                openReview(choice.reports)
                break
            case 'written':
                dropChoice = null
                showOutcome(answer.reports)
                break
            case 'refused':
                dropChoice = null
                showOutcome(choice.reports, [{ key: 'multiRename.tags.write.stopped' }])
                break
        }
    }

    /** ⌘⇧S: the Write dialog, the change list first; `continuing`: the applies of a ⌘⏎ press it continues. */
    function openReview(continuing: TagApplyReport[] = []): void {
        reviewContinues = continuing
        writeMode = 'write'
    }

    const COMMANDS: Record<TagsCommandId, () => void> = {
        'tags.edit': () => {
            if (panes.active === 'tracks') tracksPane?.editAtCursor()
            else albumPane?.editAtCursor()
        },
        'tags.editNumber': () => tracksPane?.editAtCursor('track'),
        'tags.play': () => tracksPane?.playAtCursor(),
        'tags.openInPlayer': () => void openInPlayer(),
        'tags.replace': () => { openTool('replace') },
        'tags.case': () => { openTool('case') },
        'tags.format': () => { openTool('format') },
        'tags.cleanUp': () => { openTool('cleanUp') },
        'tags.fromName': () => { openTool('fromName') },
        'tags.number': () => { openTool('number') },
        'tags.clearField': () => { openTool('clearField') },
        'tags.removeExtras': () => { openTool('extras') },
        'tags.write': () => void writeAtOnce(),
        'tags.review': () => { openReview() },
        'tags.undo': () => void undoLast(),
        'tags.redo': () => void redoLast(),
    }

    /** Whether `id` can act on the tags as they are now. */
    function canRun(id: TagsCommandId): boolean {
        if (id === 'tags.undo' || id === 'tags.redo') return tags.view?.editable === true
        return enabledCommands.has(id)
    }

    /**
     * Runs a `Tags window` command: from the bar (no `road`), or from a key (`road`: the window's keydown, or the
     * native menu's accelerator, `TagsMenuKey`). A key that is the echo of the same press by the other road does
     * nothing. An open value box is settled first (its text sent, as a click away does); when its text can't be sent
     * the box keeps it with its hint and the command doesn't run. Returns whether it acted; a command that can't act
     * now (tags still being read, read-only, no extras, a dialog open) does nothing, and the status line or the
     * reading notice already says why.
     */
    export function runCommand(id: TagsCommandId, road?: TagsKeyRoad): boolean {
        if (hasDialog() || !canRun(id)) return false
        if (road !== undefined && isEcho(id, road)) return false
        const settled = (albumPane?.settleEditor() ?? true) && (tracksPane?.settleEditor() ?? true)
        if (!settled) return false
        notice = null
        COMMANDS[id]()
        return true
    }

    /** A tool or the Write dialog is open: it owns the keys, and Escape closes it, never the window. */
    export function hasDialog(): boolean {
        return dialogOpen
    }

    /**
     * The Write dialog closed. Its per-file report goes with it, so when files were left unwritten the status line
     * keeps saying so; Write lists their changes again.
     */
    function closeWrite(notWritten: number): void {
        writeMode = null
        reviewContinues = []
        closeWaits = false
        if (notWritten > 0) notice = 'multiRename.tags.write.someLeft'
    }

    /**
     * The window was asked to close while a write or an undo is on its way (`TagsWindow`'s discard guard): the Write
     * dialog, which shows it, says to wait; its outcome follows once it's done.
     */
    export function waitForWrite(): void {
        closeWaits = true
        if (writeMode === null) notice = 'multiRename.tags.window.closeWaits'
    }

    /** Writes at once (⌘⏎, ⌘S). */
    export function start(): void {
        runCommand('tags.write')
    }

    async function listCandidates() {
        const res = await tags.coverCandidates()
        return res.ok ? res.value : null
    }
</script>

<div
    class="tags-content"
    onfocusin={(e) => { followFocus(e.target) }}
    onfocusout={(e) => { if (!(e.relatedTarget instanceof Node) || !e.currentTarget.contains(e.relatedTarget)) { inPanes = false; panes.noteFocus(null) } }}
>
    {#if tags.view === null}
        {#if tags.error !== null}
            <p class="opening" role="alert">
                {tString(tags.error.kind === 'readTimedOut' ? 'multiRename.tags.readTimedOut' : 'multiRename.tags.error')}
            </p>
        {:else}
            <p class="opening" role="status">{tString('multiRename.tags.reading')}</p>
        {/if}
    {:else}
        {@const view = tags.view}
        <div class="panes">
            <div class="pane-slot album" data-pane="album">
                <AlbumPane
                    bind:this={albumPane}
                    rows={view.shared}
                    editable={view.editable}
                    badDates={view.badDates}
                    scope={view.scopeCount === null ? null : { count: view.scopeCount, total: view.tracks.length }}
                    focused={inPanes && panes.active === 'album'}
                    cursor={panes.albumCursor}
                    onCursorChange={(index: number) => { panes.albumCursor = index }}
                    onEdit={(edits: TagEdit[]) => editAlbum(edits)}
                    onClear={(target) => void clearTarget(target)}
                    onSwitchPane={() => { panes.switchPane() }}
                    isEcho={(id: TagsCommandId) => isEcho(id, 'keyboard')}
                    cover={coverGroup}
                />
            </div>
            <div class="divider" aria-hidden="true"></div>
            <div class="pane-slot tracks" data-pane="tracks">
                <TracksPane
                    bind:this={tracksPane}
                    tracks={view.tracks}
                    editable={view.editable}
                    focused={inPanes && panes.active === 'tracks'}
                    cursor={panes.tracksCursor}
                    onCursorChange={(index: number) => { panes.tracksCursor = index }}
                    {selection}
                    playing={player.playing}
                    onPlay={(path: string) => void player.toggle(path)}
                    onEdit={(edits: TagEdit[]) => tags.edit(edits)}
                    onClear={(target) => void clearTarget(target)}
                    onSwitchPane={() => { panes.switchPane() }}
                    sort={view.sort}
                    onSort={(sort: TrackSort) => void tags.sortTracks(sort)}
                    isEcho={(id: TagsCommandId) => isEcho(id, 'keyboard')}
                    footer={tracksFooter}
                />
            </div>
        </div>
        <!-- One line, always there, so a notice coming or going never moves the panes. -->
        <div class="status">
            <!-- eslint-disable @typescript-eslint/no-confusing-void-expression -- Svelte {@render} syntax -->
            {#if !view.editable}
                {@render line('multiRename.tags.readOnly', 'status')}
            {/if}
            {#if tags.error !== null}
                {@const errorKey = editErrorKey(tags.error)}
                {#if errorKey !== null}{@render line(errorKey, 'alert')}{/if}
            {/if}
            {#if player.failed}
                {@render line('multiRename.tags.play.failed', 'alert')}
            {/if}
            {#if playerError !== null}
                {@render line(playerError, 'alert')}
            {/if}
            {#if selectionGone}
                {@render line('multiRename.tags.selectionGone', 'status')}
            {/if}
            {#if notice !== null}
                {@render line(notice, 'status')}
            {/if}
            {#if writingNow}
                {@render line('multiRename.tags.write.writing', 'status')}
            {:else if shownSummary !== null}
                {@render textLine(joinSentences(shownSummary.parts.map((part) => tString(part.key, part.params))), shownSummary.role)}
            {/if}
            <!-- eslint-enable @typescript-eslint/no-confusing-void-expression -->
        </div>
    {/if}
    <TagsFunctionKeyBar enabled={enabledCommands} onCommand={(id: TagsBarCommandId) => { runCommand(id) }} />
</div>

{#if tool !== null && tags.view !== null}
    <TagToolDialogs
        open={tool.id}
        field={tool.field}
        trackCount={tags.view.tracks.length}
        selectedCount={selection.count}
        extras={tags.view.extras}
        state={tags}
        {runScoped}
        {scopeKey}
        onTool={(chosen: TagTool) => void runTool(chosen)}
        onClear={(field: TagField) => void clearField(field)}
        onClose={() => { tool = null }}
    />
{/if}
{#if viewing !== null && tags.view !== null}
    {@const shown = viewing}
    <CoverViewer
        choice={shown.choice}
        total={shown.total}
        loadImage={() => tags.coverImage(shown.choice.key)}
        editable={tags.view.editable}
        onUseForAll={(key: string) => {
            viewing = null
            void tags.useCoverForAll(key)
        }}
        onClose={() => { viewing = null }}
    />
{/if}
{#if writeMode !== null}
    <WriteDialog {tags} mode={writeMode} {closeWaits} continuing={reviewContinues} onWriting={() => { player.stop() }} onClose={closeWrite} />
{/if}

{#if dropChoice !== null}
    {@const choice = dropChoice}
    <ModalDialog
        titleId="tag-write-now-title"
        containerStyle="width: 520px; max-width: 90vw"
        growDownward
        onclose={choice.answering ? undefined : () => { skipDropped(choice) }}
    >
        {#snippet title()}{tString('multiRename.tags.dialog.write')}{/snippet}
        <WouldDropDialog
            rows={choice.reports.at(-1)?.wouldDrop ?? []}
            nameOf={(path: string) => tags.view?.tracks.find((track) => track.path === path)?.fileName ?? fileNameOf(path)}
            busy={choice.answering}
            onWriteAnyway={() => void writeAnyway(choice)}
            onSkip={() => { skipDropped(choice) }}
        />
    </ModalDialog>
{/if}

{#snippet line(key: MessageKey, role: 'status' | 'alert')}
    <!-- eslint-disable-next-line @typescript-eslint/no-confusing-void-expression -- Svelte {@render} syntax -->
    {@render textLine(tString(key), role)}
{/snippet}

{#snippet textLine(text: string, role: 'status' | 'alert')}
    <p class="notice" {role} use:tooltip={{ text, overflowOnly: true }}>{text}</p>
{/snippet}

{#snippet coverGroup()}
    {#if tags.view !== null}
        <CoverPanel
            cover={tags.view.cover}
            editable={tags.view.editable && !dialogOpen}
            revision={tags.view}
            listChoices={() => tags.coverChoices()}
            loadThumbnail={(key: string) => tags.coverThumbnail(key)}
            {listCandidates}
            onView={(shown: CoverShown) => { viewing = shown }}
            onPick={(fileName: string) => void tags.coverFromFile(fileName)}
            onEdit={(edits: TagEdit[]) => tags.edit(edits)}
            onPaste={(data: Uint8Array) => void tags.coverFromBytes(data)}
            onDrop={(path: string) => void tags.coverFromPath(path)}
        />
    {/if}
{/snippet}

{#snippet tracksFooter()}
    <Button size="mini" onclick={() => void openInPlayer()}>{tString('multiRename.tags.play.openInPlayer')}</Button>
{/snippet}

<style>
    /* Panes, status line, function-key bar: edge to edge, as in the main window. Only the panes flex. */
    .tags-content {
        display: flex;
        flex-direction: column;
        flex: 1;
        min-height: 0;
        padding-top: var(--spacing-sm);
        color: var(--color-text-primary);
    }

    .opening {
        flex: 1;
        margin: 0;
        padding: var(--spacing-sm) var(--spacing-md);
    }

    /* Album | Tracks, split like the main window's panes: a 1px divider in `--color-border` (`PaneResizer`). */
    .panes {
        display: flex;
        flex: 1;
        min-height: 0;
        border-top: 1px solid var(--color-border);
    }

    .pane-slot {
        display: flex;
        flex-direction: column;
        min-width: 0;
        min-height: 0;
    }

    .pane-slot.album {
        flex: 45 1 0;
    }

    .pane-slot.tracks {
        flex: 55 1 0;
    }

    .divider {
        width: 1px;
        flex-shrink: 0;
        background: var(--color-border);
    }

    /* A fixed line: its notices sit side by side and cut with a tooltip rather than wrap. */
    .status {
        display: flex;
        gap: var(--spacing-md);
        flex-shrink: 0;
        min-height: calc(var(--font-size-sm) * 2);
        align-items: center;
        padding: 0 var(--spacing-md);
        overflow: hidden;
        font-size: var(--font-size-sm);
    }

    .notice {
        min-width: 0;
        margin: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        color: var(--color-text-secondary);
    }
</style>
