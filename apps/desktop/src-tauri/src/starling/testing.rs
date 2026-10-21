// Scratch-directory helpers for the starling tests: a fake `starling`
// script on disk, the way starling's own test/magpie.test.mjs fakes
// magpie, so the real resolution and subprocess code is what runs.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "magpie-starling-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        // Canonical so paths compare equal on macOS, where /tmp is a link.
        Self(dir.canonicalize().unwrap())
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn join(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Writes `body` as an executable `#!/bin/sh` script at `path`, creating
/// parent directories.
#[cfg(unix)]
pub fn script(path: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.to_path_buf()
}

/// A fake `starling` that answers `ls --json` and `streams --json` with
/// the given text (printed verbatim), or exits 1 when both are `None`.
#[cfg(unix)]
pub fn fake_starling(dir: &Path, ls: Option<&str>, streams: Option<&str>) -> PathBuf {
    std::fs::write(dir.join("ls.json"), ls.unwrap_or("")).unwrap();
    std::fs::write(dir.join("streams.json"), streams.unwrap_or("")).unwrap();
    let fail = if ls.is_none() && streams.is_none() {
        "exit 1"
    } else {
        ""
    };
    script(
        &dir.join("starling"),
        &format!(
            "{fail}\ncase \"$1\" in ls) cat \"{d}/ls.json\";; streams) cat \"{d}/streams.json\";; esac",
            d = dir.display()
        ),
    )
}
