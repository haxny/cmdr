//! Measures what opening and indexing a real restic repo costs, through a Cmdr
//! volume. Dev-only evidence for `docs/notes/restic/open-cost.md`; never shipped.
//!
//! ```sh
//! RESTIC_PASSWORD="$(secret NAME)" /usr/bin/time -l \
//!   cargo run --release -p cmdr-restic --example open_cost -- mount ids /Volumes/naspi/backups/repo
//! ```
//!
//! - Via: `mount` (a `LocalPosixVolume` over the OS mount) or `smb` (a direct
//!   `cmdr-smb` session; needs `SMB_SERVER`, `SMB_SHARE`, `SMB_USER`, and
//!   `SMB_PASSWORD`, and the repo path is then relative to the share root).
//! - Phase: `snapshots` (open + snapshot list + repo file counts), `ids`
//!   (`to_indexed_ids` + a three-level tree walk), `full` (`to_indexed` + a
//!   100 MiB file read), `cache` (`ids` with rustic's cache in `PROBE_CACHE_DIR`).
//!
//! `PROBE_HELD_READERS=1` reads packs through held-open readers, as the crate's
//! adapter does, instead of one `read_range` each.
//!
//! One phase per process, so `/usr/bin/time -l`'s peak RSS belongs to it alone.
//! Reads only: every write the repo sees is refused and counted. The password
//! comes from `RESTIC_PASSWORD` and is never printed.

#![allow(clippy::print_stdout, reason = "a measurement harness prints its measurements")]

use bytes::Bytes;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::{MountClass, Volume, VolumeError, VolumeRandomReader};
use cmdr_local::LocalPosixVolume;
use cmdr_smb::volume::{MountAnchor, SmbConnectionParams, connect_smb_volume};
use rustic_core::repofile::{Node, NodeType};
use rustic_core::{
    BytesList, Credentials, ErrorKind, FileType, Id, IndexedFull, IndexedTree, ReadBackend, Repository,
    RepositoryBackends, RepositoryOptions, RusticError, RusticResult, TreeId, WriteBackend,
};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::runtime::{Handle, Runtime};

const FILE_TYPES: [FileType; 5] = [
    FileType::Config,
    FileType::Key,
    FileType::Snapshot,
    FileType::Index,
    FileType::Pack,
];

/// Bytes, calls, and concurrency for one repo file type.
#[derive(Default)]
struct Counter {
    calls: AtomicU64,
    bytes: AtomicU64,
    in_flight: AtomicUsize,
    peak_in_flight: AtomicUsize,
}

/// rustic's storage trait over a Cmdr volume, counting everything it reads.
/// The same shape as the crate's own adapter, plus the counters.
struct CountingBackend {
    parent: Arc<dyn Volume>,
    root: PathBuf,
    handle: Handle,
    by_type: [Counter; 5],
    all_in_flight: AtomicUsize,
    all_peak_in_flight: AtomicUsize,
    write_attempts: AtomicUsize,
    /// Packs held open, least recently used first, when `PROBE_HELD_READERS=1`.
    held: Option<Mutex<HeldPacks>>,
}

/// Pack paths and their open readers, least recently used first.
type HeldPacks = Vec<(PathBuf, Arc<Box<dyn VolumeRandomReader>>)>;

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn type_slot(tpe: FileType) -> usize {
    FILE_TYPES.iter().position(|t| *t == tpe).unwrap_or(0)
}

fn io_error(err: VolumeError) -> Box<RusticError> {
    RusticError::with_source(ErrorKind::Backend, "The probe's volume read didn't work.", err)
}

impl CountingBackend {
    fn path_of(&self, tpe: FileType, id: &Id) -> PathBuf {
        let hex = id.to_hex();
        match tpe {
            FileType::Config => self.root.join("config"),
            FileType::Pack => self.root.join("data").join(&hex[..2]).join(&*hex),
            _ => self.root.join(tpe.dirname()).join(&*hex),
        }
    }

    /// Runs one read with the in-flight gauges up, and books its bytes.
    fn counted(&self, tpe: FileType, read: impl FnOnce() -> Result<Vec<u8>, VolumeError>) -> RusticResult<Bytes> {
        let counter = &self.by_type[type_slot(tpe)];
        let now = counter.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        counter.peak_in_flight.fetch_max(now, Ordering::SeqCst);
        let all = self.all_in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.all_peak_in_flight.fetch_max(all, Ordering::SeqCst);
        let result = read();
        counter.in_flight.fetch_sub(1, Ordering::SeqCst);
        self.all_in_flight.fetch_sub(1, Ordering::SeqCst);
        let bytes = result.map_err(io_error)?;
        counter.calls.fetch_add(1, Ordering::Relaxed);
        counter.bytes.fetch_add(bytes.len() as u64, Ordering::Relaxed);
        Ok(Bytes::from(bytes))
    }

