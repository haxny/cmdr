/**
 * The saved Synchronize folders profiles (F2 in the dialog): the list in the person's order, load, save as, rename,
 * move, and delete over the backend store (`src-tauri/src/sync/profile_store.rs`). A profile holds two roots (a
 * volume id and a path, never a credential), the options, the show-filters, and its overrides.
 *
 * Never thrown at the menu: every change answers `saved`, `nameTaken` (the control asks, then calls again with
 * `replace`), or the typed refusal. A name compares trimmed and lowercased, as the store does.
 */
import { getAppLogger } from '#lib/logging/logger.js'
import {
  asProfileEditRefusal,
  asProfileSaveRefusal,
  type ProfileEditRefusal,
  type ProfileEntry,
  type ProfileMove,
  type ProfileOverrides,
  type ProfileProblem,
  type ProfileSaveRefusal,
  type RootRef,
  type ShowFilters,
  type SyncOptions,
  type SyncProfile,
} from '#lib/tauri-commands/sync.js'

const log = getAppLogger('sync')

export interface SyncProfilesIpc {
  getSyncProfiles: () => Promise<ProfileEntry[]>
  saveSyncProfile: (profile: SyncProfile) => Promise<ProfileEntry[]>
  deleteSyncProfile: (id: string) => Promise<ProfileEntry[]>
  // eslint-disable-next-line cmdr/no-confusable-callback-params -- mirrors the `#lib/tauri-commands/sync` wrapper it carries
  renameSyncProfile: (id: string, name: string) => Promise<ProfileEntry[]>
  moveSyncProfile: (id: string, direction: ProfileMove) => Promise<ProfileEntry[]>
}

/** What the dialog has that a profile keeps. */
export interface ProfileSource {
  left: RootRef
  right: RootRef
  options: SyncOptions
  showFilters: ShowFilters
  overrides: ProfileOverrides
}

/** A profile as the dialog loads it: `showFilters` is `null` unless the profile asks to restore them. */
export interface LoadedProfile {
  id: string
  name: string
  left: RootRef
  right: RootRef
  options: SyncOptions
  showFilters: ShowFilters | null
  overrides: ProfileOverrides
}

export type ProfileRefusal = ProfileSaveRefusal | ProfileEditRefusal | { type: 'unexpected' }

/** How a change went. `nameTaken` asks first: another profile has that name. */
export type ProfileAnswer =
  | { type: 'saved' }
  | { type: 'nameTaken'; name: string }
  | { type: 'refused'; refusal: ProfileRefusal }

/** One row of the menu: a usable profile, or one greyed with its problem. */
export interface ProfileView {
  id: string
  name: string
  problem: ProfileProblem | null
}

export function profileView(entry: ProfileEntry): ProfileView {
  return entry.type === 'valid'
    ? { id: entry.profile.id, name: entry.profile.name, problem: null }
    : { id: entry.id, name: entry.name, problem: entry.problem }
}

const nameKey = (name: string): string => name.trim().toLowerCase()

function refusalOf(error: unknown, what: string): ProfileRefusal {
  const typed = asProfileSaveRefusal(error) ?? asProfileEditRefusal(error)
  if (typed) return typed
  log.warn("couldn't {what} a sync profile: {error}", { what, error })
  return { type: 'unexpected' }
}

class SyncProfilesState {
  entries = $state.raw<ProfileEntry[]>([])

  constructor(
    private readonly ipc: SyncProfilesIpc,
    private readonly newId: () => string,
  ) {}

  async refresh(): Promise<void> {
    try {
      this.entries = await this.ipc.getSyncProfiles()
    } catch (error) {
      log.warn("couldn't read the sync profiles: {error}", { error })
    }
  }

  private valid(id: string): SyncProfile | null {
    for (const entry of this.entries) if (entry.type === 'valid' && entry.profile.id === id) return entry.profile
    return null
  }

  /** The profile another row would collide with on `name`, ignoring `exceptId` (a rename to its own name). */
  private taken(name: string, exceptId?: string): ProfileView | undefined {
    return this.entries.map(profileView).find((view) => view.id !== exceptId && nameKey(view.name) === nameKey(name))
  }

  /** The profile to apply, or `null` (gone, or greyed). Show-filters only when the profile asks for them. */
  load(id: string): LoadedProfile | null {
    const profile = this.valid(id)
    if (!profile) return null
    const { restoreShowFilters, showFilters, ...rest } = profile
    return { ...rest, showFilters: restoreShowFilters ? showFilters : null }
  }

  /** Saves the dialog as profile `name`; a taken name asks first (`replace` saves over it, keeping its place). */
  async saveAs(
    name: string,
    from: ProfileSource,
    choice: { replace?: boolean; restoreShowFilters?: boolean } = {},
  ): Promise<ProfileAnswer> {
    const taken = this.taken(name)
    if (taken && !choice.replace) return { type: 'nameTaken', name: taken.name }
    const profile: SyncProfile = {
      id: taken?.id ?? this.newId(),
      name: name.trim(),
      left: from.left,
      right: from.right,
      options: from.options,
      showFilters: from.showFilters,
      restoreShowFilters: choice.restoreShowFilters ?? false,
      overrides: from.overrides,
    }
    return this.change('save', () => this.ipc.saveSyncProfile(profile))
  }

  /** Renames in place; onto another profile's name it asks first, then saves over that one. */
  async rename(id: string, name: string, choice: { replace?: boolean } = {}): Promise<ProfileAnswer> {
    const taken = this.taken(name, id)
    if (taken && !choice.replace) return { type: 'nameTaken', name: taken.name }
    const profile = this.valid(id)
    if (taken && profile)
      return this.change('rename', () => this.ipc.saveSyncProfile({ ...profile, name: name.trim() }))
    return this.change('rename', () => this.ipc.renameSyncProfile(id, name.trim()))
  }

  async move(id: string, direction: ProfileMove): Promise<ProfileAnswer> {
    return this.change('move', () => this.ipc.moveSyncProfile(id, direction))
  }

  async remove(id: string): Promise<ProfileAnswer> {
    return this.change('delete', () => this.ipc.deleteSyncProfile(id))
  }

  private async change(what: string, write: () => Promise<ProfileEntry[]>): Promise<ProfileAnswer> {
    try {
      this.entries = await write()
      return { type: 'saved' }
    } catch (error) {
      return { type: 'refused', refusal: refusalOf(error, what) }
    }
  }
}

export type SyncProfiles = SyncProfilesState

export function createSyncProfiles(
  ipc: SyncProfilesIpc,
  newId: () => string = () => crypto.randomUUID(),
): SyncProfiles {
  return new SyncProfilesState(ipc, newId)
}
