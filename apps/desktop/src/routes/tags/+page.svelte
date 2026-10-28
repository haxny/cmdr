<script lang="ts">
    /**
     * The Tags window (fork, `multiRename.tagEditor`): one per Multi-Rename session the main window opened and holds
     * for it (`?session=<id>&folder=<name>`, label `tags-<id>`, `tag-editor/tags-window.ts`). This file is the shell
     * (settings, language and title, accent, text size, the menu bar, the remembered place, the red button); the body
     * is `TagsWindow.svelte`.
     */
    import { onDestroy, onMount } from 'svelte'
    import { getCurrentWindow } from '@tauri-apps/api/window'
    import type { UnlistenFn } from '@tauri-apps/api/event'
    import { initWindowSettings, initWindowLanguageSync } from '#lib/settings/window-settings.js'
    import { initializeShortcuts } from '#lib/shortcuts/index.js'
    import { initAccentColor, cleanupAccentColor } from '#lib/accent-color.js'
    import { initDisplayAccessibility, cleanupDisplayAccessibility } from '#lib/display-accessibility.js'
    import { initTextSize, cleanupTextSize } from '#lib/text-size.svelte.js'
    import { trackOwnRect } from '#lib/window-positioning.js'
    import { activateWindowMenu } from '#lib/tauri-commands/index.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import TagsWindow from '#lib/multi-rename/tag-editor/TagsWindow.svelte'
    import DialogBoundary from '#lib/ui/boundary/DialogBoundary.svelte'
    import Button from '#lib/ui/Button.svelte'
    import { closeSelfWindow } from '#lib/child-window-close.js'
    import { TAGS_WINDOW_RECT_KEY } from '#lib/multi-rename/tag-editor/tags-window.js'

    const log = getAppLogger('tagEditor')

    const params = new URLSearchParams(window.location.search)
    const sessionId = params.get('session') ?? ''
    const folder = params.get('folder') ?? ''

    let initialized = $state(false)
    /** The shell couldn't start: the page says so and offers a way out, since Escape lives in the body. */
    let failed = $state(false)
    let body = $state<{ requestClose: () => void; isEnding: () => boolean }>()
    const unlisten: UnlistenFn[] = []
    let unsubscribeLanguage: (() => void) | undefined

    onMount(async () => {
        const loadingScreen = document.getElementById('loading-screen')
        if (loadingScreen) loadingScreen.style.display = 'none'
        try {
            await Promise.all([initWindowSettings(), initializeShortcuts()])
            unsubscribeLanguage = initWindowLanguageSync({ title: () => tString('multiRename.tags.window.title', { folder }) })
        } catch (error) {
            log.error('Failed to initialize the Tags window: {error}', { error })
            failed = true
            return
        }
        // Looks only: a failure here leaves the window usable.
        for (const init of [initAccentColor, initDisplayAccessibility, initTextSize]) {
            try {
                await init()
            } catch (error) {
                log.warn('A Tags window look step failed: {error}', { error })
            }
        }
        initialized = true
        try {
            const win = getCurrentWindow()
            // The red button asks about unwritten edits first, as Escape does. Once the window is on its way out
            // (`closeSelfWindow` closes it again from Rust) the request goes through.
            unlisten.push(
                await win.onCloseRequested((event) => {
                    const shown = body
                    if (shown === undefined || shown.isEnding()) return
                    event.preventDefault()
                    shown.requestClose()
                }),
            )
            // macOS shares one menu bar: this window gets the main menu with the file items greyed.
            unlisten.push(
                await win.onFocusChanged(({ payload: focused }) => {
                    if (focused) void activateWindowMenu('other')
                }),
            )
            unlisten.push(await trackOwnRect(TAGS_WINDOW_RECT_KEY))
        } catch (error) {
            log.warn('Tags window: no close guard, menu sync, or remembered place: {error}', { error })
        }
    })

    onDestroy(() => {
        for (const stop of unlisten) stop()
        unsubscribeLanguage?.()
        cleanupAccentColor()
        cleanupDisplayAccessibility()
        cleanupTextSize()
    })
</script>

<main class="tags-window">
    <h1 class="sr-only">{tString('multiRename.tags.window.title', { folder })}</h1>
    <!-- Drag strip under the overlay traffic lights, like Settings and Keyboard shortcuts. -->
    <div class="window-drag-region" data-tauri-drag-region aria-hidden="true"></div>
    <DialogBoundary name="tagsWindow" onClose={() => void closeSelfWindow()}>
        {#if failed || (initialized && sessionId === '')}
            <div class="failed">
                <p role="alert">{tString('multiRename.tags.window.openFailed')}</p>
                <Button onclick={() => void closeSelfWindow()}>{tString('multiRename.tags.window.close')}</Button>
            </div>
        {:else if initialized}
            <TagsWindow bind:this={body} {sessionId} />
        {/if}
    </DialogBoundary>
</main>

<style>
    .tags-window {
        width: 100%;
        height: 100vh;
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        overflow: hidden;
        display: flex;
        flex-direction: column;
        position: relative;
        padding-top: var(--titlebar-height);
    }

    .failed {
        display: flex;
        flex-direction: column;
        align-items: flex-start;
        gap: var(--spacing-sm);
        padding: var(--spacing-lg);
    }

    .window-drag-region {
        position: absolute;
        top: 0;
        left: 0;
        right: 0;
        height: var(--titlebar-height);
        z-index: var(--z-sticky);
    }
</style>
