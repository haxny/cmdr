<!--
  The rail's "no AI provider is ready" notice: a yellow strip at the top of the rail body, above
  whichever gate or chat sits below it, so with Ask Cmdr off the user sees both steps at once.
  Not dismissable: it goes away by itself once the slot status reads `ready`
  (`ask-cmdr-slot-status.svelte.ts` re-reads when the user comes back from Settings).

  One sentence per setup gap, each ending on the same link to Settings > AI > Provider.
-->
<script lang="ts">
    import Icon from '#lib/ui/Icon.svelte'
    import LinkButton from '#lib/ui/LinkButton.svelte'
    import SectionCard from '#lib/ui/SectionCard.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import { openSettingsWindow } from '#lib/settings/settings-window.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import type { ProviderReadiness } from '#lib/tauri-commands/index.js'

    interface Props {
        gap: Exclude<ProviderReadiness, 'ready'>
    }

    const { gap }: Props = $props()

    const log = getAppLogger('askCmdr')

    // Literal keys per variant: a computed key would trip the unused-key check.
    const MESSAGES: Record<Props['gap'], MessageKey> = {
        off: 'askCmdr.providerNotice.off',
        cloudNeedsKey: 'askCmdr.providerNotice.cloudNeedsKey',
        cloudNeedsEndpoint: 'askCmdr.providerNotice.cloudNeedsEndpoint',
        localNotSetUp: 'askCmdr.providerNotice.localNotSetUp',
        localNotRunning: 'askCmdr.providerNotice.localNotRunning',
    }

    function openProviderSettings(): void {
        openSettingsWindow('ask-cmdr-provider-notice', ['AI', 'Provider']).catch((e: unknown) => {
            log.warn('opening the AI settings failed: {error}', { error: e })
        })
    }
</script>

<div class="provider-notice" data-gap={gap}>
    <SectionCard tone="warning">
        <div class="notice-row">
            <span class="notice-glyph"><Icon name="triangle-alert" size={16} aria-hidden="true" /></span>
            <p class="notice-text">
                {tString(MESSAGES[gap])}
                <LinkButton onclick={openProviderSettings}>{tString('askCmdr.providerNotice.openSettings')}</LinkButton>
            </p>
        </div>
    </SectionCard>
</div>

<style>
    .provider-notice {
        padding: var(--spacing-sm) var(--spacing-sm) 0;
    }

    /* The card's own padding is sized for Settings; the rail is narrow. */
    .provider-notice :global(.section-card) {
        padding: var(--spacing-sm);
    }

    .notice-row {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-xs);
    }

    .notice-glyph {
        display: flex;
        color: var(--color-warning);
    }

    .notice-text {
        margin: 0;
        font-size: var(--font-size-sm);
        line-height: var(--font-line-height-prose);
        color: var(--color-text-primary);
    }
</style>
