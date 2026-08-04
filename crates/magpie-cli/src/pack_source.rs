use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use magpie_core::{ParsedPack, ParsedPrompt};
use serde::Deserialize;

/// The manifest at a pack's root, `magpie.json`. Deliberately narrow: a
/// pack is a curated list of prompts, not a general-purpose config format.
#[derive(Debug, Deserialize)]
struct Manifest {
    name: String,
    description: Option<String>,
    prompts: Vec<ManifestPrompt>,
}

#[derive(Debug, Deserialize)]
struct ManifestPrompt {
    title: String,
    description: Option<String>,
    body: String,
    #[serde(default)]
    variables: HashMap<String, ManifestVariable>,
}

#[derive(Debug, Deserialize, serde::Serialize)]
struct ManifestVariable {
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default: Option<String>,
}

/// Resolves `source` to a `(source_url, pack directory)` and reads its
/// manifest. `source_url` is what gets stored on the `packs` row and
/// matched against on re-import -- for git sources it's the resolved
/// clone URL; for a local path (the only way to test this without a
/// published pack) it's the canonicalized absolute path, so re-running
/// against the same directory still updates in place rather than
/// duplicating.
pub fn fetch(source: &str) -> Result<(String, ParsedPack)> {
    if let Some(local) = as_local_path(source) {
        let dir = local
            .canonicalize()
            .with_context(|| format!("no such local pack path: {}", local.display()))?;
        let manifest = read_manifest(&dir)?;
        return Ok((dir.to_string_lossy().into_owned(), parsed_pack(manifest)));
    }

    let url = as_git_url(source);
    validate_clone_url(&url)?;
    let tmp = tempfile::tempdir().context("failed to create a temp directory for the clone")?;
    let status = Command::new("git")
        // Defense-in-depth alongside `validate_clone_url`: even if a
        // future edit loosened that check, git itself refuses any
        // transport outside this list. `https`/`ssh` are the only two
        // `validate_clone_url` ever allows through.
        .env("GIT_ALLOW_PROTOCOL", "https:ssh")
        .args(["clone", "--depth", "1", &url])
        .arg(tmp.path())
        .status()
        .with_context(|| format!("failed to run `git clone {url}`"))?;
    if !status.success() {
        bail!("git clone of {url} failed");
    }
    let manifest = read_manifest(tmp.path())?;
    Ok((url, parsed_pack(manifest)))
}

fn read_manifest(dir: &Path) -> Result<Manifest> {
    // Accepting either a directory (containing magpie.json) or a direct
    // path straight to the manifest file is what makes `magpie pack add
    // ./my-pack/magpie.json` and `magpie pack add ./my-pack` both work --
    // a small convenience, not a second format.
    let manifest_path = if dir.is_file() {
        dir.to_path_buf()
    } else {
        dir.join("magpie.json")
    };
    let text = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("no magpie.json found at {}", manifest_path.display()))?;
    serde_json::from_str(&text)
        .with_context(|| format!("{} is not a valid pack manifest", manifest_path.display()))
}

fn parsed_pack(manifest: Manifest) -> ParsedPack {
    ParsedPack {
        name: manifest.name,
        description: manifest.description,
        prompts: manifest
            .prompts
            .into_iter()
            .map(|p| ParsedPrompt {
                title: p.title,
                description: p.description,
                body: p.body,
                variables_json: if p.variables.is_empty() {
                    None
                } else {
                    serde_json::to_string(&p.variables).ok()
                },
            })
            .collect(),
    }
}

/// `github:owner/repo` -> a full clone URL; anything else that isn't a
/// local path is passed through as-is (a plain `https://` or `git@` URL
/// already works with `git clone` directly).
fn as_git_url(source: &str) -> String {
    match source.strip_prefix("github:") {
        Some(rest) => format!("https://github.com/{rest}.git"),
        None => source.to_string(),
    }
}

/// `source` is untrusted by design -- `magpie pack add` exists to install
/// packs other people wrote and shared (`github:someone/agent-prompts`, a
/// link pasted from a README or chat), and whatever it resolves to gets
/// passed straight through to `git clone`'s argv. Git treats a clone URL
/// as a transport dispatch, not merely an endpoint: `ext::<command>` runs
/// an arbitrary shell command as the "transport" (allowed by git's
/// default `protocol.ext.allow=user`), and a string starting with `-`
/// (e.g. `--upload-pack=...`) can be parsed as a clone option instead of
/// a repository. Requiring an exact `https://`/`git@` prefix closes both
/// at once -- neither an `ext::` payload nor a leading `-` can ever match.
fn validate_clone_url(url: &str) -> Result<()> {
    if url.starts_with("https://") || url.starts_with("git@") {
        return Ok(());
    }
    bail!(
        "refusing to clone {url:?}: pack sources must be an https:// or git@ URL \
         (or an existing local path)"
    );
}

