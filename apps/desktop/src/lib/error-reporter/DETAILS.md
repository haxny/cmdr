# Error reporter (frontend) — details

Read this before any non-trivial work here: editing, planning, reorganizing, or advising. `CLAUDE.md` holds the
must-knows; this is the depth.

## The dialog's two modes

`ErrorReportDialog.svelte` reads `errorReportFlow.mode` ONCE at init (like `initialNote`) and branches on a plain
`isAmend` const, so a close mid-flight can't swap the mode under an in-flight submit.

What differs, compose → amend:

- Opened by: `openErrorReportDialog({ from, initialNote? })` (directly from the Help menu, through `openErrorReportFor`
  from every `ReportOffer`) → `openErrorReportDialogForAutoSentReport()`.
- Preview source: `prepareErrorReportPreview()`, which builds a bundle → `getAutoSentReportPreview()`, which reads the
  backend stash, so the manifest and sample lines are the ones that actually shipped.
- "What's about to be sent": a link that opens the WHOLE bundle in the File viewer (`openReportPreview` in
  `report-preview.ts`, over `writeErrorReportPreviewFile(note, email, preview.id)`) → an in-place toggle over the
  stash's manifest and sample lines, since no bundle survives an auto-send to open.
- Submit: `sendErrorReport(note, email, preview.id)` → `amendErrorReport(note, email)` (no id: there's only ever one
  stashed report).
- Submit enabled when: the note is under the cap and the email is valid → that, PLUS a note or an email to carry, since
  the server turns down an amendment with neither.
- "Save bundle to disk (debug)": shown in dev → hidden, there's no local bundle.
- Post-send toast: `kind: 'sent'` → `kind: 'amended'`.
- Copy: everything else, the reference-ID badge and its Copy button included, comes from the same keys or the same
  shape; only the `errorReporter.amend.*` family differs.

The mode lives in the store rather than in the compose request, so a compose caller can't ask for amend mode: only the
auto-sent toast's own entry point reaches it. `closeErrorReportDialog()` resets it to `compose`, so a leftover `amend`
can't leak into the next Help-menu open.

### Why amend exists

One incident used to produce three ids: Flow B auto-sent `ERR-J9BKB` and said so in a toast; the toast's button opened
the compose dialog, which built and displayed a THIRD bundle (`ERR-ZVWQ2`); pressing Send uploaded a SECOND report
(`ERR-AYVM4`). The id the user copied was the one that never existed. Hence the two invariants the tests guard: the
amend path never calls `sendErrorReport`, and the id on screen is the id that shipped.

### The dead end, and why it's a dead end

`getAutoSentReportPreview()` returns `null` when nothing was auto-sent this run (the stash dies with the process), and
`canAmend: false` when the server handed back no amend key. Either one, plus a throwing lookup, lands on
`errorReporter.amend.unavailable`: a sentence pointing at the Help menu, and a Close button. ❌ No fallback send, no
retry loop. Branch on `canAmend`, never on a message (`cmdr/no-error-string-match`).

An amend can land more than once for the same report; amendments accumulate server-side and `canAmend` stays true. The
button is therefore disabled DURING the call, not after it.

## Report offers

Every surface where something went wrong or got slow offers the compose dialog through ONE action, `ReportOffer.svelte`,
handed ONE typed `ReportIncident` (`report-incident.ts`). The incident decides everything the surface used to improvise:

- **The breadcrumb's `from`** (`reportEntryFor`): which KIND of surface offered it (`transferError`, `queueFailure`,
  `listingFailure`, `createRefusal`, `connectionFailure`, `slowOperation`, `errorToast`, `updateFailure`; the Help menu
  is `helpMenu`).
- **The pre-filled note** (`reportNoteFor`): the headline the person saw, then `Reference: <operation>/<kind>` built
  from the typed tags (`WriteOperationError['type']`, `ListingErrorReason['reason']`, `MutationError['type']`,
  `UpgradeFailure`). The reference stays the same whatever language the note is in, so triage can search for it. ❌
  Nothing reads a message to find the kind (`cmdr/no-error-string-match`).
- **The look**: a failure is the "Send error report…" button (or `look="link"` in a sentence, the Settings update row);
  a slow incident is always the lighter "Taking long? Send a report" link, since nothing has gone wrong yet.

Where each surface gets its incident: the toast frame (`ui/DETAILS.md` § Toast system), `failureIncidentFor` for a
retained failure (queue toast and queue row), `stallIncidentFor` for a stall notice (progress dialog and queue row),
`CreateSubmission`'s refusal for the new-folder and new-file dialogs, the dialog's own typed error in
`TransferErrorDialog`, and `FriendlyError.listingReason` in `ErrorPane`.

### Slow is reportable

