//! One shared private-filesystem helper for every managed-state write.
//!
//! Several exa-agent instances routinely run at once (agent fleets, `--jobs`, a human shell
//! beside a script). Every managed-state file the CLI owns — `config.toml`,
//! `credentials.json`, `pending-runs.jsonl`, `--trace` files, `--max-output-bytes` spills —
//! is therefore written through this module so that:
//!
//! * a read-modify-write cycle holds an exclusive lock for the *whole* transaction (unique
//!   temp names alone only prevent a torn file, never a lost update),
//! * new files are `0600` and new managed directories `0700` **from creation**, independent of
//!   the caller's umask, and never by chmod-ing something that already exists,
//! * replacement is atomic: unique `O_EXCL` sibling temp, `fsync`, `rename`, `fsync` parent,
//! * a lock file that someone replaced with a symlink fails the operation closed rather than
//!   following the link.
//!
//! Explicit user-controlled destinations (`--output FILE`, an `EXA_AGENT_*` path the caller
//! chose) keep their own contracts and are deliberately *not* routed through here.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Mode for every file this module creates.
pub const PRIVATE_FILE_MODE: u32 = 0o600;
/// Mode for every managed-state directory this module creates.
pub const PRIVATE_DIR_MODE: u32 = 0o700;

/// Per-process counter so two threads (or two temp files in one millisecond) cannot collide on
/// a temp name even before `O_EXCL` rejects the loser.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Create `dir` and any missing parents, giving **newly created** directories `0700`.
///
/// Directories that already exist are left exactly as they are: silently tightening a
/// directory the user created (or `$XDG_STATE_HOME` itself) is not this function's business.
pub fn create_dir_all_private(dir: &Path) -> io::Result<()> {
    if dir.as_os_str().is_empty() || dir.is_dir() {
        return Ok(());
    }
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(PRIVATE_DIR_MODE);
    }
    builder.create(dir)
}

/// Create `path`'s parent directory (0700 when new) if it has one.
pub fn create_parent_dir_private(path: &Path) -> io::Result<()> {
    match path.parent().filter(|p| !p.as_os_str().is_empty()) {
        Some(parent) => create_dir_all_private(parent),
        None => Ok(()),
    }
}

fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(PRIVATE_FILE_MODE);
    }
    options
}

/// Open `path` for appending, creating it `0600` if it does not exist.
///
/// An existing file keeps its mode: the caller asked to append to *that* file, and re-chmod-ing
/// a path a user or another tool set up is a side effect nobody asked for.
pub fn open_append_private(path: &Path) -> io::Result<File> {
    private_options().create(true).append(true).open(path)
}

/// Create `path` fresh, failing if anything is already there. `0600` from creation.
pub fn create_new_private(path: &Path) -> io::Result<File> {
    private_options().create_new(true).write(true).open(path)
}