    fn list_dir(&self, dir: &Path) -> RusticResult<Vec<(Id, u32)>> {
        let entries = match self.handle.block_on(self.parent.list_directory(dir, None)) {
            Ok(entries) => entries,
            Err(VolumeError::NotFound(_)) => return Ok(Vec::new()),
            Err(err) => return Err(io_error(err)),
        };
        Ok(entries
            .into_iter()
            .filter(|e| !e.is_directory)
            .filter_map(|e| {
                Some((
                    e.name.parse().ok()?,
                    u32::try_from(e.size.unwrap_or(0)).unwrap_or(u32::MAX),
                ))
            })
            .collect())
    }

    fn report(&self, label: &str) {
        for tpe in FILE_TYPES {
            let c = &self.by_type[type_slot(tpe)];
            let calls = c.calls.load(Ordering::Relaxed);
            if calls > 0 {
                println!(
                    "  {label} reads {:?}: {calls} calls, {} bytes, peak {} concurrent",
                    tpe,
                    c.bytes.load(Ordering::Relaxed),
                    c.peak_in_flight.load(Ordering::Relaxed)
                );
            }
        }
        println!(
            "  {label} peak concurrent reads (all types): {}; write attempts: {}",
            self.all_peak_in_flight.load(Ordering::Relaxed),
            self.write_attempts.load(Ordering::Relaxed)
        );
    }

    /// With `PROBE_HELD_READERS=1`, the pack's reader held open the way the crate's
    /// adapter holds them (up to 8, least recently used let go first); otherwise
    /// `None`, and the read is one `read_range` (open + read + close on SMB).
    fn held_reader(&self, tpe: FileType, path: &Path) -> Result<Option<Arc<Box<dyn VolumeRandomReader>>>, VolumeError> {
        let Some(held) = self.held.as_ref().filter(|_| tpe == FileType::Pack) else {
            return Ok(None);
        };
        {
            let mut held = lock(held);
            if let Some(at) = held.iter().position(|(p, _)| p == path) {
                let entry = held.remove(at);
                let reader = Arc::clone(&entry.1);
                held.push(entry);
                return Ok(Some(reader));
            }
        }
        let reader = Arc::new(self.handle.block_on(self.parent.open_random_reader(path))?);
        let mut held = lock(held);
        held.push((path.to_path_buf(), Arc::clone(&reader)));
        if held.len() > 8 {
            held.remove(0);
        }
        Ok(Some(reader))
    }

    fn reset(&self) {
        for c in &self.by_type {
            c.calls.store(0, Ordering::Relaxed);
            c.bytes.store(0, Ordering::Relaxed);
            c.peak_in_flight.store(0, Ordering::Relaxed);
        }
        self.all_peak_in_flight.store(0, Ordering::Relaxed);
    }
}

impl ReadBackend for CountingBackend {
    fn location(&self) -> String {
        self.root.display().to_string()
    }

    fn list_with_size(&self, tpe: FileType) -> RusticResult<Vec<(Id, u32)>> {
        match tpe {
            FileType::Config => Ok(vec![(Id::default(), 0)]),
            FileType::Pack => {
                let data = self.root.join("data");
                let subdirs = self
                    .handle
                    .block_on(self.parent.list_directory(&data, None))
                    .map_err(io_error)?;
                let mut out = Vec::new();
                for sub in subdirs.into_iter().filter(|e| e.is_directory) {
                    out.extend(self.list_dir(&data.join(&sub.name))?);
                }
                Ok(out)
            }
            _ => self.list_dir(&self.root.join(tpe.dirname())),
        }
    }

    fn read_full(&self, tpe: FileType, id: &Id) -> RusticResult<Bytes> {
        let path = self.path_of(tpe, id);
        self.counted(tpe, || {
            self.handle.block_on(async {
                let mut stream = self.parent.open_read_stream(&path).await?;
                let mut out = Vec::new();
                while let Some(chunk) = stream.next_chunk().await {
                    out.extend_from_slice(&chunk?);
                }
                Ok(out)
            })
        })
    }

