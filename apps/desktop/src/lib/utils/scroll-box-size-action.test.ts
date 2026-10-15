import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useScrollBoxSize } from './scroll-box-size-action'
import { installLayoutMock } from '#lib/test-layout.js'

/**
 * The action exists for one property Svelte's `bind:clientHeight` family lacks: it
 * re-measures when a classic scrollbar appears or goes, which moves the CONTENT box
 * but leaves the border box (all those bindings observe) where it was.
 */
describe('useScrollBoxSize', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  function makeNode(): HTMLElement {
    const el = document.createElement('div')
    el.className = 'scroller'
    document.body.appendChild(el)
    return el
  }

  it('reports the scroll box before the observer has fired, so nothing renders a frame unmeasured', () => {
    installLayoutMock({ '.scroller': { clientWidth: 785, clientHeight: 400, offsetWidth: 800 } })
    const onResize = vi.fn()

    const action = useScrollBoxSize(makeNode(), { onResize })

    expect(onResize).toHaveBeenCalledWith({ clientWidth: 785, clientHeight: 400, offsetWidth: 800 })
    action.destroy?.()
  })

  it('observes the content box, the one a scrollbar toggle moves', () => {
    const observe = vi.spyOn(globalThis.ResizeObserver.prototype, 'observe')
    installLayoutMock({ '.scroller': { clientHeight: 400 } })
    const el = makeNode()

    const action = useScrollBoxSize(el, { onResize: vi.fn() })

    expect(observe).toHaveBeenCalledWith(el, { box: 'content-box' })
    action.destroy?.()
  })

  it('reports again when a horizontal scrollbar takes height from the box', () => {
    const layout = installLayoutMock({ '.scroller': { clientWidth: 800, clientHeight: 400, offsetWidth: 800 } })
    const onResize = vi.fn()
    const action = useScrollBoxSize(makeNode(), { onResize })
    onResize.mockClear()

    layout.resize('.scroller', { clientHeight: 390 })

    expect(onResize).toHaveBeenCalledWith({ clientWidth: 800, clientHeight: 390, offsetWidth: 800 })
    action.destroy?.()
  })

  it('stops reporting once destroyed', () => {
    const layout = installLayoutMock({ '.scroller': { clientHeight: 400 } })
    const onResize = vi.fn()
    const action = useScrollBoxSize(makeNode(), { onResize })
    action.destroy?.()
    onResize.mockClear()

    layout.resize('.scroller', { clientHeight: 100 })

    expect(onResize).not.toHaveBeenCalled()
  })
})
