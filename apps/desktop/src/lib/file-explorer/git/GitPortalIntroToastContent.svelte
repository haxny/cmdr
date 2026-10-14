<script lang="ts">
    /**
     * The once-ever intro to the `.git` portal (`git-portal-intro.ts` decides when).
     * "Show hidden files" flips `listing.showHiddenFiles`, the same write as the
     * View menu, the shortcut, and Settings, and only shows while that's off, so
     * it follows a change made anywhere else too.
     */
    import type { Snippet } from 'svelte'
    import { dismissToast } from '#lib/ui/toast/index.js'
    import Button from '#lib/ui/Button.svelte'
    import Trans from '#lib/intl/Trans.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { getSetting, onSpecificSettingChange, setSetting } from '#lib/settings/index.js'

    interface Props {
        toastId: string
        goToGitDir: () => void
    }

    const { toastId, goToGitDir }: Props = $props()

    let showingHidden = $state(getSetting('listing.showHiddenFiles'))
    $effect(() =>
        onSpecificSettingChange('listing.showHiddenFiles', (v) => {
            showingHidden = v
        }),
    )

    function handleDismiss(): void {
        dismissToast(toastId)
    }

    function handleShowHidden(): void {
        // Optimistic: the setting store notifies its subscribers asynchronously in the
        // app, and the button should go the moment it's pressed.
        showingHidden = true
        setSetting('listing.showHiddenFiles', true)
    }

    function handleGoToGitDir(): void {
        dismissToast(toastId)
        goToGitDir()
    }
</script>

{#snippet code(children: Snippet)}<code>{@render children()}</code>{/snippet}

<div class="content">
    <strong class="title">{tString('fileExplorer.git.portalIntro.title')}</strong>
    <p class="body">{tString('fileExplorer.git.portalIntro.what')}</p>
    <p class="body"><Trans key="fileExplorer.git.portalIntro.where" snippets={{ code }} /></p>
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={handleDismiss}
            >{tString('fileExplorer.git.portalIntro.dismiss')}</Button
        >
        {#if !showingHidden}
            <Button size="mini" variant="secondary" onclick={handleShowHidden}
                >{tString('fileExplorer.git.portalIntro.showHidden')}</Button
            >
        {/if}
        <Button size="mini" variant="primary" onclick={handleGoToGitDir}
            >{tString('fileExplorer.git.portalIntro.goToGitDir')}</Button
        >
    </div>
</div>

<style>
    .content {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    .title {
        display: block;
        font-weight: 600;
    }

    .body {
        margin: var(--spacing-xs) 0 0;
    }

    code {
        font-family: var(--font-mono);
    }

    .actions {
        display: flex;
        /* A long locale moves a whole button to the next row instead of breaking
           its label mid-word. */
        flex-wrap: wrap;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-lg);
    }
</style>