    fn read_partial(&self, tpe: FileType, id: &Id, _cacheable: bool, offset: u32, length: u32) -> RusticResult<Bytes> {
        let path = self.path_of(tpe, id);
        self.counted(tpe, || match self.held_reader(tpe, &path)? {
            Some(reader) => self.handle.block_on(reader.read_at(u64::from(offset), length as usize)),
            None => self
                .handle
                .block_on(self.parent.read_range(&path, u64::from(offset), length as usize)),
        })
    }

    fn warmup_path(&self, _tpe: FileType, _id: &Id) -> String {
        String::new()
    }
}

impl WriteBackend for CountingBackend {
    fn create(&self) -> RusticResult<()> {
        self.write_attempts.fetch_add(1, Ordering::SeqCst);
        Err(RusticError::new(ErrorKind::Unsupported, "The probe is read-only."))
    }

    fn write_bytes(&self, _tpe: FileType, _id: &Id, _cacheable: bool, _content: BytesList) -> RusticResult<()> {
        self.create()
    }

    fn remove(&self, _tpe: FileType, _id: &Id, _cacheable: bool) -> RusticResult<()> {
        self.create()
    }
}

fn secs(start: Instant) -> String {
    format!("{:.3} s", start.elapsed().as_secs_f64())
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} isn't set"))
}

fn build_parent(via: &str, rt: &Runtime, host: VolumeHost, repo: &str) -> (Arc<dyn Volume>, PathBuf) {
    match via {
        "mount" => {
            let volume = LocalPosixVolume::on_mount("probe", "/", MountClass::SmbShare, host);
            (Arc::new(volume), PathBuf::from(repo))
        }
        "smb" => {
            // A made-up anchor, so nothing can reach the OS mount by accident.
            let anchor = PathBuf::from("/cmdr-probe-smb");
            let (server, share) = (env("SMB_SERVER"), env("SMB_SHARE"));
            let password = env("SMB_PASSWORD");
            let params = SmbConnectionParams::new(&server, &share, 445, Some(&env("SMB_USER")), Some(&password));
            let id = cmdr_fs::volume::smb_volume_id(&server, 445, &share);
            let start = Instant::now();
            let volume = rt
                .block_on(connect_smb_volume(
                    &share,
                    MountAnchor::at_share_root(&anchor),
                    &id,
                    params,
                    host,
                ))
                .expect("SMB connect");
            println!("smb connect: {}", secs(start));
            (Arc::new(volume), anchor.join(repo.trim_start_matches('/')))
        }
        other => panic!("unknown via {other:?}: mount or smb"),
    }
}

/// Walks the top `levels` levels below `root`, loading every directory's tree.
fn walk_levels<S: IndexedTree>(repo: &Repository<S>, root: TreeId, levels: usize) -> (usize, usize) {
    let (mut trees, mut nodes) = (0, 0);
    let mut frontier = vec![root];
    for _ in 0..levels {
        let mut next = Vec::new();
        for id in frontier {
            let tree = repo.get_tree(&id).expect("get_tree");
            trees += 1;
            nodes += tree.nodes.len();
            next.extend(tree.nodes.iter().filter_map(|n| n.subtree));
        }
        frontier = next;
    }
    (trees, nodes)
}

/// Already-compressed formats: their bytes cross the wire at full size, so a
/// read of one measures throughput. (A sparse disk image reads 100 MiB from
/// 3 MB of packs.)
const INCOMPRESSIBLE: [&str; 9] = ["mp4", "mov", "mkv", "zip", "dmg", "gz", "xz", "zst", "7z"];

fn is_incompressible(node: &Node) -> bool {
    Path::new(&node.name())
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| INCOMPRESSIBLE.contains(&e.to_ascii_lowercase().as_str()))
}

/// Breadth-first search for an already-compressed regular file of at least
/// `min` bytes, capped at `max_trees` loaded trees.
fn find_big_file<S: IndexedTree>(repo: &Repository<S>, root: TreeId, min: u64, max_trees: usize) -> Option<Node> {
    let mut queue = VecDeque::from([root]);
    let mut loaded = 0;
    while let Some(id) = queue.pop_front() {
        if loaded >= max_trees {
            return None;
        }
        loaded += 1;
        let tree = repo.get_tree(&id).ok()?;
        for node in tree.nodes {
            if matches!(node.node_type, NodeType::File) && node.meta.size >= min && is_incompressible(&node) {
                return Some(node);
            }
            if let Some(sub) = node.subtree {
                queue.push_back(sub);
            }
        }
    }
    None
}

