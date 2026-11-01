<script lang="ts">
    /**
     * Synchronize folders' confirm (spec 4.5): everything the run would do, grouped by kind and direction with a
     * checkbox each (TC's second dialog, Krusader's), where replaced and removed files go, the "Older replaces newer"
     * rows FIRST, `?` rows the person chose, hidden actions, and every guard in words with its second yes. Enter
     * accepts only what `enterMayConfirm` allows; anything that removes, replaces newer, or needs a second yes takes
     * the Synchronize button. Nothing runs without this dialog: `run.accept()` names the preview shown here.
     */
    import { onMount, tick } from 'svelte'
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import Button from '#lib/ui/Button.svelte'
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import { formatNumber } from '#lib/file-explorer/selection/selection-info-utils.js'
    import { formatDateTime } from '#lib/settings/reactive-settings.svelte.js'
    import { formatByteSize, formatDuration, seconds } from '#lib/units/index.js'
    import type { Acknowledgement, ConfirmGroup, GuardBlock, WarnedItem } from '#lib/tauri-commands/sync.js'
    import {
        acknowledgementsNeeded,
        confirmBlock,
        enterMayConfirm,
        groupHold,
        groupTarget,
        isRemovalGroup,
        type ConfirmBlock,
    } from './confirm-rules'
    import { syncProblemText } from './sync-errors'
    import type { SyncRun } from './sync-run.svelte'

    interface Props {
        run: SyncRun
    }

    const { run }: Props = $props()

    let notice = $state<string | null>(null)
    let bodyEl = $state<HTMLDivElement>()

    const preview = $derived(run.preview)
    const acks = $derived(preview ? acknowledgementsNeeded(preview) : [])
    const warns = $derived(
        preview !== null &&
            (preview.olderOverwritesNewerTotal > 0 ||
                preview.blocks.length > 0 ||
                preview.groups.some((group) => isRemovalGroup(group.key))),
    )

    const GROUP_KEY = {
        copyRight: 'sync.confirm.group.copyRight',
        copyLeft: 'sync.confirm.group.copyLeft',
        replaceRight: 'sync.confirm.group.replaceRight',
        replaceLeft: 'sync.confirm.group.replaceLeft',
        removeRight: 'sync.confirm.group.removeRight',
        removeLeft: 'sync.confirm.group.removeLeft',
        copyRightRecreates: 'sync.confirm.group.copyRightRecreates',
        copyLeftRecreates: 'sync.confirm.group.copyLeftRecreates',
    } as const satisfies Record<ConfirmGroup['key'], MessageKey>

    const block = $derived(preview ? confirmBlock(preview, run.accepted, run.acknowledged) : null)
    /** What holds Synchronize back, said under the groups; a notice from a key press wins. */
    const status = $derived(notice ?? (block ? blockNotice(block) : ''))

    const WAIT_KEY = {
        removeAnyway: 'sync.confirm.waitRemoveAnyway',
        copyIntoEmpty: 'sync.confirm.waitCopyAnyway',
    } as const satisfies Record<Acknowledgement, MessageKey>

    function blockNotice(why: ConfirmBlock): string {
        switch (why.type) {
            case 'nothingTicked':
                return tString('sync.confirm.nothingTicked')
            case 'allHeld':
                return tString('sync.confirm.allHeld')
            case 'waiting':
                return why.waits
                    .map(({ ack, files }) => tString(WAIT_KEY[ack], { files, filesText: formatNumber(files) }))
                    .join(' ')
            default: {
                const unreachable: never = why
                return unreachable
            }
        }
    }

    /** The guard that holds a group for good (no button releases it), or `null`. */
    function hardHold(group: ConfirmGroup): GuardBlock | null {
        if (!preview) return null
        const hold = groupHold(preview, group.key, [])
        return hold?.kind === 'hard' ? hold.block : null
    }

    const ACK_KEY = {
        removeAnyway: 'sync.confirm.removeAnyway',
        copyIntoEmpty: 'sync.confirm.copyAnyway',
    } as const satisfies Record<Acknowledgement, MessageKey>

    function count(n: number): { count: number; countText: string } {
        return { count: n, countText: formatNumber(n) }
    }

    function groupLabel(group: ConfirmGroup): string {
        return tString(GROUP_KEY[group.key], {
            files: group.files,
            filesText: formatNumber(group.files),
            size: formatByteSize(group.bytes),
        })
    }

    function keepLine(group: ConfirmGroup): string | null {
        const keep = group.keep
        if (!keep) return null
        if (keep.type === 'trash') return tString('sync.confirm.keepTrash')
        const line = tString('sync.confirm.keepFolder', { path: keep.path })
        return preview?.keepIsSlow ? `${line} ${tString('sync.confirm.keepSlow')}` : line
    }

    function blockText(block: GuardBlock): string {
        const right = block.side === 'right'
        switch (block.type) {
            case 'maxRemove':
                return tString(right ? 'sync.confirm.maxRemoveRight' : 'sync.confirm.maxRemoveLeft', {
                    removalsText: formatNumber(block.removals),
                    targetText: formatNumber(block.targetFiles),
                    target: block.targetFiles,
                })
            case 'emptyRoot': {
                const text = tString(right ? 'sync.confirm.emptyRootRight' : 'sync.confirm.emptyRootLeft')
                const copiesInto = preview?.groups.some((group) => groupTarget(group.key) === block.side)
                return copiesInto ? `${text} ${tString('sync.confirm.emptyRootCopies')}` : text
            }
            case 'sentinelMissing':
                return tString(right ? 'sync.confirm.sentinelRight' : 'sync.confirm.sentinelLeft')
            default: {
                const unreachable: never = block
                return unreachable
            }
        }
    }

    const dateOf = (mtime: number | null): string =>
        mtime === null ? tString('sync.confirm.noDate') : formatDateTime(mtime)

    const skewText = $derived.by(() => {
        const skew = preview?.skew
        if (!skew) return null
        const offset = formatDuration(seconds(Math.abs(skew.offsetSecs)))
        return tString(skew.offsetSecs > 0 ? 'sync.confirm.skewAhead' : 'sync.confirm.skewBehind', { offset })
    })

    function accept(): void {
        if (block) {
            notice = blockNotice(block)
            return
        }
        notice = null
        void run.accept()
    }

    // The first group takes focus: Space unticks it, and Enter stays this dialog's (a focused Synchronize would let
    // Enter press it past `enterMayConfirm`).
    onMount(async () => {
        await tick()
        requestAnimationFrame(() => {
            bodyEl?.querySelector<HTMLInputElement>('.groups input[type="checkbox"]:not(:disabled)')?.focus()
        })
    })

    function handleKeydown(event: KeyboardEvent): void {
        const plain = !event.metaKey && !event.altKey && !event.ctrlKey
        if (event.key !== 'Enter' || event.isComposing || !plain) return
        event.preventDefault()
        if (preview && enterMayConfirm(preview, run.accepted)) accept()
        else notice = tString('sync.confirm.enterRefused')
    }
