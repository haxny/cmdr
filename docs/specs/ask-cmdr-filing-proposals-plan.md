# Ask Cmdr: file with a new name, propose only where it can land, and propose a server

Implementation plan for the suggested-ops half of #749 (§ 1's `propose_server` and destination validation, § 5's "a move
with an optional per-file new name"). Loop: `docs/guides/multi-agent-refactors.md`. One executor agent per milestone,
sequential; each lands green on its own. Builds on the suggested-ops redesign (#747) and on
`ask-cmdr-volume-aware-tools-plan.md`'s location resolver (`apps/desktop/src-tauri/src/location/`).

## The headline acceptance test

A sweep from prod conversation 179's situation, replayed against fixtures:

1. The model proposes ONE group: move `~/Downloads/Invoice-X.pdf` into `invoices paid/` on the LAN share as
   `2026-09-27 OpenAI invoice Rymd.pdf`. The group lands, the dialog row reads
   `Invoice-X.pdf → 2026-09-27 OpenAI invoice Rymd.pdf`, approving it moves and renames in one operation, and undo puts
   `Invoice-X.pdf` back where it was.
2. The same proposal naming the Tailscale share's id (not connected, its same-user LAN sibling live, BOTH GUIDs recorded
   by authenticated sessions) lands on the LAN share, the report says `via` the Tailscale id, and the dialog names the
   LAN share as the destination volume. With the LAN share gone too, it refuses whole with `miss: volumeNotConnected`,
   `nearby` (each with a `candidate`), and `sameShareAs`, and stages nothing.
3. The model proposes adding an SFTP server. The dialog shows protocol, host, port, folder, and account, no password
   field, and the AI's reason labelled. Approving saves it exactly as the add form's "Add anyway" would, with no network
   contact; it shows in the servers list and the switcher, not connected. A proposed edit that changes a saved server's
   host refuses at proposal time and never reaches the user.

## Loud rules

- **We do not trust the agent** (`agent/suggested_ops/CLAUDE.md`). Every new power is disclosure plus an ordinary
  user-equivalent action. ❌ No agent-specific execution behaviour: no auto-skip on a name collision, no refusing an
  overwrite after approval. Refuse at PROPOSAL time, where it costs the model a retry and the user nothing. Where a
  refusal guards data (a chain overwrite), the ENGINE refuses it too, for every caller.
- **A group is exactly one executor call** (`agent/store/proposals/DETAILS.md`). ❌ Never split a group into several
  operations to fake a capability the engine lacks.
- **`propose_server` is `Access::Propose`.** ❌ Never `Write`, ❌ never `Session`. It stages a card; the user's click is
  the only thing that saves. No secret ever enters a proposal, in either direction, and approving an add contacts no
  server.
- **Reuse the shipped cores.** Approval calls the app's own add and edit cores; validation calls `crate::location`'s one
  resolver; names fold through `cmdr_fs::name_fold::fold_name`. Move a core DOWN when `agent/` can't reach it
  (`commands/` sits above `agent/`; `agent/` names nothing in `commands/` today, keep it so). ❌ Never copy one.
- **The spine keeps `rusqlite` as its only real dependency.** A server draft stored on the spine is the spine's own
  plain type; mapping it to `ServerTarget` and friends happens a layer up.
