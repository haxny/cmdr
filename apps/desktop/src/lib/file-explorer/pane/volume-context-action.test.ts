import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('#lib/tauri-commands/index.js', () => ({ ejectVolume: vi.fn(), ejectAllOnDisk: vi.fn() }))
vi.mock('#lib/intl/messages.svelte.js', () => ({ tString: (key: string) => key }))
vi.mock('#lib/ui/toast/index.js', () => ({ addToast: vi.fn() }))
vi.mock('../navigation/server-row-actions', () => ({ runServerRowAction: vi.fn() }))
vi.mock('../navigation/picked-volume-path', () => ({ pathForPickedVolume: (v: { path: string }) => v.path }))
vi.mock('../navigation/eject-error-messages', () => ({
  wordEjectRefusal: (e: unknown) => String(e),
  ejectRefusalReport: () => undefined,
}))

import { ejectAllOnDisk, ejectVolume } from '#lib/tauri-commands/index.js'
import { addToast } from '#lib/ui/toast/index.js'
import { EjectFailure } from '../navigation/eject-error'
import { runServerRowAction } from '../navigation/server-row-actions'
import { handleVolumeContextAction, type VolumeContextActionDeps } from './volume-context-action'
import type { VolumeInfo } from '../types'

const volume = { id: 'ext', path: '/Volumes/Ext' } as VolumeInfo

function makeDeps() {
  const navigate = vi.fn()
  const deps: VolumeContextActionDeps = { getVolumes: () => [volume], getFocusedPane: () => 'left', navigate }
  return { deps, navigate }
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('handleVolumeContextAction', () => {
  it('eject: calls ejectVolume with the payload volume id', async () => {
    vi.mocked(ejectVolume).mockResolvedValueOnce('diskEjected')
    const { deps } = makeDeps()
    handleVolumeContextAction({ action: 'eject', volumeId: 'ext', volumeName: 'Ext' }, deps)
    await Promise.resolve()
    expect(ejectVolume).toHaveBeenCalledWith('ext')
    expect(addToast).not.toHaveBeenCalled()
  })

  it('eject: says the drive stays when other volumes keep it attached', async () => {
    vi.mocked(ejectVolume).mockResolvedValueOnce('volumeEjectedDiskStays')
    const { deps } = makeDeps()
    handleVolumeContextAction({ action: 'eject', volumeId: 'ext', volumeName: 'Ext' }, deps)
    await vi.waitFor(() => {
      expect(addToast).toHaveBeenCalledWith('fileExplorer.pane.ejectedDriveStaysToast', { level: 'info' })
    })
  })

  it('eject-all: asks for the whole disk, and a success says nothing', async () => {
    vi.mocked(ejectAllOnDisk).mockResolvedValueOnce('diskEjected')
    const { deps } = makeDeps()
    handleVolumeContextAction({ action: 'eject-all', volumeId: 'ext', volumeName: 'Ext' }, deps)
    await Promise.resolve()
    expect(ejectAllOnDisk).toHaveBeenCalledWith('ext')
    expect(ejectVolume).not.toHaveBeenCalled()
    expect(addToast).not.toHaveBeenCalled()
  })

  it('eject-all: a refusal before anything unmounted says nothing was ejected', async () => {
    vi.mocked(ejectAllOnDisk).mockRejectedValueOnce(
      new EjectFailure({ type: 'heldBeforeEject', holders: { type: 'incomplete', named: [] } }),
    )
    const { deps } = makeDeps()
    handleVolumeContextAction({ action: 'eject-all', volumeId: 'ext', volumeName: 'Ext' }, deps)
    await vi.waitFor(() => {
      expect(addToast).toHaveBeenCalledWith('fileExplorer.pane.ejectAllHeldToast', { level: 'error' })
    })
  })

  it('eject-all: any other refusal says some may have gone', async () => {
    vi.mocked(ejectAllOnDisk).mockRejectedValueOnce(new EjectFailure({ type: 'timedOut' }))
    const { deps } = makeDeps()
    handleVolumeContextAction({ action: 'eject-all', volumeId: 'ext', volumeName: 'Ext' }, deps)
    await vi.waitFor(() => {
      expect(addToast).toHaveBeenCalledWith('fileExplorer.pane.ejectAllFailedToast', { level: 'error' })
    })
  })

  it('eject: toasts a friendly error when the eject is refused', async () => {
    vi.mocked(ejectVolume).mockRejectedValueOnce(new Error('busy'))
    const { deps } = makeDeps()
    handleVolumeContextAction({ action: 'eject', volumeId: 'ext', volumeName: 'Ext' }, deps)
    await Promise.resolve()
    await Promise.resolve()
    expect(addToast).toHaveBeenCalledWith(expect.any(String), { level: 'error' })
  })

  it('non-eject: forwards the payload to runServerRowAction with an onOpen callback', () => {
    const { deps } = makeDeps()
    handleVolumeContextAction({ action: 'disconnect', volumeId: 'ext', volumeName: 'Ext' }, deps)
    expect(runServerRowAction).toHaveBeenCalledWith(
      expect.objectContaining({ action: 'disconnect', volumeId: 'ext', volumeName: 'Ext' }),
    )
  })

  it('onOpen navigates the focused pane to the volume, through the switch arm', () => {
    const { deps, navigate } = makeDeps()
    handleVolumeContextAction({ action: 'open', volumeId: 'ext', volumeName: 'Ext' }, deps)
    const onOpen = vi.mocked(runServerRowAction).mock.calls[0]?.[0].onOpen
    onOpen?.('ext')
    expect(navigate).toHaveBeenCalledWith({
      pane: 'left',
      to: { selectVolume: { volumeId: 'ext', path: '/Volumes/Ext' } },
      source: 'user',
    })
  })

  it('onOpen does nothing for a volume id nothing matches (row gone since the menu opened)', () => {
    const { deps, navigate } = makeDeps()
    handleVolumeContextAction({ action: 'open', volumeId: 'ext', volumeName: 'Ext' }, deps)
    const onOpen = vi.mocked(runServerRowAction).mock.calls[0]?.[0].onOpen
    onOpen?.('gone')
    expect(navigate).not.toHaveBeenCalled()
  })
})
