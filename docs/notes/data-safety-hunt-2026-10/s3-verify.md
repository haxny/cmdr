# S3 backend: data-safety hunt verification

Verifier pass over `s3-hunt.md`. All excerpts are verbatim and current (`mutation.rs:197`, `listing.rs:60`,
`writes.rs:476`, `server_list_file.rs:33`, `streams.rs:50`, `multipart_upload.rs:600`, `upload_ledger.rs:126`).

### s3-1

- **Verdict:** confirmed (shadowed-name trigger); plausible (second trigger, narrower than the hunter wrote).
- **Severity:** high (agree). Silent, permanent loss of an object the user never selected; the trigger needs a name
  collision that other tools produce (`aws s3 sync` writes no markers), unusual but plausible.
- **Evidence, shadowed name:**
  - `listing.rs:60–77`: when `<name> (file)` is already taken, the shadowed object maps to `None` and is never marked
    `beside_folder`, so `paths.rs:115` `note_beside_folders` never inserts it. `listing_test.rs:118` pins the hiding; no
    test covers deleting the folder beside a hidden file.
  - `paths.rs:93–98`: the folder row resolves `Holder::Either`.
  - Delete: the volume walker deletes files first, then each dir via `delete_with_cancel` (`delete/walker.rs:981`),
    result ignored. Move: `source_sweep.rs:174` deletes the ledger's files, then `volume.delete(dir)` (`:185`).
  - `mutation.rs:204–211`: with `notes/` now empty (no marker) the listing is `Nothing`, `Either` falls to the HEAD of
    the bare key, finds the hidden object, and `delete_key` removes it. Nothing above passes the walker's "this is a
    directory" to `Volume::delete`.
- **Evidence, second trigger:** a redial into a NOT-registered id can't hit it: `connect_saved_place` refuses an
  already-registered id (`commands/servers.rs:178–184`), and an unregistered place has no pane standing on it (its
  unmount sends panes home), so the next pane visit lists through the new volume and records the set. The one reachable
  swap under a live id is ADD mode on a place that's already connected: `connect_server` → `connect_and_register_since`
  always calls `install_retiring_incumbent` (`s3_volume_wiring.rs:146`), which builds a fresh `S3Volume` with an empty
  `beside_folders` (`volume/mod.rs:366`), and the sheet explicitly lets a live place be re-added (`open-sign-in.ts:300`,
  "A place that was live before the Add keeps its session"). S3 has no watcher, so a pane that doesn't relist after
  `volumes-changed` keeps its old `notes` + `notes (file)` rows; deleting folder `notes` from it then takes the `Either`
  route and deletes the file `notes`. Not traced: whether the pane relists on that `volumes-changed`. This trigger also
  hits the ordinary (non-shadowed) beside-folder case, which the hunter's "recorded" guard otherwise protects.
- The fix should not depend on the in-memory set at all: the engine knows the entry is a directory, so `delete` of a
  directory-kind entry must never fall back to the object.

### s3-2

- **Verdict:** plausible.
- **Severity:** medium (agree).
- **Evidence:** `writes.rs:404–411` runs `settle_cut_off_put` only on an interrupted send; its HEAD failing on the dead
  link returns `None` at once (`writes.rs:481–483`), with no record of the token. `noting`'s `select!`
  (`volume_impl.rs:47–51`) can drop the whole write future on `liveness.lost()`, so no settle runs. On the retry,
  `settle_unanswered_create` (`interrupted_landing.rs:133–144`) sees a size mismatch → `RunAgain`, leaving the name
  alone; the new attempt's `CreateNew` then meets the truncated object as a conflict. The user sees a conflict whose
  destination is smaller, and Skip keeps the corrupt copy (the move keeps its source). Unverified step: that a server on
  the "Other" path really publishes the cut-off body after a connection drop the client never saw closed cleanly
  (VersityGW per `crates/cmdr-s3/CLAUDE.md`).

### s3-3

- **Verdict:** confirmed.
- **Severity:** medium (agree): recoverable by re-entry, but pins, names, auto-reconnect switches, and bucket places go.
- **Evidence:** `server_list_file.rs:39–42` swallows a parse failure into `T::default()` and still returns the path;
  `s3_known_places.rs:294–297` sets `STORE_PATH`; `remember` calls `save()` (`s3_known_places.rs:436`), which writes the
  empty-plus-one store durably. `S3ProviderChoice` (`s3_known_places.rs:37–39`) is internally tagged with no catch-all,
  so one unknown `kind` fails the whole file. The file header (`server_list_file.rs:3–7`) documents the swallow as a
  decision ("losing the file costs one re-entry"), but not the overwrite of the unparsed file.
- **Shared loader:** yes. `server_list_file::load` also serves `sftp_known_servers.rs:168` and
  `webdav_known_servers.rs:215`; `sftp-webdav-hunt.md` doesn't report it.
- **Cross-cutting:** `smb-4` (`smb-hunt.md`) is the same pattern through separate loaders (`known_shares.rs:174`,
  `manual_servers.rs:458`). Recommend one issue: "stores that load empty on a parse failure are then saved over",
  covering `server_list_file::load` (S3, SFTP, WebDAV), `known_shares`, and `manual_servers`. `settings-hunt.md` names a
  `state_recovery::set_aside` mechanism that fits as the shared fix (move the unparsed file aside, refuse to arm the
  save path from it).

### s3-4

- **Verdict:** confirmed (the engine side); plausible end to end (trigger needs a close-delimited HTTP/1.1 body).
- **Severity:** low (agree).
- **Evidence:** `streams.rs:93–117` returns `None` at body end without comparing `read` with `total`; a missing length
  becomes `Known(0)` (`streams.rs:58–60`, `:119–121`). The local writer loops to EOF and never compares `bytes_written`
  with `length` (`cmdr-local/src/local_posix/streams.rs:239–270`); `source_reads::watch` only flags read errors;
  `stream_pipe.rs:439–451` commits and returns `written.bytes` with no check against `size` or `source_facts.size`. The
  move sweep (`source_sweep.rs:45–57`, `:156–166`) compares the SOURCE's stamp with the ledger, never the destination's
  size. So nothing checks a download's length before a move deletes its source.
- **Cross-cutting:** `sftp-webdav-7` (WebDAV download with no `Content-Length` declares zero) is the same engine gap. A
  single engine-side guard in `stream_pipe_file` (fail when `written.bytes` differs from a known `size`) would close
  both, plus each backend reporting `Unknown` for a missing length.

### s3-5

- **Verdict:** confirmed.
- **Severity:** low (agree): the user's provider bill, not their files.
- **Evidence:** `multipart_upload.rs:600–603` keys records by `credential_service()#key id`, and `credential_service`
  includes host and port (`params.rs:174–176`); `leftovers` keeps only records matching the connecting account
  (`upload_ledger.rs:126`). Nothing in `server_move.rs` or `s3_known_places.rs` touches the ledger, so an endpoint edit
  orphans the old records for good.
