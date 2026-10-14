import { describe, it, expect } from 'vitest'
import { fnKeyToCommand } from './function-key-commands'
import { commands } from '#lib/commands/command-registry.js'

describe('fnKeyToCommand', () => {
  // Pins the 10 F-key bar buttons to their command ids. A registry rename that
  // breaks one of these would otherwise silently dispatch a dead id (the button
  // would no-op). This is the contract the F-bar's reactive chips read from.
  it('maps each button to the expected command id', () => {
    expect(fnKeyToCommand).toEqual({
      view: 'file.view',
      edit: 'file.edit',
      copy: 'file.copy',
      move: 'file.move',
      rename: 'file.rename',
      newFile: 'file.newFile',
      newFolder: 'file.newFolder',
      delete: 'file.delete',
      deletePermanently: 'file.deletePermanently',
      terminal: 'file.openTerminalHere',
    })
  })

  it('covers exactly 10 buttons', () => {
    expect(Object.keys(fnKeyToCommand)).toHaveLength(10)
  })

  it('references only real registry commands', () => {
    const registryIds = new Set(commands.map((c) => c.id))
    for (const id of Object.values(fnKeyToCommand)) {
      expect(registryIds).toContain(id)
    }
  })
})