- **Schema text is prefix.** Every milestone touching a schema or registry line re-pins `FIXED_PROMPT_OVERHEAD_TOKENS`
  (`agent/chat/budget.rs`, 6,828 at this plan's writing, pinned by `agent/chat/context/cost_tests.rs`) and the number in
  `agent/chat/DETAILS.md`. Budget for this plan: ≤ +15 for M5 (one optional field, one clause), ≤ +25 for M6, ≤ 350 for
  M9. Report the real numbers.
- **Characterize, then convert.** M2 pins the transfer engine's named-leaf behaviour (including undo) before anything
  generalizes it.
- **English strings only**, each with a `@key` description; the translator agent does the rest.
- **Strip milestone tags** from code, comments, test names, and docs before landing
  (`rg -n '\b(M[0-9][a-z]?|Milestone\s*[0-9]|Phase\s*[0-9])\b'`).
- **Foreground commands only**, in every executor.

## What the code does today (fresh read, 2026-10-26)

Move or copy with a new name:

- `proposal_ops.destination` already exists: per-op, a bare NAME, written only by `GroupIntent::Rename`
  (`agent/store/proposals/mod.rs::GroupIntent::rows`). Move and copy ops write `NULL`. The rename review reads it as
  `op.destination.unwrap_or_default()` (`agent/tools/propose/rename/store.rs:354`), so a rename op's destination must
  never be `NULL`.
- The binding digest already hashes `(id, source_path, destination)` per op (`proposals/DETAILS.md` § The binding
  digest), so a per-op name is bound by the claim with no change. Verified.
- The wire already carries it for every verb: `commands/agent/suggested_ops.rs` maps `new_name: op.destination` (line
  437), and `agent/tools/suggestions/group.rs:154` does the same for the model. Only the field's doc comment says
  "rename only".
- The dialog already renders `old → new` for any row whose op carries `newName` (`SuggestedOpFiles.svelte:222`), and
  `SuggestedOpsDialog.a11y.test.ts:373` exercises it. Verified.
- **The executor does NOT honor a per-op name for move or copy.** `bridge/mod.rs::start_for` calls `start_volume_move` /
  `start_volume_copy` with bare sources and one destination folder and never reads `op.destination` outside rename.
  `capture_expected_sources` drops a source it can't fingerprint, and the binding then skips it.
- **The write engine takes an explicit leaf for exactly ONE source.** `VolumeCopyConfig::destination_name` /
  `WriteOperationConfig::destination_name` (`crates/cmdr-operations/src/types.rs:603, 748`), validated by
  `validation::validate_transfer_destination_name`, which refuses `sources.len() != 1`. It is honored on every route:
  native copy (`transfer/copy/mod.rs`, via `named_destination::initial_remap`), native move same-fs and cross-fs
  (`transfer/move_op/`), volume copy and move (`transfer/volume/copy.rs`, `copy_serial.rs`, `landing.rs`,
  `move_same.rs`, `move_cross.rs`, `move.rs`), archive copy-into (`archive_edit/copy_into*.rs`), and retry
  (`routing/retry.rs`, whose `RetryRecipe` captures the config at the routing entry, `routing.rs:478, 520`). About 25
  non-test sites read it. The both-local volume facades re-root sources with `src_root.join(p)` and copy
  `destination_name` into a `WriteOperationConfig` (`transfer/volume/move.rs:114-123`, `volume/copy.rs:204-214`);
  `impl From<&WriteOperationConfig> for VolumeCopyConfig` (`types.rs:789`) copies it back. Contract:
  `crates/cmdr-operations/src/transfer/DETAILS.md` § Named destinations.
- `validation::validate_source_names_are_distinct` refuses two sources sharing a top-level name, byte-exact on the
  ORIGINAL names (`validation.rs:50-61` explains the cross-fs staging hazard behind it). `validation.rs::is_same_file`
  compares by `dev+ino`.
- Two fold functions exist. `cmdr_fs::name_fold::fold_name` is NFC then lowercase on every platform, the key a
  transfer's destination buckets (`transfer/dest_name_index.rs`) and the rename engine already use.
  `cmdr_fs::name_fold::normalize_for_comparison` (re-exported as `cmdr_index::store::normalize_for_comparison`) follows
  THIS machine's local filesystem: NFD + lowercase on macOS, identity elsewhere. The rename proposal's duplicate check
  uses the second (`agent/tools/propose/rename/plan.rs:406`), so on Linux it misses a case-only duplicate.
- `cmdr_fs::validation::validate_filename` refuses an empty-after-trim name, `/`, NUL, and 255+ bytes. Nothing refuses
  control or format characters.
- Default conflict policy for an approved transfer is `ConflictResolution::Stop` (it asks), the same as a click.
- `input.rs` refuses `renames` on any verb but `rename` (`RenamesVerbMismatch`).

Destination validation:

- `propose_suggestions` deserializes `destination` and `selector.root` as `Location` with BOTH `volumeId` and `path`
  required, tilde-expands, and checks only `WritableDestination` (no archive, no `.git` route). Nothing resolves either
  against a volume: a destination on a sleeping NAS stages fine, and the review then shows
  `DestinationState::NotConnected` and approval refuses. `sourceVolumeId` with `paths` is taken on trust too.
- The resolver is pure over `VolumeFacts` (`location/resolve.rs::resolve`), and `location/miss.rs::volume_not_connected`
  builds the typed miss with `nearby` and `sameShareAs`. `VolumeFacts::readers` re-addresses a resolved location onto
  every same-share, same-account sibling with `via` (`location/same_share.rs`).
- **The same-share proof accepts a probe-filled GUID.** `ShareIdentity` holds a GUID with no record of how it was
  learned, and `redirectable` needs only GUID + share name + username. `network/server_guid.rs::GuidProof` distinguishes
  `Authenticated` (a signed-in session) from `Unauthenticated` (a credential-free NEGOTIATE probe or a guest session),
  but only to decide whether a GUID may overwrite a stored one. So whatever answers a probe of a saved address (a hotel
  network handing out the NAS's LAN IP) can make itself a redirect target for reads today, and for proposal destinations
  once M5 lands.

Servers:

- A user can add five kinds: SMB (a manual host, `network/manual_servers.rs::add_manual_server`, plus a share row via
  `smb_saved_shares::remember_named_share`), and SFTP, WebDAV, S3, and NFS (`ServerTarget`, `commands/servers/wire.rs`,
  saved by `commands/servers/saves.rs::save_target`, the add sheet's "Add anyway").
- **`add_manual_server` contacts the network even with `Reachability::Skip`**: it ends in `on_host_found`, which injects
  the host into discovery, and a new share row can start the background GUID probe (the volume-aware plan's decision 7:
  two rows sharing a share name, one lacking a GUID).
- Edits: `save_edit` for SFTP / WebDAV / S3 place / NFS, `update_saved_smb_host` for an SMB host, and
  `update_saved_s3_account` for an S3 account. **An address change MOVES the server and its stored password to the new
  address** (`server_move.rs`, `server_move_smb.rs`; `SavedServerOutcome::SecretNotMoved` exists because of it). SFTP
  and WebDAV treat the username as part of the identity (`wire.rs`: "❗ Part of the identity").
- Those cores live in `commands/servers/` (`saves.rs`, `outcomes.rs::saved_by_id`, `smb_hosts.rs`, `target_ids.rs`),
  which `agent/` must not name.
- A new SFTP / WebDAV entry is saved `pinned: true` (`saves.rs::entry_of`).
- The AI-client view already has `connect_to_server` (adds an SMB host, `Access::Write`, not in the agent view).
- Typed turn origins are in the tree: every handler gets `CallContext { origin, cancel }` (`agent/tools/view.rs`), and
  `origin.user_started()` is the test (`agent/types.rs`). `admit` gates by ACCESS tier only (`Session` on a user-started
  turn).

## Decisions

1. **The per-file name rides `renames`, now legal under `move` and `copy`.** No new schema field: a group sends `paths`
   (keep every name) or `renames` (`[{ path, newName }]`), exactly one source list as today. Under move or copy the
   group lists EVERY file in `renames`, and a file given its current name keeps it. For move and copy only, such an item
   is stored with `destination = NULL`, so the dialog draws no arrow for it; a rename op always stores its name (the
   rename review reads it unconditionally). `extract`, `compress`, `trash`, and `delete` still refuse `renames`, and a
   selector never carries names. **Why**: the shape the model already knows for "this file becomes that name", almost no
   new prefix, and the store's per-op `destination` already means exactly this.
2. **`TransferLeaves` is the ONE leaf field; the group stays one operation.** It replaces `destination_name` inside both
   `WriteOperationConfig` and `VolumeCopyConfig`: either the single-item leaf or a per-source map. The WIRE is
   unchanged: serde and specta still read and write `destinationName` (a single leaf), and the map is never on the wire
   (only the bridge builds it, in Rust). It is resolved and validated ONCE, at the routing entry where `RetryRecipe`
   captures the config, before the operation is admitted to the queue; downstream code only asks `leaf_of(source)`,
   which tolerates a pruned source set (a source dropped by the binding or skipped later simply never asks), and nothing
   downstream re-validates keys. `rebased(|p| …)` maps the map's keys in the same step the both-local facades re-root
   sources (`src_root.join(p)`), so the keys always match the paths the engine walks. **Why not N single-item
   operations**: a group is one executor call, one decorator, one settle, one queue row, one undo entry, and `completed`
   means that one call settled. Splitting breaks every one of those. **Why not "one op per group when named"**: filing
   12 invoices would be 12 approvals, which is exactly the friction § 5 exists to remove. The engine already has the
   mechanism (a top-level remap seeded before the walk); this generalizes its key from one source to many.
3. **Archive routes refuse a per-source map, typed.** Copy or move into a zip and move out of one already refuse an
   execution binding (`routing.rs::route_cannot_hold_a_binding`), and `WritableDestination` keeps proposals off them, so
   no approved group reaches one. The single-item leaf keeps working there for the dialog.
4. **Name rules, checked at proposal time AND in `TransferLeaves`.**
   - **One fold**: `cmdr_fs::name_fold::fold_name` (NFC, then lowercase, on every platform). `cmdr-operations` already
     depends on `cmdr-fs` and already folds destination names with it. ❌ Not `normalize_for_comparison`: it's identity
     off macOS, so a Linux run against a case-insensitive SMB share would miss a collision. The rename proposal's
     duplicate check (`propose/rename/plan.rs:406`) switches to `fold_name` too, so every "are these one name?" answer
     in a proposal agrees.
   - **A leaf is a plain name**: not empty after trimming, no leading or trailing whitespace, no `/`, no NUL, not `.` or
     `..`, at most 255 bytes, no control character (`char::is_control`, which is exactly category `Cc`), and none of an
     explicit list of invisible format code points: the bidi embeddings and overrides U+202A–U+202E, the isolates
     U+2066–U+2069, the marks U+200E, U+200F, and U+061C, the zero-width U+200B–U+200D and U+2060, U+FEFF, and U+00AD.
     Std plus one const list, no new crate. One function, `cmdr_fs::validation::validate_plain_leaf`, used by the
     engine, the proposal checks, and the server draft. **Why**: a name that renders as something else
     (`invoice‮fdp.exe`) is a spoof the review can't show, and the list costs a legitimate name almost nothing.
   - **The same rules guard the server draft's free-text fields**: `name`, `share`, `path`, and `export` (decisions 13
     and 14 add the server-specific ones).
   - **MTP and ADB destinations add the phone's limits, at proposal time**: a PTP string carries at most 255 UTF-16 code
     units including its terminator, so a leaf is at most 254 UTF-16 code units; and Android shared storage refuses the
     FAT-reserved `" * : < > ? \ |`. Both from the PTP string format and Android's FUSE layer, unverified on a device
     for this plan: M6 checks what `mtp-rs` and `crates/cmdr-mtp/src/virtual_device.rs` enforce, anchors the claim with
     `(verified on <device/version>, <method>, <date>)` in `crates/cmdr-mtp/DETAILS.md`, and names the volume call
     (create, rename, or upload) that refuses such a name if one does. Only such a named call enforces them below the
     proposal; otherwise they are proposal-time only.
   - **Per-source effective leaves are distinct after folding**, because two names one filesystem considers equal
     collide in the cross-fs staging folder exactly like two equal names (`validation.rs:50-61`). The single-item leaf
     and the original-name check (`validate_source_names_are_distinct` with no map) stay byte-exact, as today.
   - **No chain overwrite.** A group refuses when any effective target `destination/leaf` is any source path in the
     group, compared folded, or (in the engine, where it can stat) by `dev+ino` through `is_same_file`. Applies to move
     AND copy: a copy onto a later source overwrites it before it's read; a move onto one turns "rename A to B, B to C"
     into a lost file whenever the order runs wrong.
   - **Names never go into a source's own folder.** A group with names refuses as `useRename` whenever the destination
     is the parent of ANY of its sources, with "That's a rename for the files already in that folder: split the group,
     renaming those with the rename verb and moving the rest." The same-volume move's identity guard drops a case-only
     leaf there (`transfer/DETAILS.md` § Named destinations), so the rename verb is the one that works.
   - A collision with a file already AT the destination that isn't a source is NOT refused: it meets the ordinary
     conflict policy (`Stop`, which asks), as a clicked move would.
   - **The proposal checks run on RESOLVED paths** (decision 6's output: re-addressed and rebased), which is why name
     checks land after resolution (M5 before M6).
5. **Undo restores the original name.** The journal records landing paths and the originals separately
   (`transfer/DETAILS.md` § Named destinations: "Original-source stamps and deletion ledgers continue naming the
   originals"), so the rollback planner moves `new` back to `old` and deletes a named copy. M2 pins that for one source;
   M3 and M4 pin it for many.
6. **Every place a proposal names goes through the resolver, refusing whole on a miss.** `destination`, `selector.root`,
   and the `paths` / `renames` of a hand-listed group (under `sourceVolumeId`) each resolve once through
   `location::resolve::resolve` over one `VolumeFacts::now()` snapshot taken per call. A resolver miss (`unknownVolume`,
   `pathOutsideVolume`, `noVolumeAtPath`) or a place on a volume that isn't live refuses the WHOLE call, in the same
   shape every read tool answers:
   `{ readyForReview: false, refusal: "location", group, field, miss, location, detail, nearby?, sameShareAs?, root? }`
   (the `LocationMissResult` fields flattened beside the refusal envelope, `field` one of `destination` /
   `selector.root` / `paths`). **Why sources too**: a hand-listed source on a sleeping NAS is the same
   approve-then-refuse trap as a destination there, and checking each path against its named volume's root also catches
   a hallucinated volume/path pair. 200 pure in-memory resolves per group cost nothing.
7. **A proven same-user live sibling re-addresses instead of missing, and "proven" means proven by sign-in.** When the
   named volume isn't live, `VolumeFacts::readers` supplies the first live redirectable member; the group is STORED on
   that member (rebased paths, its volume id) and the report's group entry carries ONE `via: { volumeId }` naming the
   volume asked. A group re-addresses at most one asked volume onto one member: when its sources and its destination
   would re-address onto DIFFERENT members (or one re-addresses and the other names a different volume of the same
   share), it refuses as `mixedReaddress`, simpler and safer than a `via` per field. Redirectable needs a GUID recorded
   by an AUTHENTICATED session on BOTH members (M1); a probe-filled or guest GUID only ever yields the `sameShareAs`
   hint, for reads and proposals alike. A different-username sibling stays a hint too. A selector re-addressed this way
   resolves against the sibling's index, and `NotIndexed` there is today's refusal (the model then lists paths it read
   live). **Why stored on the member**: the stored group is what runs; storing the dead id would refuse at approval.
   **Why authenticated on both**: re-addressing a WRITE onto a box that only answered a probe of a saved address would
   send the user's files to whatever answers there.
8. **`volumeId` becomes optional inside `destination` and `selector.root`**, matching the location contract (a bare path
   resolves to a LIVE volume or to `noVolumeAtPath`). `sourceVolumeId` stays required: one volume per group is the
   spine's rule, and naming it is the check.
9. **Spine decision for servers: a zero-op `GroupIntent::SaveServer` variant, with its draft in a sidecar table the
   spine writes in the group's own transaction.** Verbs `add_server` and `edit_server` (`ProposalVerb`), a new
   `Reversibility::ServerSettings` token, and
   `proposal_server_drafts (group_id PK → proposals ON DELETE CASCADE, draft TEXT NOT NULL, editing_id TEXT, previous TEXT)`
   where `draft` / `previous` are the spine's own `ServerDraft` as JSON. Weighed against a sibling `server_proposals`
   table:
   - **For the variant**: one lifecycle. Sweeps, `pending`-only amend with `revision`, the claim's `pending → approved`,
     rejection, the recovery sweep, the thread line, the agent hearing every answer (`outcomes.rs`), the pending badge
     (`count_pending` counts groups), `suggestions-changed`, analytics, and `list_suggestions` all apply unchanged. A
     sibling table would duplicate each of those or leave a server card outside them (a badge that doesn't count it, an
     answer the agent never hears).
   - **Against it, and the answer**: file-shaped code assumes ops. The acceptance record and binding are meaningless for
     zero ops (`record_acceptance` would answer `NothingSelected`), so a server group gets its own claim
     (`claim_server_group`: one `BEGIN IMMEDIATE`, conditional `UPDATE ... WHERE status = 'pending' AND revision = ?`,
     reading the draft in the same transaction) and `record_acceptance` / `approve_and_execute` refuse a server group
     typed (`NotAFileGroup`), so neither path can claim the other's kind. The revision IS the binding here: a draft only
     changes through `repropose_group`, which bumps it, and nothing changes it after the claim.
   - **Why the spine writes this sidecar when rename's producer writes its own**: rename's evidence is display-only, so
     a group missing it can fail closed later. A server group's draft IS the operation; a pending card with no draft is
     one nothing can show or save. Written in `create_group` / `repropose_group`'s transaction from the intent, it can't
     be missing. Record the decision in `agent/store/proposals/DETAILS.md` § What a producer may add beside the ops.
   - `source_volume_id` (NOT NULL) holds the edited server's place id for an edit and `""` for an add; nothing reads it
     for a server group. Note it in the DDL notes.
10. **Approving an add contacts no server; approving an edit of a connected place uses the session the user opened.**
    SFTP / WebDAV / NFS / S3 bucket: `save_target` (add) or `save_edit` (edit), whose `save_without_connecting` paths M7
    confirms (and pins) dial nothing. SMB: a contact-free save (M7) that writes the manual-server entry and the share
    row, and defers `on_host_found`'s discovery injection and any GUID probe of the host until the user first connects
    it; the background probe never targets a host nobody has connected. A saved edit keeps its host's discovery state.
    Adds take the add form's defaults for every field the draft doesn't carry. Connecting is a separate act (the user's
    Enter, or `connect_volume` on a turn the user started, which answers `needsCredentials` for a fresh server). A
    connected place's edit applies live (`live_server_edit.rs`) over its existing session and may answer `RootNotFound`
    / `Unreachable`, as the user's own edit does. **Why not let the save contact the host**: a server the user hasn't
    connected to yet shouldn't be probed on the agent's say-so, and a probe-filled GUID is exactly what M1 stops
    trusting.
11. **A proposed edit can't change where a server is, or which account it is.** Host, port, URL, protocol, S3 endpoint
    or provider, NFS export, an SMB host's address, and the SFTP / WebDAV username (part of those protocols' identity)
    are immutable in an edit; a change refuses at proposal time as `addressChange`, telling the model to propose a new
    server or ask the user to edit it in Servers. **Why**: the app's edit MOVES the stored password to the new address,
    and `autoReconnect` would then send it there with nobody watching. One approved click would hand a stored credential
    to whatever answers at an address an injected filename chose. Showing it as a diff isn't enough: the click is cheap
    and the loss isn't recoverable. What stays editable is M9's field map; each change shows old → new. An SMB edit
    applies to the HOST, so the card says how many saved shares on that host it changes.
12. **Which kinds the agent may propose.** SMB (host, optional share, account), SFTP, WebDAV, NFS: add and edit. S3: add
    a BUCKET place under an S3 account already saved (named by its id), and edit a place's name. A new S3 account
    refuses as `protocolNotProposable` ("ask the user to add it in Servers"). **Why S3 is narrower**: a new account
    needs the provider-and-endpoint union in the schema on every turn for a rare case, and its key id can only come from
    the user, who is then better served by the add sheet.
13. **No secret, ever, and nothing that unlocks one.** The input has no password, secret, key file, `useAgent`,
    `autoReconnect`, or pin field: an add takes the add form's defaults for those, an edit keeps the stored values. A
    `password` / `secret` / `secretAccessKey` / `keyFile` key in the input refuses as `secretRefused` ("Cmdr never takes
    a password from you; the user types it when they connect") rather than the generic `malformed`, so the model learns
    the rule. The spine's `ServerDraft` has no field that could hold one, so a stored secret is unrepresentable.
14. **Anti-spoofing, at proposal time AND again at approval.** An add whose address is already saved refuses as
    `alreadySaved` with that server's id (propose an edit instead), so an add can't silently replace a saved entry's
    settings (`remember` replaces by id). A display name equal under `fold_name` to ANOTHER saved server's refuses as
    `nameTaken`, so an attacker host can't wear the NAS's name. Both re-run inside `approve_server`, because a server
    saved between proposal and click (by the user, or by another approved card) can make either true. The card shows the
    address as a Cmdr fact, and marks an address Cmdr hasn't saved before ("A new address").
15. **`propose_server` only on turns the user started.** Its handler reads `ctx.origin.user_started()` from the
    `CallContext` dispatch already hands it, and answers `refusedInBackground` on a `wake` or `outcomes` turn, with no
    write. ❌ No tool-name check in `admit`: the access gate stays tier-only, and this rule lives with the one tool it
    belongs to. **Why**: a background wake has no reason to suggest a server, and a badge saying "AI suggestion: add
    server naspi-backup" arriving unprompted is the social-engineering shape the injection threat wants.
16. **An edit's diff stays honest to approval.** The proposal stores `previous` (the saved entry's editable fields AND
    its address when proposed). Approval re-reads the saved entry; if any of those no longer match `previous`, the group
    goes back with `serverChanged` ("The server changed since the AI suggested this"), never saving over a change the
    card didn't show. A forgotten server answers the core's own `Unreachable`.
17. **A claim goes back only when nothing was written.** The saver answers a typed `SaveResult`: `Refused` (the core
    refused before writing anything, which every `SavedServerOutcome` refusal and `AddServerError` is documented to
    mean: check each one), `Saved`, or `Partial { wrote }` (an SMB add that saved the host but not its share row). Only
    `Refused` releases the claim back to `pending`. `Partial` ends `completed` with a note the card, the thread line,
    and the agent all read ("saved the server, not the share"). A saver that never answers (a panic, a hang past its
    deadline) leaves the group `approved`, which the next launch's recovery flips to `interrupted`. ❌ Nothing that may
    have written goes back to approvable: a second click would write twice.

## Milestones

### M1: Re-addressing trusts only a GUID a sign-in recorded

Early and independent of everything else here; it tightens shipped behaviour (the volume-aware plan's same-share rule).

**Scope**

- Persist how each share row's GUID was learned: `serverGuidProof` (`authenticated` / `unauthenticated`) beside
  `serverGuid` in `known-shares.json`, written wherever the GUID is (`GuidProof` already travels to that write). A row
  saved before the field reads as `unauthenticated` until an authenticated session records it again. Live direct
  sessions report their proof the same way (signed in as the share's account ⇒ authenticated; guest ⇒ not).
- `ShareIdentity` carries the proof. `same_share` keeps accepting any GUID (it's a hint). `redirectable` additionally
  needs `Authenticated` on BOTH members. Every caller of `readers`, `redirect_group`, `preferred_member`, and
  `redirect_group_ids` follows (reads, `important_folders`' re-addressing, and M5's proposal destinations later).
- Docs: `location/` docs, `agent/tools/DETAILS.md` § The location contract, `network/` docs for the field, the
  volume-aware plan's decision 8 line (it's still live) pointing here.

**Intentions.** "Same share" stays a cheap hint; "answer for it" and "write to it" need proof that the box signed the
user in on both addresses.

**Landmines**

- The GUID write site distinguishes overwrite rules by proof today; keep those rules exactly, and record the proof that
  won.
- `known-shares.json` is user data: an additive field only, defaulting safe when absent.
- The volume-aware executors may be mid-flight on `location/` (§ Coordination): land this between their milestones,
  coordinated through the lead.

**Test plan.** Table tests in `same_share_tests.rs`: both authenticated ⇒ redirect; one probe-filled ⇒ only
`sameShareAs`; guest-session GUID ⇒ only `sameShareAs`; a probe-only member is never a redirect target for `readers`,
`preferred_member`, and `list_dir`'s offline-index pick; legacy row without the field ⇒ unauthenticated; an
authenticated session upgrades a row's proof. The volume-aware plan's headline fixture updated to record authenticated
proofs and still passes.

**DONE.** Gates green, the field documented where `serverGuid` is.

### M2: Pin the engine's named leaf, then make it one field

**Scope**

- Characterization first (own commit): pin, red-first where missing, what the named single-item leaf does on each route
  today, including UNDO: the operation log's rollback of a named native move puts the original name back, of a named
  volume move (the serial driver) likewise, and of a named copy deletes exactly the named copy. Reuse
  `copy/named_copy_tests.rs`, `move_op/named_move_tests.rs`, `volume/named_move_tests.rs`, and the move journal suite;
  add only what's missing (likely the undo pins: find where rollback is tested,
  `crates/cmdr-operation-log/src/rollback/` and the host-side undo tests).
- `TransferLeaves` in `cmdr-operations` REPLACES the `destination_name` field on `WriteOperationConfig` and
  `VolumeCopyConfig` (decision 2). Serde and specta keep the wire field `destinationName` byte-identical (`bindings.ts`
  doesn't move). Resolved and validated once at the routing entry, where `RetryRecipe` captures the config (today's
  `validate_transfer_destination_name` moves there), and asked `leaf_of(source) -> &OsStr` everywhere the ~25 sites read
  `destination_name` today (`TreeLanding::under`, `LockedScan::of_renames`, `named_destination::initial_remap`,
  `landing::top_level_precheck`, the same-volume no-op guards, retry, archive copy-into). Behaviour byte-identical: only
  the single-item form exists yet.

**Intentions.** Make the next two milestones a small change per route: every site asks one value "where does this source
land", so a per-source map can't be honored on one route and forgotten on another.

**Landmines**

- `routing/retry.rs` carries the config into a retry: the leaves ride it unchanged (a test exists for the single name;
  keep it).
- Validation that runs per route today (`transfer/volume/move.rs:68`, `move_cross.rs:71`, `move_same.rs:318`,
  `copy.rs:175, 619`, `move_op/mod.rs:395`, `cross_fs.rs:67`, `archive_edit/copy_into.rs:116`) collapses to the one
  check at the entry; a route reached without the entry (a test calling an engine directly) gets a constructor that
  validates, so no path skips it.
- `validate_source_names_are_distinct` runs on original names, byte-exact; leave it alone here.
- Identity guards: the same-volume move's "already in place" check and native copy's identity check already consider the
  leaf; keep exactly that.

**Test plan.** The characterization pins green before and after; a serde round trip of both configs proves the wire;
`bindings.ts` unchanged; `cargo nextest run -p cmdr-operations`; clippy.

**DONE.** No `destination_name: Option<String>` field remains, every read goes through `TransferLeaves`, the undo pins
exist, `transfer/DETAILS.md` § Named destinations names the type and the one validation point.

### M3: Per-source leaves, native routes

**Scope**

- `TransferLeaves` gains the per-source form: a map from source path to leaf, mutually exclusive with the single-item
  name, never on the wire, validated once at the routing entry (decision 2):
  - every KEY is one of the sources as passed (an unknown key refuses typed; ❌ never silently ignored);
  - each leaf passes `cmdr_fs::validation::validate_plain_leaf` (decision 4, added here in `cmdr-fs`), plus
    `validate_path_length` on `destination/leaf`;
  - effective leaves (named or kept) distinct under `fold_name`;
  - no effective target equals any source, under `fold_name`, or by `dev+ino` (`is_same_file`) where both exist;
  - names refuse when the destination is the parent of any source.
- `rebased(|p| …)`: re-keys the map in the same step the both-local facades re-root sources
  (`transfer/volume/move.rs:114-123` and `volume/copy.rs:204-214`, each `src_root.join(p)`), and those two facades carry
  `TransferLeaves` into the `WriteOperationConfig` they build. `impl From<&WriteOperationConfig> for VolumeCopyConfig`
  (`types.rs:789`) carries it back whole.
- `leaf_of` tolerates a pruned set: a source the binding drops or the queue prunes never asks, and nothing downstream
  re-validates the keys.
- Honored on native copy and native move (same-fs and cross-fs), including through the both-local facades. Volume routes
  refuse the per-source form typed until M4 (so nothing reaches them half-done; nothing proposes yet either). Archive
  routes refuse it typed for good (decision 3); check `suggested-ops-refusal.ts` words the variant.
- Undo: the journal records each landing path, and the rollback restores every original name.

**Intentions.** "Move these 12 files into that folder, each under this name" is one ordinary operation on the local
disk: one queue row, one progress bar, one conflict policy, one undo.

**Landmines**

- Cross-fs moves stage under `.cmdr-staging-<op>/` by name (`validation.rs:50-61`): stage under the EFFECTIVE leaf.
- The folded comparison must not depend on the destination volume's case rule: `fold_name` always (a refusal on a
  case-sensitive volume costs a retry; a missed collision on a case-insensitive one costs a file).
- `ExpectedSources` binds by SOURCE path; names don't change that. The decorator keys op outcomes by source path, so it
  needs nothing (`bridge/decorator.rs`).

**Test plan.** Native copy, native move same-fs, native move cross-fs (a second tempdir filesystem if the suite has
one), and a both-local VOLUME move and copy through the facades with a map, landing named: three sources, two renamed,
one kept; conflict at a renamed leaf meets `Stop` / `Skip` / `Overwrite` at the RENAMED path; refusals for two to one
name, for names differing only by case or NFC form, for an unknown map key, for a destination that's a source's parent,
for a bidi override and a zero-width joiner in a leaf, and the chain swap (`a → b`, `b → c` with `b` a source) for move
AND copy; a case-insensitive-volume case (an APFS tempdir on macOS where available, skipped with a logged reason
elsewhere) proving `Invoice.pdf` and `invoice.pdf` refuse and the `dev+ino` check catches a target that is a source
under another spelling; a queued-then-pruned case (one source removed after admission, the rest land named, no key
error); undo restores all originals; volume and archive routes refuse typed; retry keeps the map.

**DONE.** Gates green, `transfer/DETAILS.md` § Named destinations describes both forms, the one validation point, and
the refusals; `cmdr-fs` docs name `validate_plain_leaf`.

### M4: Per-source leaves, volume routes

**Scope**

- The per-source form honored on volume copy and volume move (same-volume and cross-volume): the serial driver's landing
  probe (`landing.rs::top_level_precheck`) applies each source's leaf before conflict resolution, and the move paths'
  no-op guards use it.
- The same validation as M3 at the routing entry; where the destination is remote, `dev+ino` isn't available, so the
  folded comparison is the check, plus the volume's own identity where it has one.
- An `#[ignore]`d SMB Docker test (`apps/desktop/test/smb-servers/` fixture, run manually and in the slow lane if one
  fits): copy and move local → SMB with per-source names, then undo through the operation log, asserting the share holds
  the named files and then the originals are back.

**Intentions.** The same one-operation behaviour on a NAS, which is where filing actually lands.

**Landmines**

- The landing probe runs per top-level item with a fetched destination name index; apply the leaf where the name index
  is consulted, or a conflict is checked at the original name and lands at the new one.
- MTP and ADB destinations: the volume's own create and rename calls carry the leaf; note what they do with a name past
  the phone's limits (M6 records the evidence).

**Test plan.** Volume copy and move with an in-memory or Docker-less volume: the M3 matrix minus `dev+ino`, including
the queued-then-pruned case; undo; the ignored SMB Docker run, executed once by the executor and its result reported.

**DONE.** Gates green, the volume half of `transfer/DETAILS.md` § Named destinations written.

### M5: Every proposed place resolves, or the call refuses whole

Before the name milestone, so every name check runs on resolved paths.

**Scope**

- `propose_in_thread` takes one `VolumeFacts::now()` per call and passes it into `apply_planned_sweep` (pure over the
  facts, so tests inject them), which resolves `destination`, `selector.root`, and a hand-listed group's sources
  (decisions 6–8) before the selector resolves, before any later per-group check, and before the first write. The
  resolved, re-addressed group (paths, volume ids, the destination volume's kind) is what every later check and the
  store see: M6's name checks consume it.
- The refusal shape of decision 6, built from `LocationMissResult` (`location/miss.rs`, `volume_not_connected` for the
  not-live case), plus `field`.
- Re-addressing onto a live redirectable sibling (`VolumeFacts::readers`, with M1's proof rule): the stored destination,
  root, and source paths are the member's rebased ones; `GroupReport` gains one `via: Option<Via>`; a group whose
  sources and destination would re-address to different members refuses as `mixedReaddress` (decision 7). A rebased
  location (`rebased_from`) stores the rebased path.
- The dialog names the volume the group will actually write to: `destinationVolumeName` / `destinationVolumeId` derive
  from the stored (re-addressed) destination; pin it with a test so a later change can't show the volume asked instead.
- `volumeId` optional in `destination` and `selector.root` (decision 8): `Location` there becomes the flat
  `LocationInput`; a bare path resolves to a live owner or misses `noVolumeAtPath`.
- `WritableDestination` still applies, to the RESOLVED path.
- Schema and registry line: drop `volumeId` from `required` in the location helper; one clause, "a place on a volume
  that isn't connected refuses". Re-pin (budget ≤ +15).

**Intentions.** A proposal names only places that can run now, so approval never refuses for a reason the model could
have been told while it was still talking.

**Landmines**

- `VolumeFacts::now()` is async and in-memory only (`facts.rs`); call it once, before `spawn_blocking`, never per path.
- Hand-listed sources: resolve each path WITH `sourceVolumeId` (named resolution), not bare: a bare resolve of a path on
  a not-live volume is `noVolumeAtPath`, which would lose `sameShareAs`.
- A group's sources must all land on ONE member after re-addressing; if the named volume is live, nothing moves.
- Amendments re-resolve too (an amend is a fresh proposal of the group).
- The review's `DestinationState::NotConnected` stays: a volume can leave after the proposal.
- Coordinate with `exec-m*` agents on `location/` (§ Coordination): consume, don't reshape.

**Test plan.** Table tests over fixture facts: live destination stages; named not-live with no sibling refuses with
`volumeNotConnected`, `nearby` with `candidate`, `sameShareAs`, and stages nothing (store row count unchanged, a group
earlier in the call included); not-live with an authenticated same-user live sibling stores the sibling's rebased path,
reports `via`, and the group view names the sibling's volume; a probe-only sibling refuses with it in `sameShareAs`;
different-username sibling likewise; sources and destination re-addressing to different members refuse `mixedReaddress`;
unknown volume and path-outside-volume refuse with their tokens; bare path on a dead mount point refuses
`noVolumeAtPath`; selector root re-addressed onto a sibling without an index refuses `selector` / `notIndexed`; the
conversation-179 fixture from the volume-aware plan's headline test, reused. Cost test re-pinned.

**DONE.** Gates green, `tools/suggestions/DETAILS.md` gains § Where a proposal may point, `tools/DETAILS.md` § The
location contract lists `propose_suggestions` as a consumer.

### M6: `newName` on move and copy proposals

**Scope**

- `input.rs`: `renames` admitted under `move` and `copy` (decision 1). New `GroupProblem`s: `useRename` (its sentence
  says to split the group), `duplicateLeaf { name }`, `chainOverwrite { path }`, and decision 4's name rules (extend
  `NotABareName` to carry which rule). `RenamesVerbMismatch` keeps meaning "renames under a verb that can't take names".
  The checks run on M5's RESOLVED group and call the SAME helpers M3 added (`validate_plain_leaf`, the folded
  distinctness and chain checks in `cmdr-fs` or `cmdr-operations`), never a second copy.
- The rename proposal's duplicate check (`propose/rename/plan.rs:406`) switches from `normalize_for_comparison` to
  `fold_name` (decision 4), with a test for a case-only duplicate that now refuses on every platform.
- Phone limits: read what `mtp-rs` and `crates/cmdr-mtp/src/virtual_device.rs` enforce on object names, test one against
  a real device if one's at hand, record the evidence anchor in `crates/cmdr-mtp/DETAILS.md`, name the volume call that
  refuses an over-limit name if one does, and apply decision 4's phone rules when the resolved destination's kind is MTP
  or ADB.
- Planned shapes: move and copy take a `TransferOp { source_path, new_name: Option<String>, snapshot }`; for move and
  copy only, a name equal to the source's own normalizes to `None`. `GroupIntent::Move` / `Copy` carry
  `Vec<TransferOp>`, so trash, delete, extract, and compress still can't carry a name, and rename keeps `NewRename` with
  its name always stored. A selector's ops enter as `new_name: None`.
- Store: `GroupIntent::rows` writes the name into `proposal_ops.destination`; update the schema comment in
  `migrations.rs` only where the file documents the current shape (❌ never edit a shipped migration step's SQL) and
  `proposals/DETAILS.md`.
- `snapshot.rs` freezes the renamed list the same way.
- Bridge: `start_for` builds the per-source map from `op.destination` for move and copy, keyed by EXACTLY the sources it
  passes: a source `capture_expected_sources` drops is dropped from the map too.
- Read shapes: `SuggestedOpView.new_name` and `group.rs`'s doc comments say "rename, or a move or copy that renames";
  `list.rs` unchanged.
- Schema: `renames`' description becomes "rename, move, copy: each source and the bare name it becomes. Under move or
  copy, list every file; give a file its current name to keep it."; the registry line for `propose_suggestions` gains
  "move and copy can rename on the way in". Re-pin (budget ≤ +25).
- Dialog: no code change expected (verified); a component test that a MOVE row with `newName` draws the arrow and reads
  "New name" to a screen reader, a gallery fixture row, and one Playwright E2E: a store-seeded move group with a renamed
  op shows `old → new`, approve, the file lands under the new name.

**Intentions.** "Match the destination's naming" is one group the user reads and approves once.

**Landmines**

- `PlannedSources` / `SourceShape::into_intent` take `Vec<NewOp>` today; keep the selector path untouched and let the
  named list enter through its own arm, so a selector can never mint a name.
- ❌ Don't normalize a rename op's name to `NULL`: the rename review reads `op.destination.unwrap_or_default()`
  (`propose/rename/store.rs:354`) and would rename the file to an empty name.
- Invariant 11: the renamed rows are still fingerprinted by source at preflight; nothing about names bypasses it.
- The E2E harness seeds the store; read `test/e2e-playwright/CLAUDE.md` first.

**Test plan.** Input table tests (each refusal, each name rule incl. a bidi override and a zero-width joiner, the trim
check, the phone rules on an MTP destination, normalization of a same-name item for move and copy and NOT for rename,
chain overwrite for move and copy, case- and NFC-only duplicates, `useRename` when any one source's parent is the
destination); write-path test that ops store the names and the digest changes when a name changes (an amend that only
renames refuses an old approval); bridge test with a fake engine asserting the map it receives, including a dropped
unfingerprinted source; the rename duplicate-check test; dialog component test and the E2E; cost test re-pinned.

**DONE.** Gates green, `tools/suggestions/DETAILS.md`, `tools/propose/DETAILS.md` (the fold), and
`suggested_ops/DETAILS.md` § The approval bridge updated.

### M7: Saved-server cores below `commands/`, and a contact-free SMB save

**Scope**

- Commit one, a pure move: the non-IPC halves of `commands/servers/` that M9 and M10 need go into a new
  `crate::saved_servers` module (beside `server_move`): the saved listing builder, `saved_by_id` / `SavedEntry`,
  `target_ids::place_id`, `save_target` / `save_edit` and `entry_of`, `smb_hosts::{smb_host_group(s), edit_host}`, and
  the `nfs_places` / `s3_accounts` helpers they call. `commands/servers.rs` stays the facade and keeps every command
  signature. Tests move with their code.
- Commit two, the behaviour change decision 10 needs: an SMB save that writes the manual-server entry and the share row
  with no network contact, deferring `on_host_found`'s discovery injection until the user first connects the host; and
  the background GUID probe skips a host nobody has connected (no `lastConnectedAt`). Confirm and pin that SFTP, WebDAV,
  NFS, and S3 `save_without_connecting` contact nothing.

**Intentions.** `agent/` reads saved servers and saves one through the same functions the hub uses, with no
`agent → commands` edge, and saving a server is a store write.

**Landmines**

- `ServerTarget` and the outcome enums are specta wire types re-exported by `commands::servers`; moving them must keep
  `bindings.ts` identical (`pnpm bindings:regen` and diff: no change).
- The servers hub must still list a host saved without discovery injection (it reads the manual store, check `smb_hosts`
  grouping and the frontend's servers list); if a row depends on discovery state, make that injection contact-free
  rather than calling `on_host_found`.
- The user's own "Add anyway" keeps today's behaviour unless the lead and David decide to share the contact-free path;
  raise it, don't change it silently.
- `file-length` and `claude-md-length` may fire on the new module's docs; split sensibly.
- Another session may touch `commands/servers/`; check `git log` for it before starting.

**Test plan.** Existing server tests green unchanged; `bindings.ts` byte-identical; a contact-free SMB save writes both
rows, registers no discovered host, and schedules no probe (a probe seam or counter); the probe skips a never-connected
host; the other protocols' saves make no network call (their wiring's dial seam asserted unused).

**DONE.** A `saved_servers/CLAUDE.md` + `DETAILS.md` pair, `commands/servers.rs`'s module doc points there, `network/`
docs say when the probe and discovery injection run, `docs/architecture.md` updated.

### M8: The spine holds a server proposal

**Scope**

- `ServerDraft` (spine-owned, plain fields: protocol token, display name, host, port, username, share, remote root, url,
  bucket, account id, export, uid, gid, start folder, SMB shares affected; no secret-shaped field) and
  `GroupIntent::SaveServer { draft, editing: Option<String>, previous: Option<ServerDraft> }`; `verb()` → `AddServer` /
  `EditServer`; `reversibility()` → `ServerSettings`; `op_count()` 0; `rows()` empty.
- Migration (next number): `proposal_server_drafts` (decision 9), written and replaced inside `create_group` /
  `repropose_group`'s transaction. Reading a server group without its draft (or with one that won't decode) is a typed
  store error: fail closed.
- `claim_server_group(conn, group_id, shown_revision) -> ServerClaimOutcome { Claimed { draft, editing, previous } | RevisionChanged | NotPending { found } | Unknown }`,
  one `BEGIN IMMEDIATE`. `release_claim` gives one back (zero ops, so its "no outcome recorded" guard holds trivially;
  decision 17 decides WHEN). `mark_group_completed` as for files, plus a nullable completion note (on the sidecar) for
  decision 17's `Partial`.
- `record_acceptance` and `bridge::approve_and_execute` refuse a server group as `NotAFileGroup`; the dialog's
  `ApprovalResultView` maps it to `couldNotStart` with a warn log (it can't happen from the UI).
- `outcomes.rs`: a server group's decision line and memory-ring entry ("saved the SFTP server build-box", "saved the
  server naspolya, not its share", "the user rejected adding …"), from the draft and the note, no counts.
- `list_suggestions` / `get_suggestion_group` shapers: a server group's summary carries its draft (protocol, name,
  address, account; never a secret: there isn't one) and `opCount: 0`.
- `ProposalVerb` / `Reversibility` tokens: check every exhaustive `match` (frontend `approveKey`, `reversibilityKey`,
  analytics), compile-driven.

**Intentions.** A server card is a group like any other everywhere the lifecycle reaches, and file-shaped machinery
can't mistake it for a file group.

**Landmines**

- The spine's dependency rule: `ServerDraft` uses `serde` + std only; nothing from `network/`.
- `count_pending`, the badge, and `suggestions-changed` must include server groups (they should, by being groups: test
  it).
- `recover_interrupted_groups` flips an `approved` server group to `interrupted`, honest for a save the app died during.
- Analytics: the verb token and a count bucket of 0, ❌ never a host or name.

**Test plan.** Store tests: create, amend (revision bump, draft replaced in the same transaction, a crash-mid-amend
can't leave the old draft with the new revision), claim races (two claims, one winner), revision mismatch, claim of a
file group by the server claim and vice versa refuse typed, cascade delete, decode failure fails closed, completion with
a note; outcome line text; read-shaper tests.

**DONE.** Gates green; `proposals/CLAUDE.md` + `DETAILS.md` record decision 9 (the variant, the sidecar written in the
group's transaction and why it differs from rename's, the claim, `source_volume_id` for a server group).

### M9: `propose_server`

**Scope**

- The tool (`agent/tools/suggestions/server.rs` or a sibling module), `Access::Propose`, `consumers: [Agent]`. Input,
  flat:
  `{ sweepId?, groupId?, editing?, rationale?, protocol, name?, host?, port?, username?, share?, path?, url?, export?, bucket?, account?, startFolder?, uid?, gid? }`.
- **The per-protocol field map** (anything outside a row refuses as `unboundField`; "identity" fields are required on an
  add and refuse as `addressChange` on an edit when they differ; every default is the ADD FORM's):
  - `smb`. Add: `host` (required), `port` (default 445), `username`, `share` (a share row under the host), `name` (the
    HOST's display name). Edit: `editing` is any of the host's share volume ids (what `list_volumes` shows), and the
    edit applies to that share's HOST: `name` renames the host as the servers list shows it (❌ never a share), and
    `username` sets the host's account. Identity: `host`, `port`. The draft records the host's saved share count for the
    card.
  - `sftp`. Add: `host` (required), `port` (default 22), `username` (required), `path` (the remote root), `startFolder`,
    `name`. Edit: `name`, `path`, `startFolder`. Identity: `host`, `port`, `username`.
  - `webdav`. Add: `url` (required, `http` or `https`), `username` (required), `path` (the collection under the URL),
    `startFolder`, `name`. Edit: `name`, `path`, `startFolder`. Identity: `url`, `username`.
  - `nfs`. Add: `host` (required), `port` (absent asks the portmapper), `export` (required), `uid`, `gid`,
    `startFolder`, `name`. Edit: `name`, `uid`, `gid`, `startFolder`. Identity: `host`, `port`, `export`.
  - `s3`. Add: `account` (a saved S3 account's id, required), `bucket` (required), `name`. Edit (a bucket place):
    `name`. Identity: `account`, `bucket`. No other S3 shape (decision 12).
- Validation, all before any write: the map above, decision 4's name rules on `name`, `share`, `path`, and `export`,
  `addressChange` (11), `alreadySaved` and `nameTaken` (14), `secretRefused` (13), `unknownServer` for an `editing` id
  nothing saved has, `protocolNotProposable` (12), length caps on every string.
- `previous` captured from the saved entry at proposal, address included (decision 16). `alreadySaved` uses `place_id`
  on the draft's `ServerTarget`, the same id funnel the listing uses.
- The origin rule (decision 15) in the handler: `ctx.origin.user_started()` from its `CallContext`, else
  `refusedInBackground` with no write. ❌ No change to `admit`.
- Registration: registry entry with a terse description, `EXPECTED_PROPOSE_TOOL_NAMES`, `ToolId::ProposeServer` +
  `KNOWN` + wire name, `EXPECTED_AGENT_TOOL_NAMES`, rail label + `askCmdr.tool.proposeServer.doing` / `.done` (English,
  `@key` descriptions). Re-pin overhead (budget ≤ 350; the field map lives in `DETAILS.md`, not the schema).
- Docs: `agent/DETAILS.md` § The agent can propose ("It holds three names"), invariant 7's line unchanged in substance
  but naming that a proposal may now describe a server, `tools/CLAUDE.md` module map, `tools/suggestions/CLAUDE.md` and
  `DETAILS.md` (the field map), `docs/security.md` (how the agent's reach is described: a server proposal, approved by
  the user, never a secret, no contact until the user connects), `docs/architecture.md`.

**Intentions.** "Add the NAS's backup share" is something the agent can put in front of the user, and nothing an
injected filename writes can turn into a saved host the user didn't look at, a stored password moved, a probe of an
address the user never chose, or a name that impersonates a saved server.

**Landmines**

- Validation reads saved stores through `saved_servers` (M7), in-memory, ❌ never the network.
- `alreadySaved` must compare by the store's own id funnel, never by the typed address (`commands/servers.rs` §
  `saved_server_id` explains the trailing-slash miss).
- Read the add form's defaults (`apps/desktop/src/lib/servers/server-form.ts`, `nfs-form.ts`, `s3-form.ts`); don't
  invent them.
- The rail label file is also being reworked by the tool-transparency session (§ Coordination).

**Test plan.** One test per refusal token; each row of the field map (an unbound field per protocol refuses; each
identity field change refuses on an edit); each name rule on `name`, `share`, `path`, and `export`; a valid add per
protocol stages a `SaveServer` group with the right draft; an SMB edit's `name` lands on the host and records the share
count; an edit stores `previous` with the address; amend via `groupId` bumps revision; `secretRefused` for each
secret-shaped key; `refusedInBackground` on `wake` and `outcomes` with no row written; allowlist tests
(`EXPECTED_PROPOSE_TOOL_NAMES`, agent view never writes); cost test re-pinned; label test.

**DONE.** Gates green, every doc above updated, the lead reviews the handler and the allowlist line before landing (a
`Propose` widening is a human sign-off: `agent/DETAILS.md` § What a `Propose` tool may do).

### M10: Approving a server proposal

**Scope**

- `agent/suggested_ops/server.rs`:
  `approve_server(conn, group_id, revision, saver: &dyn ServerSaver) -> ServerApprovalOutcome`, in order: claim (M8);
  re-run `alreadySaved` and `nameTaken` (decision 14) against the saved stores now; for an edit, re-read the saved entry
  and refuse `serverChanged` on drift in its editable fields or address (16); call the saver; then by `SaveResult`
  (decision 17): `Saved` marks `completed`, `Partial` marks `completed` with its note, `Refused` and every pre-save
  refusal release the claim. Then record the outcome and the thread line, emit `suggestions-changed`, bump the approval
  metric on a save. `ServerSaver` is a trait the command layer implements over `saved_servers` (M7), so the service is
  testable without real stores.
- Command `suggested_ops_approve_server(group_id, revision)` in `commands/agent/suggested_ops.rs`, registered in the
  `ipc.rs` manifest, answering
  `ServerApprovalResultView { saved { volumeId?, note? } | listChanged | alreadyAnswered | unknown | refused { outcome: SavedServerOutcome } | addServer { error: AddServerError } | serverChanged | alreadySaved { serverId } | nameTaken }`.
  Emits `volumes-changed` on a save, exactly as `update_saved_server` does.
- `SuggestedGroupView` gains `server: Option<ServerProposalView>`:
  `{ protocol, fields: [{ field, value, previous? }], addressIsNew, sharesAffected? }`, `field` a typed enum (`name`,
  `address`, `port`, `share`, `folder`, `export`, `bucket`, `account`, `startFolder`, `uid`, `gid`), present exactly for
  the two server verbs; `sharesAffected` for an SMB edit, read LIVE from the host group at view time (the card says what
  the click changes now).

**Intentions.** The click saves exactly what the card showed, through the same code the hub runs, or gives the card back
with the reason, and never offers a second click on something that may already have written.

**Landmines**

- Never hold the `main.db` write transaction across the save (the claim commits, then the save runs, then completion
  writes).
- SMB add: the contact-free save (M7), ❌ never `add_manual_server`'s discovery path, ❌ never `Reachability::Check`.
- Classify each `SavedServerOutcome` and `AddServerError` variant as before-any-write or not by reading the core, and
  pin the classification with a test per variant: a variant wrongly called pre-write is a double write. Map
  exhaustively, no `_ =>`, so a new variant forces the call.
- The drift compare covers editable fields and the address only, so a reconnect's bookkeeping (`last_connected_at`, last
  mount path) never reads as a change.

**Test plan.** Service tests with a fake saver: saved path writes completed + outcome + event; `Refused` gives back and
leaves `pending`; `Partial` completes with the note and is NOT approvable again; a hanging saver leaves `approved` and
recovery flips it to `interrupted`; revision race answers `listChanged`; drift (field or address) answers
`serverChanged` without calling the saver; a server saved between proposal and click makes approval refuse
`alreadySaved` / `nameTaken` without calling the saver. One real round trip per protocol against temp stores through
`saved_servers` (add then read back through the listing, no network contact). Command mapping test.

**DONE.** Gates green, `suggested_ops/DETAILS.md` gains § Approving a server.

### M11: The server card in the dialog

**Scope**

- `SuggestedOpsGroup.svelte` renders a server group as header → facts → AI's reason → actions, no file list, no
  selection, no live facts, no window: the header names the verb ("Add a server" / "Change a server") and the server's
  name; the facts are the `fields` as a label/value list (an edit shows `old → new` on changed rows, unchanged rows
  plain); `addressIsNew` adds "A new address" beside the address; an SMB edit says how many saved shares on that host it
  changes; the reversibility pill reads from `ServerSettings` ("You can forget it later in Servers" / "Change it back in
  Servers"); the button is "Add server" / "Save changes", calling `suggestedOpsApproveServer`. A `saved` result with a
  note shows the note. ❌ No password field, no input of any kind.
- `suggested-ops-trigger.svelte.ts`: a server group gets no `GroupReview` window (no `OP_WINDOW_SIZE` page, no
  `ensureLiveFacts`, no selection), so nothing pages a zero-op group. Approve's refusal goes through the existing
  `refusals` slot with copy from the server sheet's refusal sentences where one exists (`connect-refusals.ts`,
  `add-server-error.ts`); `serverChanged`, `alreadySaved`, and `nameTaken` get new sentences.
- Copy: English keys in the suggested-ops catalog with `@key` descriptions; field labels reuse the servers forms' keys
  if they exist and read right in this context, else new ones.
- Gallery: a store-seeded add card and edit card in Debug > Soft dialogs. a11y test (axe) for both.
- One Playwright E2E: a store-seeded add-server group, approve, and the server appears in the servers list, not
  connected.

**Intentions.** The user sees exactly what will be saved, laid out the way every other suggestion is, with the AI's
reason labelled as the AI's.

**Landmines**

- The layout is the argument (`lib/suggested-ops/CLAUDE.md`): the fields are Cmdr's facts about what the click saves, so
  they're unlabelled; only the rationale goes through `AgentReason`.
- Approve stays enabled while nothing is loading (there's nothing to load).
- A change notice keys on `reason`, not on the group id, for server cards too.
- `cmdr/no-raw-user-facing-string`, `prefer-ui-primitive`, and the a11y contrast rules apply.
- The E2E must not dial: the seeded server's address points nowhere reachable, and the test asserts it stays
  unconnected.

**Test plan.** Component tests: add card fields, edit card diff rows, the shares line on an SMB edit, no input elements
rendered, approve calls the server command with the seen revision, each refusal renders its sentence, a note renders,
`serverChanged` re-reads the group; a11y tests; the E2E; `pnpm check svelte-tests-related`. David reviews the layout and
copy (screenshots from the gallery).

**DONE.** Gates green, `lib/suggested-ops/CLAUDE.md` module map and `DETAILS.md` § "A server card" written, gallery
fixtures shipped.

### M12: Close-out

**Scope.** Adversarial conformance review against the invariants footer and the agent invariants register (7 and 11
especially); a docs-audit agent over every commit body and touched `CLAUDE.md` / `DETAILS.md`; `desktop-e2e-linux` if
the dialog or transfer surfaces moved; `pnpm check` then `pnpm check --only-slow`, both backgrounded; strip milestone
tags; the QA script.

**QA script for David** (prod-like data, LAN share connected):

1. Ask Cmdr to file an invoice from Downloads into `invoices paid` using that folder's naming. The dialog shows one move
   with `old → new`; approve; the file lands renamed; Undo in the queue puts it back under its old name.
2. Disconnect the NAS entirely and ask again: the rail says the NAS isn't connected and offers to connect it; nothing
   waits in the suggestions badge.
3. Ask it to add your SFTP build box: the card shows host, port, folder, and account, no password; approve; the server
   shows in Servers, not connected; Enter asks for the password in the normal sheet.
4. Ask it to point the NAS at a different address: it says it can't change where a saved server is and suggests editing
   it in Servers; no card appears.

## Invariants footer

1. A group is one executor call; a named move or copy is one operation with one undo, and undo restores every original
   name.
2. No proposal names a place the resolver didn't place on a live volume, or on a live same-user sibling whose GUID a
   sign-in recorded on both members (reported with one `via`, and named in the dialog as the volume written to); a group
   never re-addresses onto two members; a miss refuses the whole call in the read tools' miss shape, and stages nothing.
   A probe-filled GUID never redirects a read or a write.
3. Every per-op name is bound by the claim's digest and every renamed source by its live fingerprint (invariant 11); the
   engine's leaf map is validated once, at the routing entry, against exactly the sources it was given.
4. No group overwrites one of its own sources, by spelling, by `fold_name`, or by `dev+ino`; effective leaves are
   distinct under `fold_name`; names never go into a source's own folder; each name passes `validate_plain_leaf`. The
   proposal and the engine both refuse those. The phone limits (MTP, ADB) are refused at proposal time only, unless M6
   names a volume call that refuses them too. A collision at the destination with a non-source meets the ordinary
   conflict policy. No agent-specific execution behaviour anywhere.
5. `propose_server` is `Access::Propose`, hand-allowlisted, agent-only, refused in its own handler on turns the user
   didn't start. Approval is the user's click through the hub's own save cores, and an add contacts no server until the
   user connects it.
6. No secret enters, is stored in, or is shown by a server proposal, and no field that unlocks one (key file, agent,
   auto-reconnect) is proposable.
7. A proposed edit never changes where a saved server is or which account an SFTP / WebDAV place is; an add never
   replaces a saved one or borrows its name, checked at proposal and again at approval.
8. A server group's card shows exactly what the click saves (for an SMB edit, how many shares it touches), and an edit's
   diff, address included, is re-checked against the saved entry at approval.
9. A claim returns to `pending` only when nothing was written; anything that may have written ends `completed` (with a
   note) or `interrupted`.
10. The spine stays `rusqlite`-only and one lifecycle covers file and server groups; a file claim can't take a server
    group and vice versa.
11. No new content egress (no consent bump); every schema change re-pins the fixed prompt overhead.

## Coordination

- **Volume-aware tools executors (`exec-m*`, this branch)**: M1 changes their shipped same-share rule; it lands through
  the lead, between their milestones, and their headline fixture records authenticated proofs. M5 consumes `location/`
  as it stands after their M8 (`readers`, `volume_not_connected`); reshape nothing else there. M9's origin rule reads
  the `CallContext` their M9a put in the tree and touches nothing in `admit`. Their M9b (`connect_volume`) is the
  follow-on to a saved server and needs nothing from here; its same-share dial choice follows M1's proof rule.
- **Another agent is editing code in this worktree while this plan is written**: each executor reads `git log` for
  `commands/servers/`, `network/`, `agent/tools/suggestions/`, `agent/tools/propose/`, and `location/` before starting.
- **#749 § 5 (system prompt, owns `agent/prompts/system.md`)**: the `# Filing and cleanup` section is theirs. What this
  plan hands it: "propose a move with `renames` to file under the destination's naming in one group", and "a proposal on
  a volume that isn't connected refuses; offer `connect_volume`". This plan writes no prompt text.
- **Tool-transparency session (`worktree-ask-cmdr-tool-transparency`)**: M9 adds one entry to `ask-cmdr-labels.ts` and
  two catalog keys; whichever lands second rebases onto the other's label shape.

## Deferred (file as issues when this lands)

- The dialog warning per row when a renamed leaf already exists at the destination (the transfer dialog's conflict
  pre-scan, read for drawn rows). Today the conflict prompt at run time is the honest answer.
- A selector over a not-connected member's offline index, re-addressed onto its live sibling (decision 7 refuses
  `notIndexed` instead).
- Proposing a new S3 account (decision 12).
- Whether the user's own "Add anyway" should also save contact-free (M7 raises it; David decides).
