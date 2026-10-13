import { RuleTester } from 'eslint'
import * as svelteParser from 'svelte-eslint-parser'
import rule from './key-hint-via-button-prop.js'

// Flat-config RuleTester with the Svelte parser, since the rule visits template nodes.
const ruleTester = new RuleTester({
  languageOptions: { parser: svelteParser, ecmaVersion: 'latest', sourceType: 'module' },
})

const filename = 'src/lib/whatever/Thing.svelte'

ruleTester.run('key-hint-via-button-prop', rule, {
  valid: [
    // The intended path: the key goes through the prop.
    { code: `<Button keyHint="F2" aria-keyshortcuts="F2">Background</Button>`, filename },
    // A chip beside a button, or in prose, is what ShortcutChip is for.
    { code: `<p>Press <ShortcutChip key="⌘K" /> to search.</p><Button>Go</Button>`, filename },
    { code: `<div class="row"><Button>Go</Button><ShortcutChip commandId="nav.back" /></div>`, filename },
  ],
  invalid: [
    // The shape that sat low and wrapped: a chip in a Button's label.
    {
      code: `<Button aria-keyshortcuts="F2">Background<ShortcutChip key="F2" size="sm" /></Button>`,
      filename,
      errors: [{ messageId: 'chipInButton' }],
    },
    // However deeply wrapped, and however conditional.
    {
      code: `<Button><span class="label">Run{#if hint}<ShortcutChip key="⏎" size="sm" />{/if}</span></Button>`,
      filename,
      errors: [{ messageId: 'chipInButton' }],
    },
    // A raw button holding a chip is a hand-rolled Button with a key.
    {
      code: `<button type="button">This volume <ShortcutChip key="⌥V" size="sm" /></button>`,
      filename,
      errors: [{ messageId: 'chipInButton' }],
    },
  ],
})