/// A source is treated as a local path only when it's an existing
/// directory or file on disk -- not merely "doesn't start with github: or
/// a URL scheme", since that would make a typo in a real pack name (e.g. a
/// bare "owner/repo" without the github: prefix) fail as a confusing
/// missing-file error rather than a clearer "not a valid source" one from
/// the git clone attempt.
fn as_local_path(source: &str) -> Option<PathBuf> {
    if source.starts_with("github:") || source.contains("://") || source.starts_with("git@") {
        return None;
    }
    let path = PathBuf::from(source);
    path.exists().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_shorthand_expands_to_a_clone_url() {
        assert_eq!(
            as_git_url("github:someone/agent-prompts"),
            "https://github.com/someone/agent-prompts.git"
        );
    }

    #[test]
    fn a_real_url_passes_through_unchanged() {
        assert_eq!(
            as_git_url("https://example.com/pack.git"),
            "https://example.com/pack.git"
        );
    }

    #[test]
    fn github_shorthand_is_never_treated_as_a_local_path() {
        assert_eq!(as_local_path("github:someone/agent-prompts"), None);
    }

    #[test]
    fn a_url_is_never_treated_as_a_local_path() {
        assert_eq!(as_local_path("https://example.com/pack.git"), None);
        assert_eq!(
            as_local_path("git@github.com:someone/agent-prompts.git"),
            None
        );
    }

    #[test]
    fn a_nonexistent_local_looking_path_is_not_treated_as_local() {
        // Otherwise a typo'd path silently falls through to being treated
        // as a git URL, which fails with a clearer error than a bare
        // "file not found" would.
        assert_eq!(as_local_path("./definitely-does-not-exist"), None);
    }

    #[test]
    fn fetch_reads_a_manifest_from_a_local_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("magpie.json"),
            r#"{
                "name": "Test Pack",
                "description": "For tests",
                "prompts": [
                    {
                        "title": "Fix a test",
                        "body": "Investigate why {{test_name}} fails",
                        "variables": {"test_name": {"description": "which test"}}
                    },
                    {
                        "title": "No variables",
                        "body": "Just do the thing"
                    }
                ]
            }"#,
        )
        .unwrap();

        let (source_url, pack) = fetch(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(
            source_url,
            dir.path().canonicalize().unwrap().to_string_lossy()
        );
        assert_eq!(pack.name, "Test Pack");
        assert_eq!(pack.prompts.len(), 2);
        assert_eq!(pack.prompts[0].body, "Investigate why {{test_name}} fails");
        assert!(pack.prompts[0].variables_json.is_some());
        assert!(pack.prompts[1].variables_json.is_none());
    }

    #[test]
    fn fetch_reads_a_manifest_given_directly_as_a_file_path() {
        let dir = tempfile::tempdir().unwrap();
        let manifest_path = dir.path().join("magpie.json");
        std::fs::write(&manifest_path, r#"{"name": "P", "prompts": []}"#).unwrap();

        let (_, pack) = fetch(manifest_path.to_str().unwrap()).unwrap();
        assert_eq!(pack.name, "P");
    }

    #[test]
    fn fetch_errors_clearly_on_missing_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let err = fetch(dir.path().to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("magpie.json"));
    }

    // Regression test for a real vulnerability: `fetch()` used to pass
    // `source` through to `git clone`'s argv completely unvalidated once it
    // failed the "is this an existing local path" check. `ext::<command>`
    // makes git run an arbitrary shell command as its own "transport" --
    // `magpie pack add "ext::touch /tmp/pwned"` was full command execution
    // as whoever ran it, with no confirmation step, for a command whose
    // entire purpose is installing content someone else wrote and shared.
    #[test]
    fn fetch_refuses_an_ext_transport_source() {
        let err = fetch("ext::sh -c 'touch /tmp/magpie-pack-source-poc'").unwrap_err();
        assert!(err.to_string().contains("https:// or git@"));
    }

    #[test]
    fn fetch_refuses_a_leading_dash_source() {
        // Would otherwise be parsed by git as an option (e.g.
        // `--upload-pack=...`) rather than a repository.
        let err = fetch("--upload-pack=touch /tmp/magpie-pack-source-poc").unwrap_err();
        assert!(err.to_string().contains("https:// or git@"));
    }

    #[test]
    fn fetch_refuses_a_file_scheme_source() {
        let err = fetch("file:///etc").unwrap_err();
        assert!(err.to_string().contains("https:// or git@"));
    }

    #[test]
    fn validate_clone_url_accepts_https_and_ssh_shorthand() {
        assert!(validate_clone_url("https://example.com/pack.git").is_ok());
        assert!(validate_clone_url("git@github.com:someone/agent-prompts.git").is_ok());
    }

    #[test]
    fn validate_clone_url_rejects_everything_else() {
        assert!(validate_clone_url("ext::sh -c 'touch /tmp/pwned'").is_err());
        assert!(validate_clone_url("file:///etc/passwd").is_err());
        assert!(validate_clone_url("--upload-pack=x").is_err());
        assert!(validate_clone_url("ssh://git@example.com/pack.git").is_err());
    }

    #[test]
    fn fetch_errors_clearly_on_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("magpie.json"), "not json").unwrap();
        let err = fetch(dir.path().to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("not a valid pack manifest"));
    }
}