A slow incident carries `since` (when it stopped moving, `Date.now()` time) rather than a duration, so the note's
duration is the one on screen when the person clicked. It records an `operationSlow` breadcrumb twice: when the notice
first shows (`ReportOffer`'s mount, so a report sent later from the Help menu still carries it) and when it's reported
(`openErrorReportFor`, before the dialog-opened crumb, with the current duration). A notice shown in both windows (the
progress dialog and the queue row) records once per window; the ring holds 50, so that's noise, not loss.

### The dialog lives in the main window

`hostErrorReportDialog()` marks the main window's webview as the host and listens for `open-error-report`. In any other
webview (the queue window's failed or stalled row, the Settings update row) `openErrorReportDialog` emits that event
instead, and the host opens the dialog and focuses its window. The host records `errorReportDialogOpened`, so a
cross-window open logs one crumb, not two. Before the host existed, Settings' "Send error report" flipped a store no
component rendered and did nothing at all.

### Constraints the offers keep

- ❌ Never log at error level to make something reportable: the error level IS Flow B's auto-send threshold.
- One incident, one id: the offer only opens the compose dialog, which mints the id at preview as always.
- Under the managed `DisableCrashAndErrorReports` the offers stay, and the same dialog offers only "Save to disk".

## When the organization turned reports off

Under `DisableCrashAndErrorReports` (`getManagedPolicyView().reportsDisabled`, read reactively) the backend refuses
every send; the dialog stops offering one rather than letting the person hit that refusal:

- Compose: the explanation becomes `errorReporter.dialog.managedOff`, Send and the attach-email checkbox go away, and
  the primary button is "Save to disk" (`saveErrorReportToDisk` with the previewed id and no email, then the
  `BundleSavedToastContent` toast with Reveal in Finder). `canSend` is false, so ⌘Enter does nothing. The dev-only debug
  save button hides, since the primary one does the same.
- Amend: the same sentence and a lone Close, like the dead end above.

`save_error_report_to_disk` is a command in every build for this (`src-tauri/src/error_reporter/DETAILS.md` § Managed
policy).

## Flow B: auto-send toast

When `updates.errorReports` is on, the Rust auto-dispatcher fires `error-report-auto-sent` (payload: server-issued
report ID) after a successful upload. `auto-send-toast.svelte.ts`, initialized from the main window layout's `onMount`,
listens and shows `addToast(AutoSendToastContent, ...)`:

- **Title**: "Error report sent". **Body**: reference ID badge.
- **Actions**: "View or add notes to the report" opens the dialog in amend mode; "Change settings" opens the Settings
  window to flip the opt-in.
- **Auto-dismiss after 10 s** (longer than the default 4 s): auto-sent reports are surprising, so the user needs more
  time to notice and act.

The listener is initialized in `(main)/+layout.svelte` next to the dialog mount and torn down in the matching
`onDestroy`. Idempotent: repeated `init` calls are no-ops.

## Toast data

Each toast gets what it shows as `props` from its one raise: the post-send toast `{ reportId, kind }`, the bundle-saved
toast `{ path }`, and the auto-sent toast `{ reportId }`. Each closes itself through the `toastId` the toast frame hands
it. The post-send toast's id and `kind` travel in one object from one call, so they can't drift apart: an amended report
showing the "Error report sent" sentence would be the same class of lie amend mode exists to fix.

## When a preview or a send doesn't land

- **A preview that didn't build** (compose mode) shows `errorReporter.dialog.prepareFailed` and a Try again button that
  re-runs the same build. Send stays off until a preview exists: a send without one would ship a bundle nobody saw, and
  rebuild the bundle that just didn't build. The raw reason goes to the warn, never the dialog.
- **A send or an amend that didn't land** throws an `ErrorReportSendFailure` (`error-report-send-error.ts`) carrying the
  typed `ErrorReportSendError`. The toast is the surface's lead (`dialog.sendFailedToast` or `amend.addFailedToast`)
  plus `errorReportSendReason`, which words a request failure through `#lib/error-messages/server-request` and the
  dialog's own variants from this catalog. The dialog stays open, so Send is the retry.
- **Every kind logs at warn here, a 4xx included**: an error-level line would auto-send a report through this same
  endpoint, the one that just didn't take it.

## Note-capture timing and gotchas

- The preview loads exactly once per mount, in an `$effect` whose synchronous phase reads NOTHING reactive. The email
  used to be an argument to `prepareErrorReportPreview`, which made `attachEmail.emailToAttach` (a getter over `$state`)
  a tracked dependency: ticking the box or typing one character re-ran the megabyte-scale bundle build and minted a
  fresh report id. `displayedManifest` overlays the live note and email onto the cached manifest instead, and the submit
  ships the current values, so the preview stays accurate with one build.
- `errorReportFlow.initialNote` is captured on mount via `let userNote = $state(errorReportFlow.initialNote)`. Later
  textarea edits are local to the component; closing and reopening reads from the store again.
- Note caps: hard limit 100 000 code points (submit disabled, FE and Rust both enforce), soft counter at 50 000, and the
  server caps the whole payload at 10 MB.
- `<script module>` blocks in Svelte 5 do support `$state`. The compiler warns if you put module-level state in a
  regular `<script>` block by mistake.

## The gallery row

`dialog-gallery` has an `amend` state for this dialog. It seeds the real store and hits the real backend, so on a
machine that hasn't auto-sent anything this run it honestly renders the dead end rather than a staged preview; the row's
`note` says so and how to stage the real thing.
