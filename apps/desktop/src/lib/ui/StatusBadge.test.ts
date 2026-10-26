import { describe, expect, it } from 'vitest'
import { mount, unmount } from 'svelte'
import StatusBadge from './StatusBadge.svelte'
import statusBadgeSource from './StatusBadge.svelte?raw'

function mountBadge(status: 'alpha' | 'beta') {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(StatusBadge, { target, props: { status } })
  return { target, component }
}

describe('StatusBadge', () => {
  it('renders the alpha status text (uppercased via CSS)', async () => {
    const { target, component } = mountBadge('alpha')
    const badge = target.querySelector('.feature-status-badge')
    expect(badge?.textContent).toBe('alpha')
    await unmount(component)
  })

  // Off the stylesheet: the unit environment carries no CSS. A badge sits in flex
  // rows inside dialog bodies, where a squeezed pill would otherwise wrap.
  it('never wraps', () => {
    expect(statusBadgeSource).toMatch(/\.feature-status-badge\s*\{[^}]*white-space:\s*nowrap/)
  })

  it('renders the beta status text', async () => {
    const { target, component } = mountBadge('beta')
    const badge = target.querySelector('.feature-status-badge')
    expect(badge?.textContent).toBe('beta')
    await unmount(component)
  })
})
