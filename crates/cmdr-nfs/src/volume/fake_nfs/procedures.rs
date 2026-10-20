//! The fake's NFS procedures: one arm per procedure over the in-memory tree,
//! answering what RFC 1813 says a server answers (the status a failure arm
//! carries, and only the success arm's body after it).

use bytes::Bytes;
use std::collections::BTreeMap;
use std::sync::atomic::Ordering;

use cmdr_fs::ignore_poison::IgnorePoison;

use super::{Body, FakeNfs, proc, status};
use crate::proto::nfs3::{Fattr3, FileHandle, NfsTime};
use crate::proto::server::{nfs3_wire, success_reply};
use crate::xdr::{Decoder, Encoder};

impl FakeNfs {
    #[allow(
        clippy::too_many_lines,
        reason = "one arm per procedure reads better as one table than split across helpers"
    )]
    pub(super) fn answer_nfs(&self, xid: u32, procedure: u32, args: &mut Decoder) -> Bytes {
        let mut tree = self.tree.lock_ignore_poison();
        let knobs = &self.knobs;
        let outcome: Result<Results, u32> = (|| {
            let target = tree.node_of(&args.opaque(64).map_err(|_| status::BADHANDLE)?)?;
            match procedure {
                proc::GETATTR => {
                    let attrs = tree.attrs(target);
                    Ok(boxed(move |enc: &mut Encoder| nfs3_wire::fattr(enc, &attrs)))
                }
                proc::SETATTR => {
                    let before = tree.wcc_before(target);
                    let sattr = decode_sattr(args);
                    let now = tree.tick();
                    let node = tree.nodes.get_mut(&target).expect("a node");
                    if let Some(mode) = sattr.mode {
                        node.mode = mode;
                    }
                    if let (Some(size), Body::File(data)) = (sattr.size, &mut node.body) {
                        data.resize(usize::try_from(size).unwrap_or(0), 0);
                    }
                    node.atime = sattr.atime.unwrap_or(node.atime);
                    node.mtime = sattr.mtime.unwrap_or(node.mtime);
                    node.ctime = now;
                    let after = tree.attrs(target);
                    Ok(boxed(move |enc: &mut Encoder| wcc(enc, before, &after)))
                }
                proc::LOOKUP => {
                    let name = args.opaque(255).map_err(|_| status::INVAL)?;
                    if tree.children(target).is_none() {
                        return Err(status::NOTDIR);
                    }
                    let child = tree.child(target, &name).ok_or(status::NOENT)?;
                    let (handle, attrs, dir) = (tree.handle(child), tree.attrs(child), tree.attrs(target));
                    Ok(boxed(move |enc: &mut Encoder| {
                        nfs3_wire::handle(enc, &handle);
                        nfs3_wire::post_op_attr(enc, Some(&attrs));
                        nfs3_wire::post_op_attr(enc, Some(&dir));
                    }))
                }
                proc::ACCESS => {
                    let asked = args.u32().unwrap_or(0);
                    let attrs = tree.attrs(target);
                    Ok(boxed(move |enc: &mut Encoder| {
                        nfs3_wire::post_op_attr(enc, Some(&attrs));
                        enc.u32(asked);
                    }))
                }
                proc::READLINK => {
                    let Body::Link(link) = &tree.nodes[&target].body else {
                        return Err(status::INVAL);
                    };
                    let (link, attrs) = (link.clone(), tree.attrs(target));
                    Ok(boxed(move |enc: &mut Encoder| {
                        nfs3_wire::post_op_attr(enc, Some(&attrs));
                        enc.opaque(&link);
                    }))
                }
                proc::READ => {
                    let offset = args.u64().unwrap_or(0) as usize;
                    let mut count = args.u32().unwrap_or(0) as usize;
                    let short = knobs.short_reads.load(Ordering::SeqCst) as usize;
                    if short > 0 {
                        count = count.min(short);
                    }
                    let Body::File(data) = &tree.nodes[&target].body else {
                        return Err(status::ISDIR);
                    };
                    let from = offset.min(data.len());
                    let to = (from + count).min(data.len());
                    let (slice, eof) = (data[from..to].to_vec(), to == data.len());
                    let attrs = tree.attrs(target);
                    Ok(boxed(move |enc: &mut Encoder| {
                        nfs3_wire::post_op_attr(enc, Some(&attrs));
                        enc.u32(slice.len() as u32);
                        enc.bool(eof);
                        enc.opaque(&slice);
                    }))
                }
                proc::WRITE => {
                    let offset = args.u64().unwrap_or(0) as usize;
                    let _count = args.u32().unwrap_or(0);
                    let stable = args.u32().unwrap_or(0);
                    let mut data = args.opaque(usize::MAX).unwrap_or_default().to_vec();
                    let short = knobs.short_writes.load(Ordering::SeqCst) as usize;
                    if short > 0 {
                        data.truncate(short);
                    }
                    let before = tree.wcc_before(target);
                    let now = tree.tick();
                    let node = tree.nodes.get_mut(&target).expect("a node");
                    let Body::File(file) = &mut node.body else {
                        return Err(status::ISDIR);
                    };
                    if file.len() < offset + data.len() {
                        file.resize(offset + data.len(), 0);
                    }
                    file[offset..offset + data.len()].copy_from_slice(&data);
                    node.mtime = now;
                    node.ctime = now;
                    let after = tree.attrs(target);
                    let verifier = knobs.verifier.load(Ordering::SeqCst).to_be_bytes();
                    let written = data.len() as u32;
                    Ok(boxed(move |enc: &mut Encoder| {
                        wcc(enc, before, &after);
                        enc.u32(written);
                        enc.u32(stable);
                        enc.fixed_opaque(&verifier);
                    }))
                }
                proc::COMMIT => {
                    let before = tree.wcc_before(target);
                    let after = tree.attrs(target);
                    let verifier = knobs.verifier.load(Ordering::SeqCst).to_be_bytes();
                    Ok(boxed(move |enc: &mut Encoder| {
                        wcc(enc, before, &after);
                        enc.fixed_opaque(&verifier);
                    }))
                }
                proc::CREATE | proc::MKDIR => {
                    let name = args.opaque(255).map_err(|_| status::INVAL)?;
                    let guarded = if procedure == proc::CREATE {
                        let how = args.u32().unwrap_or(0);
                        if how != 2 {
                            let _ = decode_sattr(args);
                        }
                        how != 0
                    } else {
                        let _ = decode_sattr(args);
                        true
                    };
                    if tree.children(target).is_none() {
                        return Err(status::NOTDIR);
                    }
                    let before = tree.wcc_before(target);
                    let id = match tree.child(target, &name) {
                        Some(_) if guarded => return Err(status::EXIST),
                        Some(existing) => {
                            if let Body::File(data) = &mut tree.nodes.get_mut(&existing).expect("a node").body {
                                data.clear();
                            }
                            existing
                        }
                        None if procedure == proc::MKDIR => {
                            tree.insert(target, &name, Body::Dir(BTreeMap::new()), 0o755)
                        }
                        None => tree.insert(target, &name, Body::File(Vec::new()), 0o644),
                    };
                    let (handle, attrs, after) = (tree.handle(id), tree.attrs(id), tree.attrs(target));
                    Ok(boxed(move |enc: &mut Encoder| {
                        nfs3_wire::post_op_fh(enc, Some(&handle));
                        nfs3_wire::post_op_attr(enc, Some(&attrs));
                        wcc(enc, before, &after);
                    }))
                }
                proc::REMOVE | proc::RMDIR => {
                    let name = args.opaque(255).map_err(|_| status::INVAL)?;
                    let child = tree.child(target, &name).ok_or(status::NOENT)?;
                    match (&tree.nodes[&child].body, procedure) {
                        (Body::Dir(_), proc::REMOVE) => return Err(status::ISDIR),
                        (Body::Dir(children), _) if !children.is_empty() => return Err(status::NOTEMPTY),
                        (Body::File(_) | Body::Link(_), proc::RMDIR) => return Err(status::NOTDIR),
                        _ => {}
                    }
                    let before = tree.wcc_before(target);
                    tree.unlink(target, &name);
                    tree.nodes.remove(&child);
                    let after = tree.attrs(target);
                    Ok(boxed(move |enc: &mut Encoder| wcc(enc, before, &after)))
                }
                proc::RENAME => {
                    let from_name = args.opaque(255).map_err(|_| status::INVAL)?;
                    let to_dir = tree.node_of(&args.opaque(64).map_err(|_| status::BADHANDLE)?)?;
                    let to_name = args.opaque(255).map_err(|_| status::INVAL)?;
                    let moving = tree.child(target, &from_name).ok_or(status::NOENT)?;
                    let (from_before, to_before) = (tree.wcc_before(target), tree.wcc_before(to_dir));
                    if let Some(existing) = tree.child(to_dir, &to_name)
                        && existing != moving
                    {
                        let moving_is_dir = matches!(tree.nodes[&moving].body, Body::Dir(_));
                        match &tree.nodes[&existing].body {
                            Body::Dir(children) if !moving_is_dir => {
                                let _ = children;
                                return Err(status::ISDIR);
                            }
                            Body::Dir(children) if !children.is_empty() => return Err(status::NOTEMPTY),
                            Body::File(_) | Body::Link(_) if moving_is_dir => return Err(status::NOTDIR),
                            _ => {}
                        }
                        tree.unlink(to_dir, &to_name);
                        tree.nodes.remove(&existing);
                    }
                    if tree.child(to_dir, &to_name) != Some(moving) {
                        tree.unlink(target, &from_name);
                        let now = tree.tick();
                        tree.link(to_dir, &to_name, moving, now);
                    }
                    let (from_after, to_after) = (tree.attrs(target), tree.attrs(to_dir));
                    Ok(boxed(move |enc: &mut Encoder| {
                        wcc(enc, from_before, &from_after);
                        wcc(enc, to_before, &to_after);
                    }))
                }
                proc::READDIR | proc::READDIRPLUS => {
                    if procedure == proc::READDIRPLUS && knobs.no_readdirplus.load(Ordering::SeqCst) {
                        return Err(status::NOTSUPP);
                    }
                    let cookie = args.u64().unwrap_or(0);
                    let children = tree.children(target).ok_or(status::NOTDIR)?;
                    let page = knobs.page_entries.load(Ordering::SeqCst) as usize;
                    // Cookies 1 and 2 are `.` and `..`, which real servers list
                    // and the client drops by name.
                    let mut listed: Vec<(u64, Vec<u8>, u64)> =
                        vec![(1, b".".to_vec(), target), (2, b"..".to_vec(), target)];
                    listed.extend(
                        children
                            .iter()
                            .enumerate()
                            .map(|(index, (name, id))| (index as u64 + 3, name.clone(), *id)),
                    );
                    let total = listed.len() as u64;
                    let rows: Vec<(u64, Vec<u8>, u64, Fattr3, FileHandle)> = listed
                        .into_iter()
                        .filter(|(at, _, _)| *at > cookie)
                        .take(page)
                        .map(|(at, name, id)| (at, name, id, tree.attrs(id), tree.handle(id)))
                        .collect();
                    let eof = rows.last().is_none_or(|(at, ..)| *at == total);
                    let dir = tree.attrs(target);
                    let plus = procedure == proc::READDIRPLUS;
                    Ok(boxed(move |enc: &mut Encoder| {
                        nfs3_wire::post_op_attr(enc, Some(&dir));
                        enc.fixed_opaque(&[0u8; 8]);
                        for (at, name, id, attrs, handle) in &rows {
                            enc.bool(true);
                            enc.u64(*id);
                            enc.opaque(name);
                            enc.u64(*at);
                            if plus {
                                nfs3_wire::post_op_attr(enc, Some(attrs));
                                nfs3_wire::post_op_fh(enc, Some(handle));
                            }
                        }
                        enc.bool(false);
                        enc.bool(eof);
                    }))
                }
                proc::FSSTAT => {
                    let attrs = tree.attrs(target);
                    Ok(boxed(move |enc: &mut Encoder| {
                        nfs3_wire::post_op_attr(enc, Some(&attrs));
                        for value in [1_000_000_u64, 400_000, 300_000, 1000, 500, 500] {
                            enc.u64(value);
                        }
                        enc.u32(0);
                    }))
                }
                proc::FSINFO => {
                    let size = knobs.transfer_size.load(Ordering::SeqCst);
                    let attrs = tree.attrs(target);
                    Ok(boxed(move |enc: &mut Encoder| {
                        nfs3_wire::post_op_attr(enc, Some(&attrs));
                        encode_fsinfo(enc, size);
                    }))
                }
                proc::PATHCONF => {
                    let attrs = tree.attrs(target);
                    Ok(boxed(move |enc: &mut Encoder| {
                        nfs3_wire::post_op_attr(enc, Some(&attrs));
                        enc.u32(32_000);
                        enc.u32(255);
                        enc.bool(true);
                        enc.bool(true);
                        enc.bool(false);
                        enc.bool(true);
                    }))
                }
                _ => Err(status::NOTSUPP),
            }
        })();
        match outcome {
            Ok(body) => success_reply(xid, |enc| {
                enc.u32(0);
                body(enc);
            }),
            Err(status) => success_reply(xid, |enc| enc.u32(status)),
        }
    }
}

