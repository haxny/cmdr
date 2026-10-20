<script lang="ts">
    import { tick, untrack } from 'svelte'
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import TextInput from '#lib/ui/TextInput.svelte'
    import Button from '#lib/ui/Button.svelte'
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import Spinner from '#lib/ui/Spinner.svelte'
    import Trans from '#lib/intl/Trans.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import type { UnlockContainer } from '#lib/file-explorer/pane/dialog-props.js'

    interface Props {
        /** What's locked: picks the wording, and whether the Keychain box shows. */
        container: UnlockContainer
        /** Display name: the archive file ("photos.zip") or the repo folder ("m3-restic"). */
        name: string
        /** True when a password was just rejected: re-prompt with distinct copy. */
        wrongAttempt: boolean
        /** A repo is checking the password: the field and Unlock wait, Cancel stays live. */
        checking?: boolean
        /** The last check couldn't reach the repo, so the password may well be right: say so, keep it. */
        unreachable?: boolean
        /** How many times the repo refused a password in this prompt. A bump empties the field. */
        rejections?: number
        /** Called with the entered password (and the Keychain choice) when the user confirms. */
        onSubmit: (answer: { password: string; remember: boolean }) => void
        /** Called when the user cancels (Esc, ×, or the Cancel button), checking or not. */
        onCancel: () => void
    }

    const {
        container,
        name,
        wrongAttempt,
        checking = false,
        unreachable = false,
        rejections = 0,
        onSubmit,
        onCancel,
    }: Props = $props()

    const isRepo = $derived(container === 'resticRepo')

    let password = $state('')
    /** "Remember in Keychain" starts on: restic users re-open the same repo for years. */
    let remember = $state(true)
    let passwordInputRef: HTMLInputElement | undefined = $state()

    const COPY: Record<UnlockContainer, Record<'title' | 'retryTitle' | 'message' | 'retryMessage', MessageKey>> = {
        archive: {
            title: 'fileOperations.archivePassword.title',
            retryTitle: 'fileOperations.archivePassword.retryTitle',
            message: 'fileOperations.archivePassword.message',
            retryMessage: 'fileOperations.archivePassword.retryMessage',
        },
        resticRepo: {
            title: 'fileOperations.resticPassword.title',
            retryTitle: 'fileOperations.resticPassword.retryTitle',
            message: 'fileOperations.resticPassword.message',
            retryMessage: 'fileOperations.resticPassword.retryMessage',
        },
    }

    const copy = $derived(COPY[container])
    const titleKey = $derived(wrongAttempt ? copy.retryTitle : copy.title)
    const messageKey = $derived(wrongAttempt ? copy.retryMessage : copy.message)
    const inputAriaKey: MessageKey = $derived(
        isRepo ? 'fileOperations.resticPassword.inputAria' : 'fileOperations.archivePassword.inputAria',
    )
    const canSubmit = $derived(password.length > 0 && !checking)

    // A refused password empties the field for the next try. A check that
    // couldn't reach the repo leaves it: that password may well be right.
    let seenRejections = untrack(() => rejections)
    $effect(() => {
        if (rejections === seenRejections) return
        seenRejections = rejections
        password = ''
    })

    // Focus the field on open and whenever a check ends with the prompt still up
    // (disabling the field during the check took focus away from it).
    $effect(() => {
        if (checking) return
        void tick().then(() => passwordInputRef?.focus())
    })

    function handleSubmit() {
        if (!canSubmit) return
        onSubmit({ password, remember: isRepo && remember })
    }

    function handleInputKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            event.preventDefault()
            event.stopPropagation()
            handleSubmit()
        }
    }
</script>

<ModalDialog
    titleId="unlock-title"
    ariaDescribedby="unlock-message"
    dialogId="archive-password"
    onclose={onCancel}
    containerStyle="width: 400px"
>
    {#snippet title()}{tString(titleKey)}{/snippet}

    <div class="dialog-body" aria-busy={checking}>
        <p id="unlock-message" class="subtitle">
            <Trans key={messageKey} params={{ name }} snippets={{ archive: containerName, repo: containerName }} />
        </p>

        <div class="input-group">
            <TextInput
                bind:inputElement={passwordInputRef}
                bind:value={password}
                type="password"
                ariaLabel={tString(inputAriaKey)}
                spellcheck={false}
                autocomplete="off"
                autocapitalize="off"
                autocorrect="off"
                placeholder={tString('fileOperations.archivePassword.placeholder')}
                disabled={checking}
                onkeydown={handleInputKeydown}
            />
        </div>

        {#if isRepo}
            <div class="remember">
                <Checkbox bind:checked={remember} disabled={checking}
                    >{tString('fileOperations.resticPassword.remember')}</Checkbox
                >
            </div>
        {/if}

        {#if unreachable && !checking}
            <p id="unlock-unreachable" class="note">{tString('fileOperations.resticPassword.unreachable')}</p>
        {/if}
    </div>

    {#snippet footer()}
        {#if checking}
            <span class="checking">
                <Spinner size="sm" label={tString('fileOperations.resticPassword.checking')} />
            </span>
        {/if}
        <Button variant="secondary" onclick={onCancel}>{tString('fileOperations.button.cancel')}</Button>
        <Button variant="primary" onclick={handleSubmit} disabled={!canSubmit}
            >{tString('fileOperations.archivePassword.unlock')}</Button
        >
    {/snippet}
</ModalDialog>

{#snippet containerName(children: import('svelte').Snippet)}<span class="container-name">{@render children()}</span
    >{/snippet}

<style>
    .subtitle {
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
    }

    .container-name {
        color: var(--color-text-primary);
        font-weight: 500;
        word-break: break-all;
    }

    .input-group {
        margin-bottom: var(--spacing-lg);
    }

    .remember {
        margin-bottom: var(--spacing-lg);
    }

    .note {
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .checking {
        display: inline-flex;
        align-items: center;
        margin-right: auto;
    }
</style>