/// Sync the parent after rename. Only unsupported directory sync is ignored;
/// permission and I/O errors remain visible even when the replacement is installed.
fn sync_parent_dir(path: &Path) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    #[cfg(unix)]
    {
        use rustix::fs::{Mode, OFlags};
        let fd = rustix::fs::open(
            parent,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        match rustix::fs::fsync(&fd) {
            Ok(()) => {}
            Err(rustix::io::Errno::INVAL | rustix::io::Errno::OPNOTSUPP) => {}
            Err(err) => return Err(err.into()),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = parent;
    }
    Ok(())
}

/// A uniquely-named private temp file beside its eventual destination, renamed into place by
/// [`TempFile::persist`] and deleted by `Drop` if the caller never gets that far.
struct TempFile {
    path: PathBuf,
    file: Option<File>,
    persisted: bool,
}

impl TempFile {
    /// Create a unique `0600` temp file in `target`'s directory (same filesystem, so the final
    /// `rename` is atomic). The name embeds pid + counter + nanos and is created with `O_EXCL`,
    /// so a concurrent writer can never be handed the same temp path.
    fn new_beside(target: &Path) -> io::Result<Self> {
        create_parent_dir_private(target)?;
        let dir = target
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let stem = target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("exa-agent");
        let mut last_err = None;
        for _ in 0..64 {
            let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0);
            let candidate = dir.join(format!(".{stem}.{}.{n}.{nanos}.tmp", std::process::id()));
            match create_new_private(&candidate) {
                Ok(file) => {
                    return Ok(Self {
                        path: candidate,
                        file: Some(file),
                        persisted: false,
                    })
                }
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                    last_err = Some(err);
                    continue;
                }
                Err(err) => return Err(err),
            }
        }
        Err(last_err.unwrap_or_else(|| {
            io::Error::other("could not create a unique temporary file after 64 attempts")
        }))
    }

    /// Mutable handle for streaming a payload in without buffering it twice.
    fn file_mut(&mut self) -> &mut File {
        self.file
            .as_mut()
            .expect("temp file handle is taken only by persist(), which consumes self")
    }

    /// `fsync` the contents, `rename` over `target`, then `fsync` the directory.
    fn persist(mut self, target: &Path) -> io::Result<()> {
        let file = self
            .file
            .take()
            .expect("temp file handle is taken only once");
        file.sync_all()?;
        drop(file);
        std::fs::rename(&self.path, target)?;
        self.persisted = true;
        sync_parent_dir(target).map_err(|err| {
            io::Error::new(
                err.kind(),
                format!("replacement installed but parent directory sync failed: {err}"),
            )
        })?;
        Ok(())
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if !self.persisted {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Replace `target` with `bytes` atomically; the new file is `0600` from creation.
pub fn write_private_atomic(target: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let mut temp = TempFile::new_beside(target)?;
    temp.file_mut().write_all(bytes)?;
    temp.persist(target)
}

/// Path of the lock file guarding `target`.
pub fn lock_path_for(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("exa-agent");
    let dir = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    dir.join(format!(".{name}.lock"))
}

/// An exclusive advisory lock held for the lifetime of the guard (released on drop/exit).
#[derive(Debug)]
pub struct LockGuard {
    #[cfg(unix)]
    _fd: std::os::fd::OwnedFd,
    #[cfg(not(unix))]
    _unsupported: (),
}

/// Take an exclusive lock covering an entire read-modify-write of `target`.
///
/// The lock file is opened `O_NOFOLLOW`, so a planted symlink fails the operation closed
/// instead of quietly locking (and creating) something elsewhere. `flock` is per open file
/// description, so two threads of one process contend exactly like two processes do.
#[cfg(unix)]
pub fn lock_exclusive(target: &Path) -> io::Result<LockGuard> {
    use rustix::fs::{FlockOperation, Mode, OFlags};
    let path = lock_path_for(target);
    create_parent_dir_private(&path)?;
    let fd = rustix::fs::open(
        &path,
        OFlags::CREATE | OFlags::RDWR | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )
    .map_err(|errno| {
        // `O_NOFOLLOW` turns a planted symlink into ELOOP on the final component. Report it as
        // the refusal it is instead of an opaque "Too many levels of symbolic links".
        if errno.raw_os_error() == rustix::io::Errno::LOOP.raw_os_error() {
            return io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "refusing to use lock file {} because it is a symbolic link",
                    path.display()
                ),
            );
        }
        io::Error::from(errno)
    })?;
    rustix::fs::flock(&fd, FlockOperation::LockExclusive).map_err(io::Error::from)?;
    Ok(LockGuard { _fd: fd })
}

#[cfg(not(unix))]
pub fn lock_exclusive(_target: &Path) -> io::Result<LockGuard> {
    Ok(LockGuard { _unsupported: () })
}

/// Run `body` while holding the exclusive lock for `target`.
pub fn with_lock<T, E, F>(
    target: &Path,
    on_lock_error: impl FnOnce(io::Error) -> E,
    body: F,
) -> Result<T, E>
where
    F: FnOnce() -> Result<T, E>,
{
    let guard = match lock_exclusive(target) {
        Ok(guard) => guard,
        Err(err) => return Err(on_lock_error(err)),
    };
    let result = body();
    drop(guard);
    result
}

/// Append one already-terminated record to `path` under the file's exclusive lock.
///
/// `O_APPEND` alone keeps individual `write` calls atomic on local filesystems, but the lock is
/// what makes a *record* atomic across processes on the paths where a record is assembled from
/// more than one write, and it is cheap enough to hold unconditionally.
pub fn append_record_locked(path: &Path, record: &[u8]) -> io::Result<()> {
    use std::io::Write;
    create_parent_dir_private(path)?;
    let _guard = lock_exclusive(path)?;
    let mut file = open_append_private(path)?;
    file.write_all(record)?;
    file.flush()
}

/// `true` when `path` exists and is group/other accessible (unix only).
#[cfg(unix)]
pub fn is_group_or_world_accessible(path: &Path) -> io::Result<bool> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::symlink_metadata(path)?.permissions().mode() & 0o777;
    Ok(mode & 0o077 != 0)
}

