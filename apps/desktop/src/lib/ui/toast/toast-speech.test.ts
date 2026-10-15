import { describe, it, expect, afterEach } from 'vitest'
import { toastSpeech } from './toast-speech'

function body(html: string): HTMLElement {
  const element = document.createElement('div')
  element.innerHTML = html
  document.body.appendChild(element)
  return element
}

afterEach(() => {
  document.body.innerHTML = ''
})

describe('toastSpeech', () => {
  it('says the message text', () => {
    expect(toastSpeech(body('<span>Copied 3 files.</span>'))).toBe('Copied 3 files.')
  })

  it('leaves out the buttons, which focus reaches and names on its own', () => {
    const element = body(
      '<div><span>Moved 3 files to the trash.</span><div><button>Go to trash</button><button>Undo</button></div></div>',
    )
    expect(toastSpeech(element)).toBe('Moved 3 files to the trash.')
  })

  it('keeps a link that sits inside a sentence, which is part of what the sentence says', () => {
    const element = body(
      '<div><p>All of this is configurable in <button>Settings &gt; Keyboard shortcuts</button>.</p>' +
        '<div><button>Don’t show again</button></div></div>',
    )
    expect(toastSpeech(element)).toBe('All of this is configurable in Settings > Keyboard shortcuts.')
  })

  it('leaves out what is hidden from assistive tech, like the age label', () => {
    expect(toastSpeech(body('<span aria-hidden="true">2m ago</span><span>Saved.</span>'))).toBe('Saved.')
  })

  it('joins rows with one space and trims the whitespace markup leaves behind', () => {
    const element = body('<div>\n  <p>Couldn’t reach the share.</p>\n  <p>  Check the network.  </p>\n</div>')
    expect(toastSpeech(element)).toBe('Couldn’t reach the share. Check the network.')
  })
})