/// A reply's results after its `NFS3_OK`.
type Results = Box<dyn FnOnce(&mut Encoder)>;

fn boxed(results: impl FnOnce(&mut Encoder) + 'static) -> Results {
    Box::new(results)
}

/// What the fake answers FSINFO with, `size` as both preferences.
fn encode_fsinfo(enc: &mut Encoder, size: u32) {
    enc.u32(1024 * 1024);
    enc.u32(size);
    enc.u32(4096);
    enc.u32(1024 * 1024);
    enc.u32(size);
    enc.u32(4096);
    enc.u32(size);
    enc.u64(u64::MAX);
    enc.u32(0);
    enc.u32(1);
    enc.u32(0x1b);
}

pub(super) fn fake_fsinfo(size: u32) -> crate::proto::nfs3::FsInfo {
    crate::proto::nfs3::FsInfo {
        rtmax: 1024 * 1024,
        rtpref: size,
        wtmax: 1024 * 1024,
        wtpref: size,
        dtpref: size,
        maxfilesize: u64::MAX,
        time_delta: NfsTime {
            seconds: 0,
            nseconds: 1,
        },
        properties: 0x1b,
    }
}

fn wcc(enc: &mut Encoder, before: (u64, NfsTime, NfsTime), after: &Fattr3) {
    enc.bool(true);
    enc.u64(before.0);
    enc.u32(before.1.seconds);
    enc.u32(before.1.nseconds);
    enc.u32(before.2.seconds);
    enc.u32(before.2.nseconds);
    nfs3_wire::post_op_attr(enc, Some(after));
}

/// The parts of a `sattr3` the fake applies.
struct SetAttrs {
    mode: Option<u32>,
    size: Option<u64>,
    atime: Option<NfsTime>,
    mtime: Option<NfsTime>,
}

fn decode_sattr(args: &mut Decoder) -> SetAttrs {
    let flag_u32 = |args: &mut Decoder| {
        if args.bool().unwrap_or(false) {
            args.u32().ok()
        } else {
            None
        }
    };
    let mode = flag_u32(args);
    let _uid = flag_u32(args);
    let _gid = flag_u32(args);
    let size = if args.bool().unwrap_or(false) {
        args.u64().ok()
    } else {
        None
    };
    let time = |args: &mut Decoder| match args.u32().unwrap_or(0) {
        2 => Some(NfsTime {
            seconds: args.u32().unwrap_or(0),
            nseconds: args.u32().unwrap_or(0),
        }),
        _ => None,
    };
    let atime = time(args);
    let mtime = time(args);
    SetAttrs {
        mode,
        size,
        atime,
        mtime,
    }
}
