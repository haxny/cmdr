import { RuleTester } from 'eslint'
import * as svelteParser from 'svelte-eslint-parser'
import rule from './dialog-buttons-in-footer.js'

// Flat-config RuleTester with the Svelte parser, since the rule visits template nodes.
const ruleTester = new RuleTester({
  languageOptions: { parser: svelteParser, ecmaVersion: 'latest', sourceType: 'module' },
})

const filename = 'src/lib/whatever/Thing.svelte'

ruleTester.run('dialog-buttons-in-footer', rule, {
  valid: [
    // The intended path: the buttons go in the footer, a left-side control in footerLeading.
    {
      code: `<ModalDialog titleId="t">
        <p>Body</p>
        {#snippet footerLeading()}<Button>Save to disk</Button>{/snippet}
        {#snippet footer()}<Button>Cancel</Button><Button variant="primary">Send</Button>{/snippet}
      </ModalDialog>`,
      filename,
    },
    // One in-body button beside content isn't a row.
    {
      code: `<ModalDialog titleId="t"><div class="failed"><p>Didn't build.</p><Button>Try again</Button></div></ModalDialog>`,
      filename,
    },
    // Raw buttons (a copy affordance, a disclosure toggle) aren't actions.
    {
      code: `<ModalDialog titleId="t"><div><button>Copy</button><button>Details</button></div></ModalDialog>`,
      filename,
    },
    // A toolbar of mini buttons over a list is content, not the dialog's actions.
    {
      code: `<ModalDialog titleId="t"><div class="bulk"><Button size="mini">Allow all</Button><Button size="mini">Deny all</Button></div></ModalDialog>`,
      filename,
    },
    // Outside a dialog, a button row is just a button row.
    { code: `<div class="row"><Button>A</Button><Button>B</Button></div>`, filename },
  ],
  invalid: [
    // The shape that wrapped: a hand-built row in the body.
    {
      code: `<ModalDialog titleId="t"><div class="button-row"><Button>Cancel</Button><Button>OK</Button></div></ModalDialog>`,
      filename,
      errors: [{ messageId: 'buttonRowInBody' }],
    },
    // Buttons that swap by state are still one row, however deeply the row sits.
    {
      code: `<ModalDialog titleId="t"><div><div class="button-row">
        {#if busy}<Button>Stop</Button>{:else}<Button>Cancel</Button>{/if}
        <Button>Go</Button>
      </div></div></ModalDialog>`,
      filename,
      errors: [{ messageId: 'buttonRowInBody' }],
    },
    // Buttons straight in the body are a row too.
    {
      code: `<ModalDialog titleId="t"><p>Sure?</p><Button>No</Button><Button>Yes</Button></ModalDialog>`,
      filename,
      errors: [{ messageId: 'buttonRowInBody' }],
    },
  ],
})
