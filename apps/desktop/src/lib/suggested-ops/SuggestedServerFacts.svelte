<script lang="ts">
    // A server card's facts: what the click saves, row by row, inside the group's own `<dl>`.
    // Cmdr's facts about the save, so unlabelled as anyone's words, like a file group's From / To.
    // An edit shows `old → new` on the rows it changes and the rest plain. Read-only by design:
    // ❌ no field here takes input, and none could hold a password.
    import Icon from '#lib/ui/Icon.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { protocolLabel } from '#lib/file-explorer/navigation/filesystem-label.js'
    import type { ServerField, ServerProposalView } from '#lib/ipc/bindings.js'

    interface Props {
        server: ServerProposalView
    }

    const { server }: Props = $props()

    /** Each row's label. The servers sheet's own words where it has one, so a field reads the
     *  same here as where the user would edit it. A `Record`, so a new field can't go unlabelled. */
    const FIELD_KEYS: Record<ServerField, MessageKey> = {
        name: 'servers.sheet.name',
        address: 'servers.sheet.address',
        port: 'suggestedOps.server.field.port',
        account: 'suggestedOps.server.field.account',
        share: 'suggestedOps.server.field.share',
        folder: 'servers.sheet.rootFolder',
        export: 'servers.sheet.nfsExport',
        bucket: 'suggestedOps.server.field.bucket',
        startFolder: 'servers.sheet.startFolder',
        uid: 'servers.sheet.nfsUid',
        gid: 'servers.sheet.nfsGid',
    }

    const notSet = $derived(tString('suggestedOps.server.notSet'))
</script>

<dt>{tString('servers.hub.colType')}</dt>
<dd>{protocolLabel(server.protocol) ?? server.protocol.toUpperCase()}</dd>
{#each server.fields as row (row.field)}
    <dt>{tString(FIELD_KEYS[row.field])}</dt>
    <dd class="server-value">
        {#if row.changed}
            <span class="previous">{row.previous ?? notSet}</span>
            <span class="arrow" aria-hidden="true"><Icon name="arrow-right" size={12} /></span>
            <span class="sr-only">{tString('suggestedOps.server.changesTo')}</span>
        {/if}
        <span class="value" class:unset={row.value === null}>{row.value ?? notSet}</span>
        {#if row.field === 'address' && server.addressIsNew}
            <!-- A fact Cmdr read now: the AI may call it "your NAS", and Cmdr has never saved it. -->
            <span class="pill" use:tooltip={tString('suggestedOps.server.newAddressTooltip')}>
                {tString('suggestedOps.server.newAddress')}
            </span>
        {/if}
    </dd>
{/each}

<style>
    .server-value {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        column-gap: var(--spacing-xs);
        row-gap: var(--spacing-xxs);
        min-width: 0;
    }

    .value,
    .previous {
        overflow-wrap: anywhere;
    }

    /* The value the edit replaces reads quieter than the one the click saves. */
    .previous,
    .unset {
        color: var(--color-text-secondary);
    }

    .arrow {
        display: inline-flex;
        color: var(--color-text-tertiary);
    }
</style>
