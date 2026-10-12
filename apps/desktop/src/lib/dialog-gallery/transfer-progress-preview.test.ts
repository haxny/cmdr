/**
 * The transfer-progress preview's guarantees:
 *
 * - each state hands the harness a real copy of the shape its label promises,
 *   from the fixture tree the backend created (never a path the frontend made
 *   up), into that tree's emptied destination;
 * - with no fixture tree, or a state the row doesn't advertise, it opens
 *   nothing.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { FixtureDirPayload } from './disk-fixture'

vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

import { closeGalleryDialog, getOpenGalleryDialog } from './gallery-state.svelte'
import { openTransferProgressPreview } from './transfer-progress-preview'

const FROM = '/data/dialog-gallery-transfer-fixtures/From'
const TO = '/data/dialog-gallery-transfer-fixtures/To'

const FIXTURES: FixtureDirPayload = {
  root: '/data/dialog-gallery-fixtures',
  destinationDir: '/data/dialog-gallery-fixtures/Backup destination',
  existingFolderName: 'Photos',
  existingFileName: 'Invoice 2026-07.pdf',
  nestedPath: '/data/dialog-gallery-fixtures/Projects',
  conflictPreview: {
    fromDir: '/data/dialog-gallery-conflict-fixtures/From',
    toDir: '/data/dialog-gallery-conflict-fixtures/To',
    folderOverFile: 'Website redesign',
    fileOverFolder: 'Quarterly report.pdf',
    fileOverFile: 'Budget 2026.xlsx',
  },
  transferPreview: {
    fromDir: FROM,
    toDir: TO,
    folders: 'Holiday photos',
    compactChains: 'Project archive',
    flat: 'Scanned receipts',
  },
}

beforeEach(() => {
  closeGalleryDialog()
})

describe('openTransferProgressPreview', () => {
  it.each([
    ['folders', 'Holiday photos'],
    ['compact-chains', 'Project archive'],
    ['flat', 'Scanned receipts'],
  ])('%s copies %s out of the paced tree into its destination', (stateId, name) => {
    expect(openTransferProgressPreview(stateId, FIXTURES)).toEqual({ kind: 'opened' })

    expect(getOpenGalleryDialog()).toMatchObject({
      dialogId: 'transfer-progress',
      stateId,
      transfer: { sourcePaths: [`${FROM}/${name}`], sourceFolderPath: FROM, destinationPath: TO },
    })
  })

  it('opens nothing without a fixture tree', () => {
    expect(openTransferProgressPreview('folders', null)).toEqual({ kind: 'no-fixtures' })
    expect(getOpenGalleryDialog()).toBeNull()
  })

  it('opens nothing for a state the row does not advertise', () => {
    expect(openTransferProgressPreview('no-such-state', FIXTURES)).toEqual({ kind: 'unknown-state' })
    expect(getOpenGalleryDialog()).toBeNull()
  })
})