#[cfg(not(unix))]
pub fn is_group_or_world_accessible(_path: &Path) -> io::Result<bool> {
    Ok(false)
}

/// Resolve a directory-valued environment variable, ignoring values that are empty or
/// relative.
///
/// A relative `XDG_CONFIG_HOME` / `XDG_STATE_HOME` is invalid per the XDG spec ("must be an
/// absolute path"), and honoring one makes the CLI's managed state follow the process's working
/// directory: two agents in different worktrees would silently keep different credentials, and
/// a `cd` between two commands would move the config file. Explicit `EXA_AGENT_*` paths are the
/// caller naming a file on purpose and stay relative-capable.
pub fn absolute_dir_env(name: &str) -> Option<PathBuf> {
    let raw = std::env::var(name).ok()?;
    absolute_dir_value(&raw)
}

fn absolute_dir_value(raw: &str) -> Option<PathBuf> {
    if raw.trim().is_empty() {
        return None;
    }
    let path = PathBuf::from(raw);
    path.is_absolute().then_some(path)
}

/// Resolve an explicitly-named path environment variable (no absoluteness requirement).
pub fn explicit_path_env(name: &str) -> Option<PathBuf> {
    let raw = std::env::var(name).ok()?;
    if raw.trim().is_empty() {
        return None;
    }
    Some(PathBuf::from(raw))
}

/// `$HOME` as a path, when it is set to something non-empty.
pub fn home_dir() -> Option<PathBuf> {
    let raw = std::env::var("HOME").ok()?;
    if raw.trim().is_empty() {
        return None;
    }
    Some(PathBuf::from(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "exa-agent-fsutil-{label}-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn temp_files_beside_the_same_target_are_unique() {
        let dir = scratch("unique");
        let target = dir.join("config.toml");
        let a = TempFile::new_beside(&target).unwrap();
        let b = TempFile::new_beside(&target).unwrap();
        assert_ne!(a.path, b.path, "two temp files must not share a path");
        assert!(a.path.exists() && b.path.exists());
    }

    #[test]
    fn dropped_temp_file_leaves_nothing_behind() {
        let dir = scratch("drop");
        let target = dir.join("config.toml");
        let path = {
            let temp = TempFile::new_beside(&target).unwrap();
            temp.path.clone()
        };
        assert!(!path.exists(), "an unpersisted temp file must be removed");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_is_private_under_a_permissive_umask() {
        let dir = scratch("umask");
        let target = dir.join("credentials.json");
        write_private_atomic(&target, b"{}").unwrap();
        assert!(!is_group_or_world_accessible(&target).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn lock_refuses_a_symlinked_lock_file() {
        let dir = scratch("symlink");
        let target = dir.join("config.toml");
        let planted = dir.join("planted");
        std::fs::write(&planted, b"").unwrap();
        std::os::unix::fs::symlink(&planted, lock_path_for(&target)).unwrap();
        let err = lock_exclusive(&target).expect_err("a symlinked lock file must fail closed");
        assert!(
            err.to_string().contains("symbolic link"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn absolute_directory_values_reject_relative_and_empty_paths() {
        assert_eq!(absolute_dir_value("relative/state"), None);
        assert_eq!(absolute_dir_value(""), None);
        assert_eq!(
            absolute_dir_value("/absolute/state"),
            Some(PathBuf::from("/absolute/state"))
        );
    }

    #[test]
    fn persist_cleans_temp_on_rename_failure() {
        let dir = scratch("rename-failure");
        let target = dir.join("target");
        std::fs::create_dir(&target).unwrap();
        let temp = TempFile::new_beside(&target).unwrap();
        let path = temp.path.clone();
        assert!(temp.persist(&target).is_err());
        assert!(!path.exists());
        assert!(target.is_dir());
        let target = dir.join("valid");
        write_private_atomic(&target, b"saved").unwrap();
        assert_eq!(std::fs::read(target).unwrap(), b"saved");
    }

    #[cfg(unix)]
    #[test]
    fn parent_sync_reports_real_open_errors() {
        let dir = scratch("sync-errors");
        assert!(sync_parent_dir(&dir.join("file")).is_ok());
        let err = sync_parent_dir(&dir.join("missing").join("file")).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        std::fs::write(dir.join("not-a-directory"), b"fixture").unwrap();
        assert!(sync_parent_dir(&dir.join("not-a-directory").join("file")).is_err());
    }
}
