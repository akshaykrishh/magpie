use std::process::Command;

/// Where this MCP server was spawned -- an agent host (Claude Code, Cursor)
/// launches the server with its own working directory, so this needs zero
/// configuration to scope itself correctly (see docs/design.md "Projects
/// and multi-session"). Falls back to `name: None` (Inbox scope) outside a
/// git repo, or if `git` itself isn't on PATH.
#[derive(Debug, Clone, Default)]
pub struct DetectedProject {
    pub name: Option<String>,
    pub remote_url: Option<String>,
    pub common_git_dir: Option<String>,
    pub branch: Option<String>,
}

pub fn detect() -> DetectedProject {
    let remote_url = git(&["remote", "get-url", "origin"]);
    // `--git-common-dir` on git < 2.31 returns a path relative to the
    // current directory rather than absolute (the `--path-format` flag
    // that fixes this is a newer addition); resolve it ourselves so the
    // result is stable regardless of git version or cwd.
    let common_git_dir = git(&["rev-parse", "--git-common-dir"]).and_then(|p| {
        let path = std::path::Path::new(&p);
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().ok()?.join(path)
        };
        absolute
            .canonicalize()
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
    });
    let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"]).filter(|b| b != "HEAD");

    let name = remote_url
        .as_deref()
        .and_then(project_name_from_remote)
        .or_else(|| {
            common_git_dir
                .as_deref()
                .and_then(project_name_from_common_git_dir)
        });

    DetectedProject {
        name,
        remote_url,
        common_git_dir,
        branch,
    }
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// "git@github.com:x/magpie.git" or "https://github.com/x/magpie.git" -> "magpie"
fn project_name_from_remote(remote_url: &str) -> Option<String> {
    let trimmed = remote_url.trim_end_matches(".git").trim_end_matches('/');
    trimmed
        .rsplit(['/', ':'])
        .next()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// ".../magpie/.git" -> "magpie"
fn project_name_from_common_git_dir(common_git_dir: &str) -> Option<String> {
    let without_dotgit = common_git_dir
        .trim_end_matches("/.git")
        .trim_end_matches(".git");
    without_dotgit
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_name_from_ssh_remote() {
        assert_eq!(
            project_name_from_remote("git@github.com:akshaykrishh/magpie.git"),
            Some("magpie".to_string())
        );
    }

    #[test]
    fn extracts_name_from_https_remote() {
        assert_eq!(
            project_name_from_remote("https://github.com/akshaykrishh/magpie.git"),
            Some("magpie".to_string())
        );
    }

    #[test]
    fn extracts_name_from_common_git_dir() {
        assert_eq!(
            project_name_from_common_git_dir("/Users/me/code/magpie/.git"),
            Some("magpie".to_string())
        );
    }

    #[test]
    fn detect_runs_without_panicking_outside_a_repo() {
        // Can't assert a specific value -- depends on whether this test
        // process happens to run inside a git checkout. The point is that
        // shelling out to `git` and parsing its output never panics,
        // regardless of context.
        let _ = detect();
    }
}