fn read_file<S: IndexedFull>(repo: &Repository<S>, node: &Node, total: usize) {
    let file = repo.open_file(node).expect("open_file");
    let chunk = 4 << 20;
    let start = Instant::now();
    let mut read = 0;
    while read < total {
        let bytes = repo
            .read_file_at(&file, read, chunk.min(total - read))
            .expect("read_file_at");
        if bytes.is_empty() {
            break;
        }
        read += bytes.len();
    }
    let elapsed = start.elapsed().as_secs_f64();
    println!(
        "file read: {read} bytes in {elapsed:.3} s = {:.1} MB/s",
        read as f64 / 1e6 / elapsed
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [via, phase, repo_path] = args.as_slice() else {
        panic!("usage: open_cost <mount|smb> <snapshots|ids|full|cache> <repo path>");
    };
    let rt = Runtime::new().expect("runtime");
    let host = VolumeHost::builder().runtime(rt.handle().clone()).build();
    let (parent, root) = build_parent(via, &rt, host, repo_path);
    let backend = Arc::new(CountingBackend {
        parent,
        root,
        handle: rt.handle().clone(),
        by_type: Default::default(),
        all_in_flight: AtomicUsize::new(0),
        all_peak_in_flight: AtomicUsize::new(0),
        write_attempts: AtomicUsize::new(0),
        held: (std::env::var("PROBE_HELD_READERS").as_deref() == Ok("1")).then(|| Mutex::new(Vec::new())),
    });
    println!("held pack readers: {}", backend.held.is_some());

    let mut options = RepositoryOptions::default();
    if phase == "cache" {
        options.cache_dir = Some(PathBuf::from(env("PROBE_CACHE_DIR")));
    } else {
        options.no_cache = true;
    }
    let password = zeroize::Zeroizing::new(env("RESTIC_PASSWORD"));
    let backends = RepositoryBackends::new(Arc::clone(&backend) as _, None);

    let start = Instant::now();
    let repo = Repository::new(&options, &backends)
        .and_then(|r| r.open(&Credentials::password(password.as_str())))
        .expect("open");
    drop(password);
    println!("open (scrypt + key + config): {}", secs(start));
    backend.report("open");
    backend.reset();

    let start = Instant::now();
    let snapshots = repo.get_all_snapshots().expect("snapshots");
    println!("get_all_snapshots: {} snapshots in {}", snapshots.len(), secs(start));
    backend.report("snapshots");
    backend.reset();
    let latest = snapshots.iter().max_by_key(|s| s.time.timestamp()).expect("a snapshot");
    let latest_tree = latest.tree;

    match phase.as_str() {
        "snapshots" => {
            for tpe in [FileType::Key, FileType::Index, FileType::Snapshot] {
                let start = Instant::now();
                let files = backend.list_with_size(tpe).expect("list");
                let total: u64 = files.iter().map(|(_, s)| u64::from(*s)).sum();
                println!(
                    "{tpe:?} files: {} totalling {total} bytes (listed in {})",
                    files.len(),
                    secs(start)
                );
            }
        }
        "ids" | "cache" => {
            let start = Instant::now();
            let repo = repo.to_indexed_ids().expect("to_indexed_ids");
            println!("to_indexed_ids: {}", secs(start));
            backend.report("to_indexed_ids");
            for round in 1..=2 {
                backend.reset();
                let start = Instant::now();
                let (trees, nodes) = walk_levels(&repo, latest_tree, 3);
                println!(
                    "walk 3 levels (round {round}): {trees} trees, {nodes} nodes in {}",
                    secs(start)
                );
                backend.report("walk");
            }
        }
        "full" => {
            let start = Instant::now();
            let repo = repo.to_indexed().expect("to_indexed");
            println!("to_indexed: {}", secs(start));
            backend.report("to_indexed");
            backend.reset();
            let start = Instant::now();
            let node = find_big_file(&repo, latest_tree, 100 << 20, 20_000).expect("a file of 100 MiB or more");
            println!("found a {} byte file in {}", node.meta.size, secs(start));
            for round in 1..=2 {
                backend.reset();
                print!("round {round} ");
                read_file(&repo, &node, 100 << 20);
                backend.report("file read");
            }
        }
        other => panic!("unknown phase {other:?}"),
    }
}
