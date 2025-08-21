use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use rand::Rng;

const INITIAL_DELAY_MS: u64 = 200;
const BACKOFF_FACTOR: f64 = 2.0;

pub(crate) fn backoff(attempt: u64) -> Duration {
    let exp = BACKOFF_FACTOR.powi(attempt.saturating_sub(1) as i32);
    let base = (INITIAL_DELAY_MS as f64 * exp) as u64;
    let jitter = rand::rng().random_range(0.9..1.1);
    Duration::from_millis((base as f64 * jitter) as u64)
}

/// Return `true` if the project folder specified by the `Config` is inside a
/// Git repository.
///
/// The check walks up the directory hierarchy looking for a `.git` file or
/// directory (note `.git` can be a file that contains a `gitdir` entry). This
/// approach does **not** require the `git` binary or the `git2` crate and is
/// therefore fairly lightweight.
///
/// Note that this does **not** detect *work‑trees* created with
/// `git worktree add` where the checkout lives outside the main repository
/// directory. If you need Codex to work from such a checkout simply pass the
/// `--allow-no-git-exec` CLI flag that disables the repo requirement.
pub fn is_inside_git_repo(base_dir: &Path) -> bool {
    let mut dir = base_dir.to_path_buf();

    loop {
        if dir.join(".git").exists() {
            return true;
        }

        // Pop one component (go up one directory).  `pop` returns false when
        // we have reached the filesystem root.
        if !dir.pop() {
            break;
        }
    }

    false
}

/// Try to resolve the main git repository root for `base_dir`.
///
/// - For a normal repo (where `.git` is a directory), returns the directory
///   that contains the `.git` directory.
/// - For a worktree (where `.git` is a file with a `gitdir:` pointer), reads
///   the referenced git directory and, if present, its `commondir` file to
///   locate the common `.git` directory of the main repository. Returns the
///   parent of that common directory.
/// - Returns `None` when no enclosing repo is found.
pub fn git_main_repo_root(base_dir: &Path) -> Option<PathBuf> {
    // Walk up from base_dir to find the first ancestor containing a `.git` entry.
    let mut dir = base_dir.to_path_buf();
    loop {
        let dot_git = dir.join(".git");
        if dot_git.is_dir() {
            // Standard repository. The repo root is the directory containing `.git`.
            return Some(dir);
        } else if dot_git.is_file() {
            // Worktree case: `.git` is a file like: `gitdir: /path/to/worktrees/<name>`
            if let Ok(contents) = fs::read_to_string(&dot_git) {
                // Extract the path after `gitdir:` and trim whitespace.
                let gitdir_prefix = "gitdir:";
                let line = contents
                    .lines()
                    .find(|l| l.trim_start().starts_with(gitdir_prefix));
                if let Some(line) = line {
                    let path_part = line.split_once(':').map(|(_, r)| r.trim());
                    if let Some(gitdir_str) = path_part {
                        // Resolve relative paths against the directory containing `.git` (the worktree root).
                        let gitdir_path = Path::new(gitdir_str);
                        let gitdir_abs = if gitdir_path.is_absolute() {
                            gitdir_path.to_path_buf()
                        } else {
                            dir.join(gitdir_path)
                        };

                        // In worktrees, the per-worktree gitdir typically contains a `commondir`
                        // file that points (possibly relatively) to the common `.git` directory.
                        let commondir_path = gitdir_abs.join("commondir");
                        if let Ok(common_dir_rel) = fs::read_to_string(&commondir_path) {
                            let common_dir_rel = common_dir_rel.trim();
                            let common_dir_path = Path::new(common_dir_rel);
                            let common_dir_abs = if common_dir_path.is_absolute() {
                                common_dir_path.to_path_buf()
                            } else {
                                gitdir_abs.join(common_dir_path)
                            };
                            // The main repo root is the parent of the common `.git` directory.
                            if let Some(parent) = common_dir_abs.parent() {
                                return Some(parent.to_path_buf());
                            }
                        } else {
                            // Fallback: if no commondir file, use the parent of `gitdir_abs` if it looks like a `.git` dir.
                            if let Some(parent) = gitdir_abs.parent() {
                                return Some(parent.to_path_buf());
                            }
                        }
                    }
                }
            }
            // If parsing fails, continue the walk upwards in case of nested repos (rare).
        }

        if !dir.pop() {
            break;
        }
    }

    None
}

/// Normalize a path for trust configuration lookups.
///
/// If inside a git repo, returns the main repository root; otherwise returns the
// canonicalized `base_dir` (or `base_dir` if canonicalization fails).
pub fn normalized_trust_project_root(base_dir: &Path) -> PathBuf {
    if let Some(repo_root) = git_main_repo_root(base_dir) {
        return repo_root.canonicalize().unwrap_or(repo_root);
    }
    base_dir
        .canonicalize()
        .unwrap_or_else(|_| base_dir.to_path_buf())
}