</script>

{#snippet rows(listed: WarnedItem[], total: number)}
    <ul>
        {#each listed as row (row.item)}
            <li>
                {tString('sync.confirm.row', {
                    path: row.relPath,
                    leftDate: dateOf(row.leftMtime),
                    rightDate: dateOf(row.rightMtime),
                })}
            </li>
        {/each}
        {#if total > listed.length}
            <li>{tString('sync.confirm.more', { countText: formatNumber(total - listed.length) })}</li>
        {/if}
    </ul>
{/snippet}

<ModalDialog
    dialogId="sync-confirm"
    titleId="sync-confirm-title"
    role={warns ? 'alertdialog' : 'dialog'}
    containerStyle="width: min(640px, 92vw); max-height: 86vh"
    fillBody
    onclose={() => {
        run.dismissConfirm()
    }}
    onkeydown={handleKeydown}
>
    {#snippet title()}{tString('sync.confirm.title')}{/snippet}

    {#if preview}
        <div class="confirm" bind:this={bodyEl}>
            {#if run.notice === 'planChanged'}
                <p class="notice" role="alert">{tString('sync.confirm.planChanged')}</p>
            {:else if run.notice === 'staleCompare'}
                <div class="notice" role="alert">
                    <span>{tString('sync.confirm.staleCompare')}</span>
                    <Button size="mini" onclick={() => void run.compareAgain()}>{tString('sync.dialog.compareAgain')}</Button>
                </div>
            {/if}
            {#if run.problem}<p class="notice" role="alert">{syncProblemText(run.problem)}</p>{/if}

            {#if preview.olderOverwritesNewerTotal > 0}
                <section class="older" aria-labelledby="sync-confirm-older">
                    <h3 id="sync-confirm-older">{tString('sync.confirm.olderTitle', count(preview.olderOverwritesNewerTotal))}</h3>
                    <p>{tString('sync.confirm.olderBody')}</p>
                    {@render rows(preview.olderOverwritesNewer, preview.olderOverwritesNewerTotal)}
                </section>
            {/if}

            {#if preview.chosenUnknownTotal > 0}
                <section class="unknown" aria-labelledby="sync-confirm-unknown">
                    <h3 id="sync-confirm-unknown">{tString('sync.confirm.chosenUnknown', count(preview.chosenUnknownTotal))}</h3>
                    {@render rows(preview.chosenUnknown, preview.chosenUnknownTotal)}
                </section>
            {/if}

            {#if preview.groups.length === 0}
                <p>{tString('sync.confirm.nothing')}</p>
            {:else}
                <ul class="groups" aria-label={tString('sync.confirm.groupsLabel')}>
                    {#each preview.groups as group (group.key)}
                        {@const keep = keepLine(group)}
                        {@const held = hardHold(group)}
                        {@const described = [
                            keep ? `sync-confirm-keep-${group.key}` : null,
                            held ? `sync-confirm-held-${group.key}` : null,
                        ].filter((id) => id !== null)}
                        <li>
                            <Checkbox
                                checked={run.accepted.includes(group.key)}
                                disabled={held !== null}
                                ariaDescribedBy={described.length > 0 ? described.join(' ') : undefined}
                                onCheckedChange={() => {
                                    run.toggleGroup(group.key)
                                }}>{groupLabel(group)}</Checkbox
                            >
                            {#if keep}<p class="keep" id="sync-confirm-keep-{group.key}">{keep}</p>{/if}
                            {#if held}<p class="keep" id="sync-confirm-held-{group.key}">{blockText(held)}</p>{/if}
                        </li>
                    {/each}
                </ul>
            {/if}

            {#if preview.hiddenActions > 0}<p>{tString('sync.confirm.hiddenActions', count(preview.hiddenActions))}</p>{/if}
            {#if skewText}<p>{skewText}</p>{/if}

            {#if preview.blocks.length > 0}
                <section class="blocks">
                    {#each preview.blocks as block, i (i)}
                        <p class="block">{blockText(block)}</p>
                    {/each}
                    {#each acks as ack (ack)}
                        <Checkbox
                            checked={run.acknowledged.includes(ack)}
                            onCheckedChange={() => {
                                run.toggleAcknowledgement(ack)
                            }}>{tString(ACK_KEY[ack])}</Checkbox
                        >
                    {/each}
                </section>
            {/if}

            <p class="status" aria-live="polite">{status}</p>
        </div>
    {/if}

    {#snippet footer()}
        <Button variant="secondary" onclick={() => { run.dismissConfirm() }}>{tString('sync.confirm.cancel')}</Button>
        <Button
            variant={warns ? 'danger' : 'primary'}
            ariaDisabled={run.busy || run.notice === 'staleCompare' || block !== null}
            onclick={accept}>{tString('sync.confirm.run')}</Button
        >
    {/snippet}
</ModalDialog>

<style>
    .confirm {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        overflow-y: auto;
        min-height: 0;
    }

    .confirm p,
    .confirm ul,
    .confirm h3 {
        margin: 0;
    }

    .older,
    .unknown {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        padding: var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        background: var(--color-warning-bg);
    }

    .unknown {
        background: none;
    }

    .older h3,
    .unknown h3 {
        font-size: var(--font-size-md);
    }

    .older ul,
    .unknown ul {
        padding-left: var(--spacing-lg);
        font-size: var(--font-size-sm);
    }

    .groups {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        padding: 0;
        list-style: none;
    }

    .keep {
        padding-left: var(--spacing-xl);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .blocks {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    .block,
    .notice {
        color: var(--color-warning-text);
    }

    .notice {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    .status {
        min-height: 1lh;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }
</style>
