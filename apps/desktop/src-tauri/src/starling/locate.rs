// Finding the starling binary from a GUI app.
//
// An app opened from Finder or the Dock does not inherit the login shell's
// PATH: it gets launchd's short default, which has neither Homebrew nor an
// npm global installed under nvm -- where starling usually lives. So PATH
// alone would report "not installed" on exactly the machines that have it.
// Resolution therefore falls through four sources, first hit wins.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use super::run;

const BINARY: &str = "starling";

/// An explicit choice, and the only source that is never second-guessed:
/// if it is set, it is the answer or there is no answer.
pub const OVERRIDE_VAR: &str = "STARLING_BIN";

/// A login shell can source a lot of rc files before it answers; this is
/// how long that is allowed to take before moving on to the fixed
/// locations.
const SHELL_TIMEOUT: Duration = Duration::from_millis(1500);

/// Where each source looks, gathered up front so tests can point every one
/// of them at a scratch directory instead of the machine they run on.
pub struct Locator {
    pub override_bin: Option<OsString>,
    pub path: Option<OsString>,
    pub shell: Option<PathBuf>,
    pub home: Option<PathBuf>,
    /// Directories searched after the shell, besides `~/.nvm/versions/node/*/bin`.
    pub fixed_dirs: Vec<PathBuf>,
    pub shell_timeout: Duration,
}

impl Locator {
    pub fn from_process() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let mut fixed_dirs = vec![
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
        ];
        if let Some(home) = &home {
            fixed_dirs.push(home.join(".local/bin"));
            fixed_dirs.push(home.join(".volta/bin"));
        }
        Self {
            override_bin: std::env::var_os(OVERRIDE_VAR),
            path: std::env::var_os("PATH"),
            shell: std::env::var_os("SHELL").map(PathBuf::from),
            home,
            fixed_dirs,
            shell_timeout: SHELL_TIMEOUT,
        }
    }

    /// Blocks for up to `SHELL_TIMEOUT` (the login-shell step): background
    /// threads only.
    pub fn locate(&self) -> Option<PathBuf> {
        if let Some(over) = self.override_bin.as_ref().filter(|v| !v.is_empty()) {
            let path = PathBuf::from(over);
            return is_executable(&path).then_some(path);
        }
        self.on_path()
            .or_else(|| self.via_login_shell())
            .or_else(|| self.in_fixed_dirs())
    }

    fn on_path(&self) -> Option<PathBuf> {
        let path = self.path.as_ref()?;
        std::env::split_paths(path).find_map(|dir| executable_in(&dir))
    }

    fn via_login_shell(&self) -> Option<PathBuf> {
        let shell = self.shell.as_ref().filter(|s| s.is_absolute())?;
        let mut command = Command::new(shell);
        command.args(["-lc", &format!("command -v {BINARY}")]);
        let answer = run::run(command, self.shell_timeout)?;
        // rc files may print banners before the answer, and `command -v`
        // prints a bare word for an alias or function; only the last line
        // that is an absolute path to something runnable counts.
        answer
            .stdout
            .lines()
            .rev()
            .map(str::trim)
            .filter(|line| line.starts_with('/'))
            .map(PathBuf::from)
            .find(|p| is_executable(p))
    }

    fn in_fixed_dirs(&self) -> Option<PathBuf> {
        let mut dirs = self.fixed_dirs.clone();
        if let Some(home) = &self.home {
            dirs.extend(nvm_bin_dirs(home));
        }
        dirs.iter().find_map(|dir| executable_in(dir))
    }
}

/// `~/.nvm/versions/node/*/bin`, newest version first: that is the one a
/// `npm i -g` most plausibly landed in.
fn nvm_bin_dirs(home: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(home.join(".nvm/versions/node")) else {
        return Vec::new();
    };
    let mut versions: Vec<(Vec<u64>, PathBuf)> = entries
        .flatten()
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let numbers = name
                .trim_start_matches('v')
                .split('.')
                .map(|part| part.parse().unwrap_or(0))
                .collect();
            (numbers, e.path().join("bin"))
        })
        .collect();
    versions.sort_by(|a, b| b.0.cmp(&a.0));
    versions.into_iter().map(|(_, dir)| dir).collect()
}

