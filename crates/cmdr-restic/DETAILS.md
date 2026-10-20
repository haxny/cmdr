# `cmdr-restic` details

The spec and the research behind it: GitHub issue #422.

## Detection

A folder is a restic repo when its listing holds a regular file named `config` and the folders `data`, `index`, `keys`,
and `snapshots`. `locks` is optional: restic creates it lazily, and a copied repo often drops it. A symlink counts as
neither: restic never writes one, and its target is unknown from the listing alone.

## The public surface is capped

`index-crate-isolation` holds this crate to one root promise, no public module, and no public items, exactly what it
exposes:

- `is_restic_repo`: the host's overlay asks it per listing.
