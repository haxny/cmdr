<script lang="ts">
    /**
     * Synchronize folders' confirm (spec 4.5): everything the run would do, grouped by kind and direction with a
     * checkbox each (TC's second dialog, Krusader's), where replaced and removed files go, the "Older replaces newer"
     * rows FIRST, `?` rows the person chose, hidden actions, and every guard in words with its second yes. Enter
     * accepts only what `enterMayConfirm` allows; anything that removes, replaces newer, or needs a second yes takes
     * the Synchronize button. Nothing runs without this dialog: `run.accept()` names the preview shown here.
     */
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import Button from '#lib/ui/Button.svelte'
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import { formatNumber } from '#lib/file-explorer/selection/selection-info-utils.js'
    import { formatDateTime } from '#lib/settings/reactive-settings.svelte.js'
    import { formatByteSize, formatDuration, seconds } from '#lib/units/index.js'
    import type { Acknowledgement, ConfirmGroup, GuardBlock } from '#lib/tauri-commands/sync.js'
    import { acknowledgementsNeeded, enterMayConfirm, groupTarget, isRemovalGroup } from './confirm-rules'
    import { syncProblemText } from './sync-errors'
    import type { SyncRun } from './sync-run.svelte'

    interface Props {
        run: SyncRun
    }

    const { run }: Props = $props()

    let notice = $state<string | null>(null)

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
        if (run.accepted.length === 0) {
            notice = tString('sync.confirm.nothingTicked')
            return
        }
        notice = null
        void run.accept()
    }

    function handleKeydown(event: KeyboardEvent): void {
        const plain = !event.metaKey && !event.altKey && !event.ctrlKey
        if (event.key !== 'Enter' || event.isComposing || !plain) return
        event.preventDefault()
        if (preview && enterMayConfirm(preview, run.accepted)) accept()
        else notice = tString('sync.confirm.enterRefused')
    }
</script>

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
        <div class="confirm">
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
                    <ul>
                        {#each preview.olderOverwritesNewer as row (row.item)}
                            <li>
                                {tString('sync.confirm.olderRow', {
                                    path: row.relPath,
                                    leftDate: dateOf(row.leftMtime),
                                    rightDate: dateOf(row.rightMtime),
                                })}
                            </li>
                        {/each}
                        {#if preview.olderOverwritesNewerTotal > preview.olderOverwritesNewer.length}
                            <li>
                                {tString('sync.confirm.olderMore', {
                                    countText: formatNumber(
                                        preview.olderOverwritesNewerTotal - preview.olderOverwritesNewer.length,
                                    ),
                                })}
                            </li>
                        {/if}
                    </ul>
                </section>
            {/if}

            {#if preview.groups.length === 0}
                <p>{tString('sync.confirm.nothing')}</p>
            {:else}
                <ul class="groups" aria-label={tString('sync.confirm.groupsLabel')}>
                    {#each preview.groups as group (group.key)}
                        {@const keep = keepLine(group)}
                        <li>
                            <Checkbox
                                checked={run.accepted.includes(group.key)}
                                ariaDescribedBy={keep ? `sync-confirm-keep-${group.key}` : undefined}
                                onCheckedChange={() => {
                                    run.toggleGroup(group.key)
                                }}>{groupLabel(group)}</Checkbox
                            >
                            {#if keep}<p class="keep" id="sync-confirm-keep-{group.key}">{keep}</p>{/if}
                        </li>
                    {/each}
                </ul>
            {/if}

            {#if preview.chosenUnknown > 0}<p>{tString('sync.confirm.chosenUnknown', count(preview.chosenUnknown))}</p>{/if}
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

            <p class="status" aria-live="polite">{notice ?? ''}</p>
        </div>
    {/if}

    {#snippet footer()}
        <Button variant="secondary" onclick={() => { run.dismissConfirm() }}>{tString('sync.confirm.cancel')}</Button>
        <Button
            variant={warns ? 'danger' : 'primary'}
            ariaDisabled={run.busy || run.notice === 'staleCompare'}
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

    .older {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        padding: var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        background: var(--color-warning-bg);
    }

    .older h3 {
        font-size: var(--font-size-md);
    }

    .older ul {
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