fn executable_in(dir: &Path) -> Option<PathBuf> {
    let candidate = dir.join(BINARY);
    is_executable(&candidate).then_some(candidate)
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::starling::testing::{script, TempDir};

    fn bare(home: &Path) -> Locator {
        Locator {
            override_bin: None,
            path: None,
            shell: None,
            home: Some(home.to_path_buf()),
            fixed_dirs: Vec::new(),
            shell_timeout: Duration::from_secs(20),
        }
    }

    fn install(dir: &Path) -> PathBuf {
        script(&dir.join("starling"), "exit 0")
    }

    #[test]
    fn nothing_found_is_none() {
        let tmp = TempDir::new();
        assert_eq!(bare(tmp.path()).locate(), None);
    }

    #[test]
    fn the_override_wins_over_everything() {
        let tmp = TempDir::new();
        let chosen = install(&tmp.join("chosen"));
        let on_path = install(&tmp.join("on-path"));
        let locator = Locator {
            override_bin: Some(chosen.clone().into_os_string()),
            path: Some(on_path.parent().unwrap().into()),
            ..bare(tmp.path())
        };
        assert_eq!(locator.locate(), Some(chosen));
    }

    #[test]
    fn a_bad_override_is_not_second_guessed() {
        let tmp = TempDir::new();
        let on_path = install(&tmp.join("on-path"));
        let locator = Locator {
            override_bin: Some(tmp.join("missing").into_os_string()),
            path: Some(on_path.parent().unwrap().into()),
            ..bare(tmp.path())
        };
        assert_eq!(locator.locate(), None);
    }

    #[test]
    fn path_is_searched_in_order() {
        let tmp = TempDir::new();
        let first = install(&tmp.join("a"));
        install(&tmp.join("b"));
        let path = std::env::join_paths([tmp.join("empty"), tmp.join("a"), tmp.join("b")]).unwrap();
        let locator = Locator {
            path: Some(path),
            ..bare(tmp.path())
        };
        assert_eq!(locator.locate(), Some(first));
    }

    #[test]
    fn a_file_that_is_not_executable_is_skipped() {
        let tmp = TempDir::new();
        std::fs::create_dir_all(tmp.join("bin")).unwrap();
        std::fs::write(tmp.join("bin/starling"), "not a program").unwrap();
        let locator = Locator {
            path: Some(tmp.join("bin").into_os_string()),
            ..bare(tmp.path())
        };
        assert_eq!(locator.locate(), None);
    }

    #[test]
    fn the_login_shell_finds_what_path_cannot() {
        let tmp = TempDir::new();
        let real = install(&tmp.join("nvm-ish"));
        // A login shell that prints rc-file noise and an alias-style bare
        // word before the answer: only the absolute path may be taken.
        let shell = script(
            &tmp.join("fake-shell"),
            &format!(
                "echo 'welcome back'\necho starling\necho {}",
                real.display()
            ),
        );
        let locator = Locator {
            shell: Some(shell),
            ..bare(tmp.path())
        };
        assert_eq!(locator.locate(), Some(real));
    }

    #[test]
    fn a_shell_that_hangs_falls_through_to_the_fixed_locations() {
        let tmp = TempDir::new();
        let real = install(&tmp.join("fixed"));
        let shell = script(&tmp.join("hung-shell"), "exec sleep 30");
        let locator = Locator {
            shell: Some(shell),
            fixed_dirs: vec![tmp.join("fixed")],
            shell_timeout: Duration::from_millis(300),
            ..bare(tmp.path())
        };
        let started = std::time::Instant::now();
        assert_eq!(locator.locate(), Some(real));
        assert!(started.elapsed() < Duration::from_secs(4));
    }

    #[test]
    fn the_newest_nvm_node_is_searched_first() {
        let tmp = TempDir::new();
        install(&tmp.join(".nvm/versions/node/v9.11.2/bin"));
        install(&tmp.join(".nvm/versions/node/v18.20.0/bin"));
        let newest = install(&tmp.join(".nvm/versions/node/v22.3.0/bin"));
        assert_eq!(bare(tmp.path()).locate(), Some(newest));
    }

    #[test]
    fn nvm_is_only_a_fallback_after_the_fixed_directories() {
        let tmp = TempDir::new();
        install(&tmp.join(".nvm/versions/node/v22.3.0/bin"));
        let fixed = install(&tmp.join("opt-bin"));
        let locator = Locator {
            fixed_dirs: vec![tmp.join("opt-bin")],
            ..bare(tmp.path())
        };
        assert_eq!(locator.locate(), Some(fixed));
    }
}
