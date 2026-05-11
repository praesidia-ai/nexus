//! Canonical mini-agent implementations (see
//! `docs/NEXUS_MASTER_PLAN.md` §2).
//!
//! Each submodule here implements exactly one
//! [`nexus_agents_core::mini::MiniAgent`]. Implementations are
//! intentionally tiny — everything about retry / budget / caching
//! belongs in the `SwarmConductor`, not here.
//!
//! # Registry
//!
//! [`build_registry`] returns the default `MiniRegistry` wired into
//! `SwarmConductor` at boot. Additional mini-agents can be registered
//! at runtime via plugins but the v1.0 canonical set lives here.
//!
//! # File-system safety
//!
//! All filesystem operations go through [`canonical_child`] which
//! rejects any path that escapes the project root. Mini-agents never
//! touch paths outside `project_root`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use nexus_agents_core::mini::{MiniAgent, MiniKind};

use crate::coding_agents::swarm::MiniRegistry;

pub mod fs_locator;
pub mod fs_patcher;
pub mod fs_reader;
pub mod shell_runner;
pub mod test_writer;
pub mod web_fetcher;

/// Build the default mini-agent registry. Scoped to a project root so
/// filesystem mini-agents can't escape their sandbox.
pub fn build_registry(project_root: PathBuf) -> MiniRegistry {
    let mut m: HashMap<MiniKind, Arc<dyn MiniAgent>> = HashMap::new();
    m.insert(
        MiniKind::FsLocator,
        Arc::new(fs_locator::FsLocator::new(project_root.clone())),
    );
    m.insert(
        MiniKind::FsReader,
        Arc::new(fs_reader::FsReader::new(project_root.clone())),
    );
    m.insert(
        MiniKind::FsPatcher,
        Arc::new(fs_patcher::FsPatcher::new(project_root.clone())),
    );
    m.insert(
        MiniKind::TestWriter,
        Arc::new(test_writer::TestWriter::new(project_root.clone())),
    );
    m.insert(
        MiniKind::WebFetcher,
        Arc::new(web_fetcher::WebFetcher::new()),
    );
    m.insert(
        MiniKind::ShellRunner,
        Arc::new(shell_runner::ShellRunner::new(project_root)),
    );
    Arc::new(m)
}

/// Resolve `candidate` against `root` and refuse to cross the root.
///
/// This is the single point every filesystem-touching mini-agent must
/// go through. Matches the pattern used in
/// `handlers/app_runner.rs::read_file` and keeps the sandbox invariant
/// testable in one place.
pub(crate) fn canonical_child(
    root: &Path,
    candidate: &str,
) -> Result<PathBuf, String> {
    let rel = Path::new(candidate);
    if rel.is_absolute() {
        return Err(format!("absolute paths not allowed: {candidate}"));
    }

    let root_canonical = root
        .canonicalize()
        .map_err(|e| format!("canonicalise root: {e}"))?;
    let joined = root_canonical.join(rel);

    // Walk up from `joined` to find the nearest ancestor that actually
    // exists on disk (so we can canonicalise it) and treat the rest of
    // the path as suffix-to-append. This handles "new-file.rs",
    // "tests/new_test.rs" under a tmp root where `tests/` doesn't
    // exist yet, and "../../evil" which resolves outside the root.
    let mut existing = joined.as_path();
    let mut suffix: PathBuf = PathBuf::new();
    let resolved_root = loop {
        match existing.canonicalize() {
            Ok(p) => break p,
            Err(_) => {
                let tail = existing
                    .file_name()
                    .ok_or_else(|| format!("cannot resolve {candidate}"))?;
                // Prepend `tail` onto `suffix`. We can't use
                // `PathBuf::push` on an empty PathBuf because it
                // inserts a trailing empty component that turns
                // `out.txt` into `out.txt/` and confuses later
                // syscalls.
                let mut new_suffix = PathBuf::from(tail);
                if suffix.as_os_str().is_empty() {
                    suffix = new_suffix;
                } else {
                    new_suffix.push(&suffix);
                    suffix = new_suffix;
                }
                existing = existing
                    .parent()
                    .ok_or_else(|| format!("cannot resolve {candidate}"))?;
            }
        }
    };
    // `resolved_root.join(Path::new(""))` on Unix appends a trailing
    // slash which then trips filesystem syscalls ("not a directory").
    // Only append a suffix if one actually accumulated.
    let canonical = if suffix.as_os_str().is_empty() {
        resolved_root
    } else {
        resolved_root.join(suffix)
    };

    if !canonical.starts_with(&root_canonical) {
        return Err(format!("path escapes project root: {candidate}"));
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn canonical_child_rejects_absolute_paths() {
        let dir = tempdir().unwrap();
        let err = canonical_child(dir.path(), "/etc/passwd").unwrap_err();
        assert!(err.contains("absolute"));
    }

    #[test]
    fn canonical_child_rejects_escape() {
        let dir = tempdir().unwrap();
        let err = canonical_child(dir.path(), "../../etc/passwd").unwrap_err();
        assert!(err.contains("escapes"));
    }

    #[test]
    fn canonical_child_accepts_nested_existing_file() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("sub");
        std::fs::create_dir_all(&nested).unwrap();
        let f = nested.join("a.txt");
        std::fs::write(&f, "hi").unwrap();
        let got = canonical_child(dir.path(), "sub/a.txt").unwrap();
        assert_eq!(got, f.canonicalize().unwrap());
    }

    #[test]
    fn canonical_child_accepts_nonexistent_but_valid_path() {
        let dir = tempdir().unwrap();
        let got = canonical_child(dir.path(), "new-file.rs").unwrap();
        assert!(got.ends_with("new-file.rs"));
    }
}
