<script lang="ts">
    /**
     * The summary under the grid: what goes each way (files and bytes), what leaves each side, the counts TC's
     * footer shows, and the guards in words. A count that matches a show-filter is a button that shows those rows
     * (ForkLift's habit). Changes reach screen readers politely; a guard that holds actions back, at once.
     */
    import LinkButton from '#lib/ui/LinkButton.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import { formatNumber } from '#lib/file-explorer/selection/selection-info-utils.js'
    import { formatByteSize } from '#lib/units/index.js'
    import type { GuardBlock, PlanSummary, ShowFilters, SyncState } from '#lib/tauri-commands/index.js'

    interface Props {
        state: SyncState | null
        /** Sets the show-filters a clicked count asks for. */
        onShow: (patch: Partial<ShowFilters>) => void
    }

    const { state, onShow }: Props = $props()

    type CountKey = keyof Pick<
        PlanSummary,
        | 'equal'
        | 'different'
        | 'uniqueLeft'
        | 'uniqueRight'
        | 'undecided'
        | 'skipped'
        | 'notComparable'
        | 'changedByYou'
        | 'hiddenActions'
    >

    interface Count {
        key: CountKey | 'warnings'
        message: MessageKey
        /** The show-filters this count's button turns on; none for a count no filter matches. */
        show?: Partial<ShowFilters>
    }

    const SHOW_ALL: Partial<ShowFilters> = {
        toRight: true,
        different: true,
        toLeft: true,
        duplicates: true,
        singles: true,
        unknown: true,
        warningsOnly: false,
    }

    const COUNTS: Count[] = [
        { key: 'equal', message: 'sync.summary.equal', show: { equal: true, warningsOnly: false } },
        { key: 'different', message: 'sync.summary.different', show: { different: true, warningsOnly: false } },
        { key: 'uniqueLeft', message: 'sync.summary.uniqueLeft', show: { singles: true, warningsOnly: false } },
        { key: 'uniqueRight', message: 'sync.summary.uniqueRight', show: { singles: true, warningsOnly: false } },
        { key: 'undecided', message: 'sync.summary.undecided', show: { unknown: true, warningsOnly: false } },
        { key: 'skipped', message: 'sync.summary.skipped' },
        { key: 'notComparable', message: 'sync.summary.notComparable' },
        { key: 'changedByYou', message: 'sync.summary.changedByYou' },
        { key: 'hiddenActions', message: 'sync.summary.hiddenActions', show: SHOW_ALL },
        { key: 'warnings', message: 'sync.summary.warnings', show: { warningsOnly: true } },
    ]

    function countOf(key: Count['key']): number {
        if (!state) return 0
        return key === 'warnings' ? state.guards.olderOverwritesNewer : state.summary[key]
    }

    function countText(count: Count): string {
        const n = countOf(count.key)
        return tString(count.message, { count: n, countText: formatNumber(n) })
    }

    function sideText(message: MessageKey, totals: PlanSummary['intoRight']): string {
        const files = totals.copyFiles + totals.replaceFiles
        return tString(message, {
            files,
            filesText: formatNumber(files),
            size: formatByteSize(totals.copyBytes + totals.replaceBytes),
        })
    }

    function guardText(block: GuardBlock): string {
        const side = block.side === 'left'
        switch (block.type) {
            case 'maxRemove': {
                const percent = block.targetFiles === 0 ? 100 : Math.round((block.removals / block.targetFiles) * 100)
                return tString(side ? 'sync.summary.guard.maxRemoveLeft' : 'sync.summary.guard.maxRemoveRight', {
                    percentText: `${formatNumber(percent)}%`,
                })
            }
            case 'emptyRoot':
                return tString(side ? 'sync.summary.guard.emptyRootLeft' : 'sync.summary.guard.emptyRootRight')
            case 'sentinelMissing':
                return tString(
                    side ? 'sync.summary.guard.sentinelMissingLeft' : 'sync.summary.guard.sentinelMissingRight',
                )
        }
    }

    const summary = $derived(state?.summary ?? null)
    const flows = $derived(
        summary
            ? [
                  sideText('sync.summary.toRight', summary.intoRight),
                  sideText('sync.summary.toLeft', summary.intoLeft),
                  ...(summary.intoRight.removals > 0
                      ? [
                            tString('sync.summary.removeRight', {
                                count: summary.intoRight.removals,
                                countText: formatNumber(summary.intoRight.removals),
                            }),
                        ]
                      : []),
                  ...(summary.intoLeft.removals > 0
                      ? [
                            tString('sync.summary.removeLeft', {
                                count: summary.intoLeft.removals,
                                countText: formatNumber(summary.intoLeft.removals),
                            }),
                        ]
                      : []),
              ]
            : [],
    )
    const counts = $derived(COUNTS.filter((count) => countOf(count.key) > 0))
    const guards = $derived(state ? state.guards.blocks.map(guardText) : [])
    const skew = $derived(state?.guards.skew ?? null)
</script>

<div class="summary" role="region" aria-label={tString('sync.summary.label')}>
    <p class="flows" aria-live="polite">
        {#each flows as flow, i (i)}<span>{flow}</span>{/each}
    </p>
    {#if counts.length > 0}
        <ul class="counts">
            {#each counts as count (count.key)}
                <li>
                    {#if count.show}
                        {@const show = count.show}
                        <LinkButton
                            onclick={() => {
                                onShow(show)
                            }}>{countText(count)}</LinkButton
                        >
                    {:else}
                        <span class="count">{countText(count)}</span>
                    {/if}
                </li>
            {/each}
        </ul>
    {/if}
    {#if skew}
        <p class="skew">
            {tString('sync.summary.skew', {
                seconds: Math.abs(skew.offsetSecs),
                secondsText: formatNumber(Math.abs(skew.offsetSecs)),
            })}
        </p>
    {/if}
    <div aria-live="assertive">
        {#each guards as guard, i (i)}<p class="guard">{guard}</p>{/each}
    </div>
</div>

<style>
    .summary {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        font-size: var(--font-size-sm);
    }

    .flows {
        display: flex;
        flex-wrap: wrap;
        gap: var(--spacing-md);
        margin: 0;
    }

    .counts {
        display: flex;
        flex-wrap: wrap;
        gap: var(--spacing-sm);
        margin: 0;
        padding: 0;
        list-style: none;
        color: var(--color-text-secondary);
    }

    .skew,
    .guard {
        margin: 0;
        color: var(--color-warning-text);
    }
</style>
