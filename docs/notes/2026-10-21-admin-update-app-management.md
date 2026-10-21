# Why the admin-escalated update couldn't work (2026-10-21)

Evidence behind the updater's "no privilege escalation" decision (`apps/desktop/src-tauri/src/updater/DETAILS.md` § Key
decisions). Until then, an install whose bundle the user couldn't write fell back to
`osascript -e 'do shell script "rsync -a --delete …" with administrator privileges'`. It had never been tested end to
end (`docs/notes/2026-10-15-update-path-proof.md` § Not tested).

## What happened

The M1 (macOS 27.0.1, arm64) had `/Applications/Cmdr.app` 0.52.0 owned `root:wheel`, left by an enterprise-deployment
test with a Jamf-like tool on 2026-10-13. From 2026-10-20 20:36, every background check (about every three hours):

1. found 0.54.0, downloaded it, and verified minisign and the Developer ID signature;
2. failed the direct write (`EACCES`);
3. raised macOS's "Allow administrator access for a script started by "Cmdr"?" dialog, with no custom prompt text;
4. and, after the password, still failed:

```
Admin sync didn't succeed: rsync(84902): error: Resources/bin/: mkpathat: Operation not permitted
rsync(84902): error: mkstempat: '.CodeResources.CNIhOJAsAC': Operation not permitted
rsync(84902): error: CodeResources: utimensat (2): Operation not permitted
```

`EPERM` as root is a policy refusal, not ownership (which would be `EACCES`). The app stayed on 0.52.0, so the next
check prompted again. Each failure also went out as an error report.

## The denial, from the unified log

`/usr/bin/log show --start "2026-10-20 20:41:14" --end "2026-10-20 20:41:17" --predicate 'subsystem == "com.apple.TCC" OR process == "rsync" OR process == "authtrampoline" OR process == "sandboxd"'`
(zsh shadows `log` with a builtin, so call `/usr/bin/log`):

```
20:41:15.496 authtrampoline[84897]  activating connection … com.apple.security.authtrampoline
20:41:15.533 rsync[84902]           Retrieve User by Name
20:41:15.538 tccd  AUTHREQ_ATTRIBUTION: msgID=612.23791, attribution={
                     responsible={identifier=com.apple.authtrampoline, euid=0, …/Security.framework/authtrampoline},
                     accessing={identifier=com.apple.rsync, pid=84902, euid=0, binary_path=/usr/bin/rsync}, …
20:41:15.558 tccd  AUTHREQ_RESULT: msgID=612.23791, authValue=0, authReason=5
20:41:15.560 tccd  forwardMessage … service=kTCCServiceSystemPolicyAppBundles … target_id=com.apple.authtrampoline
20:41:15.560 sandboxd  tcc_server_message_request_authorization(kTCCServiceSystemPolicyAppBundles) failed
20:41:15.685 Cmdr  Download/install failed: … rsync(84902): … Operation not permitted
```

Same rsync pid, 127 ms apart. The 2026-10-21 18:50 attempt shows the same `authtrampoline` and `rsync` sequence; that
one logged no TCC line.

## Why it's structural

`kTCCServiceSystemPolicyAppBundles` is App Management (macOS 13 Ventura+). It lets a process modify a notarized app's
bundle only if the process is signed by the same team, the app's `Info.plist` names it in `NSUpdateSecurityPolicy`, or
the user granted it App Management or Full Disk Access
([lapcatsoftware.com/articles/AppManagement.html](https://lapcatsoftware.com/articles/AppManagement.html), checked
2026-10-21). Running as root isn't on that list.

`do shell script … with administrator privileges` makes Apple's `authtrampoline` the responsible process. It has no team
ID, and TCC doesn't prompt for an unentitled binary, so the write is refused. No password or setting the user controls
changes that. Cmdr writing its own bundle passes (same team), which is why every user-owned install updates.

So on macOS 13+ the escalation could never succeed. On 10.15–12 (no App Management) it probably would have, but only for
a root-owned bundle, which comes almost only from the `.pkg`. That's an MDM install, whose IT pushes packages anyway.

## Proof that the replacement doesn't break updates

Run on the M1, 2026-10-21, against production's genuine 0.54.0 release, with no updater key needed. The branch was built
as an ad-hoc-signed aarch64 release at version 0.53.99:
`pnpm build --target aarch64-apple-darwin --bundles app --config '{"version":"0.53.99","bundle":{"createUpdaterArtifacts":false,"macOS":{"signingIdentity":"-"}}}'`,
with `Cargo.toml`'s version lowered uncommitted. Two copies ran as the real prod install, each after deleting
`update-check.json` so the startup check was due:

- **A copy this user can't write** (every folder under `Contents/` `chmod 555`, the same `EACCES` a root-owned bundle
  gives): `Update available: 0.53.99 -> 0.54.0`, then `This user can't write into …/Contents, which the update needs`
  and `not writable by this user, who is an administrator`. No download, no `osascript`, no password prompt. The notice
  was held, correctly, because the ad-hoc identity has no Full Disk Access, so onboarding's FDA step was on screen.
- **A user-owned copy**: downloaded the real `Cmdr_0.54.0_aarch64.app.tar.gz` (53,371,990 bytes), `Signature verified`,
  passed the Developer ID check, `Update installed successfully`. The bundle then passed
  `codesign --verify --deep --strict` (team `83H6YAQMNP`) and relaunched as `app_version=0.54.0`.

## What would make root-owned installs update

A privileged helper inside `Cmdr.app`, signed by Cmdr's team (`83H6YAQMNP`), started through Authorization Services or
`SMAppService`. App Management treats it as same-team. That's how Sparkle 2's installer gets past the same rule, and it
would also allow custom prompt text. Not built: it's security-sensitive code (a root helper that must vet what it's
asked to do), and the population is MDM-managed. Revisit if `update_check`'s `not_writable_admin_user` count shows
non-MDM users.
