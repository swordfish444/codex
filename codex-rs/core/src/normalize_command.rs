use crate::bash::extract_words_from_command_node;
use crate::bash::find_first_command_node;
use crate::bash::remainder_start_after_wrapper_operator;
use crate::bash::try_parse_bash;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedCommand {
    pub command: Vec<String>,
    pub cwd: PathBuf,
    pub command_for_display: Vec<String>,
}

/// Normalize a shell command for execution and display.
///
/// For classic wrappers like `bash -lc "cd repo && git status"` returns:
/// - command: ["bash", "-lc", "git status"]
/// - cwd: original cwd joined with "repo"
/// - command_for_display: ["git status"]
///
/// Returns None when no normalization is needed.
pub(crate) fn try_normalize_command(command: &[String], cwd: &Path) -> Option<NormalizedCommand> {
    let invocation = parse_shell_script_from_shell_invocation(command)?;
    let shell_script = invocation.command.clone();

    let tree = try_parse_bash(&shell_script)?;

    let first_cmd = find_first_command_node(&tree)?;
    let words = extract_words_from_command_node(first_cmd, &shell_script)?;
    if !is_command_cd_to_directory(&words) {
        return None;
    }

    let dir = &words[1];
    let new_cwd = if Path::new(dir).is_absolute() {
        PathBuf::from(dir)
    } else {
        // TODO(mbolin): Is there anything to worry about if `dir` is a
        // symlink or contains `..` components?
        cwd.join(dir)
    };

    let idx = remainder_start_after_wrapper_operator(first_cmd, &shell_script)?;
    let remainder = shell_script[idx..].to_string();

    Some(NormalizedCommand {
        command: vec![invocation.executable, invocation.flag, remainder.clone()],
        cwd: new_cwd,
        command_for_display: vec![remainder],
    })
}

/// This is similar to [`crate::shell::strip_bash_lc`] and should potentially
/// be unified with it.
fn parse_shell_script_from_shell_invocation(command: &[String]) -> Option<ShellScriptInvocation> {
    // Allowed shells
    fn is_allowed_exe(s: &str) -> bool {
        matches!(s, "bash" | "/bin/bash" | "zsh" | "/bin/zsh")
    }
    match command {
        // exactly three items: <exe> <flag> <command>
        [exe, flag, cmd] if is_allowed_exe(exe) && (flag == "-lc" || flag == "-c") => {
            Some(ShellScriptInvocation {
                executable: exe.clone(),
                flag: flag.clone(),
                command: cmd.clone(),
            })
        }
        // exactly four items: <exe> -l -c <command>
        [exe, flag1, flag2, cmd] if is_allowed_exe(exe) && flag1 == "-l" && flag2 == "-c" => {
            Some(ShellScriptInvocation {
                executable: exe.clone(),
                flag: "-lc".to_string(),
                command: cmd.clone(),
            })
        }
        _ => None,
    }
}

fn is_command_cd_to_directory(command: &[String]) -> bool {
    matches!(command, [first, _dir] if first == "cd" || first == "pushd")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ShellScriptInvocation {
    executable: String,
    /// Single arg, so `-l -c` must be collapsed to `-lc`.
    /// Must be written so `command` follows `flag`, so must
    /// be `-lc` and not `-cl`.
    flag: String,
    command: String,
}

#[cfg(test)]
mod tests {
    use super::NormalizedCommand;
    use super::try_normalize_command;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;

    #[test]
    fn cd_prefix_in_bash_script_is_hidden() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cd foo && echo hi".to_string(),
        ];
        let cwd = PathBuf::from("/baz");
        let norm = try_normalize_command(&command, &cwd);
        assert_eq!(
            norm,
            Some(NormalizedCommand {
                command: vec!["bash".into(), "-lc".into(), "echo hi".into()],
                cwd: PathBuf::from("/baz/foo"),
                command_for_display: vec!["echo hi".into()],
            })
        );
    }

    #[test]
    fn cd_prefix_with_quoted_path_is_hidden() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "  cd 'foo bar' && ls".to_string(),
        ];
        let cwd = PathBuf::from("/baz");
        let norm = try_normalize_command(&command, &cwd);
        assert_eq!(
            norm,
            Some(NormalizedCommand {
                command: vec!["bash".into(), "-lc".into(), "ls".into()],
                cwd: PathBuf::from("/baz/foo bar"),
                command_for_display: vec!["ls".into()],
            })
        );
    }

    #[test]
    fn cd_prefix_with_additional_cd_is_preserved() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cd foo && cd bar && ls".to_string(),
        ];
        let cwd = PathBuf::from("/baz");
        let norm = try_normalize_command(&command, &cwd);
        assert_eq!(
            norm,
            Some(NormalizedCommand {
                command: vec!["bash".into(), "-lc".into(), "cd bar && ls".into()],
                cwd: PathBuf::from("/baz/foo"),
                command_for_display: vec!["cd bar && ls".into()],
            })
        );
    }

    #[test]
    fn cd_prefix_with_or_connector_is_preserved() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cd and_and || echo \"couldn't find the dir for &&\"".to_string(),
        ];
        let cwd = PathBuf::from("/baz");
        let norm = try_normalize_command(&command, &cwd);
        // Not a classic wrapper (uses ||), so no normalization.
        assert_eq!(norm, None);
    }

    #[test]
    fn cd_prefix_preserves_operators_between_remaining_commands() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cd foo && ls && git status".to_string(),
        ];
        let cwd = PathBuf::from("/baz");
        let norm = try_normalize_command(&command, &cwd);
        assert_eq!(
            norm,
            Some(NormalizedCommand {
                command: vec!["bash".into(), "-lc".into(), "ls && git status".into()],
                cwd: PathBuf::from("/baz/foo"),
                command_for_display: vec!["ls && git status".into()],
            })
        );
    }

    #[test]
    fn cd_prefix_preserves_pipelines() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cd foo && ls | rg foo".to_string(),
        ];
        let cwd = PathBuf::from("/baz");
        let norm = try_normalize_command(&command, &cwd);
        assert_eq!(
            norm,
            Some(NormalizedCommand {
                command: vec!["bash".into(), "-lc".into(), "ls | rg foo".into()],
                cwd: PathBuf::from("/baz/foo"),
                command_for_display: vec!["ls | rg foo".into()],
            })
        );
    }

    #[test]
    fn cd_prefix_preserves_sequence_operator() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cd foo && ls; git status".to_string(),
        ];
        let cwd = PathBuf::from("/baz");
        let norm = try_normalize_command(&command, &cwd);
        assert_eq!(
            norm,
            Some(NormalizedCommand {
                command: vec!["bash".into(), "-lc".into(), "ls; git status".into()],
                cwd: PathBuf::from("/baz/foo"),
                command_for_display: vec!["ls; git status".into()],
            })
        );
    }

    #[test]
    fn cd_prefix_with_semicolon_is_hidden() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cd foo; ls".to_string(),
        ];
        let cwd = PathBuf::from("/baz");
        let norm = try_normalize_command(&command, &cwd);
        assert_eq!(
            norm,
            Some(NormalizedCommand {
                command: vec!["bash".into(), "-lc".into(), "ls".into()],
                cwd: PathBuf::from("/baz/foo"),
                command_for_display: vec!["ls".into()],
            })
        );
    }

    #[test]
    fn non_shell_command_is_returned_unmodified() {
        let command = vec!["rg".to_string(), "--files".to_string()];
        let cwd = PathBuf::from("/baz");
        let norm = try_normalize_command(&command, &cwd);
        // Not a shell wrapper, so no normalization.
        assert_eq!(norm, None);
    }

    #[test]
    fn cd_prefix_with_or_operator_is_preserved() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cd missing && ls || echo 'fallback'".to_string(),
        ];
        let cwd = PathBuf::from("/baz");
        let norm = try_normalize_command(&command, &cwd);
        assert_eq!(
            norm,
            Some(NormalizedCommand {
                command: vec!["bash".into(), "-lc".into(), "ls || echo 'fallback'".into()],
                cwd: PathBuf::from("/baz/missing"),
                command_for_display: vec!["ls || echo 'fallback'".into()],
            })
        );
    }

    #[test]
    fn cd_prefix_with_ampersands_in_string_is_hidden() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cd foo && echo \"checking && markers\"".to_string(),
        ];
        let cwd = PathBuf::from("/baz");
        let norm = try_normalize_command(&command, &cwd);
        assert_eq!(
            norm,
            Some(NormalizedCommand {
                command: vec![
                    "bash".into(),
                    "-lc".into(),
                    "echo \"checking && markers\"".into()
                ],
                cwd: PathBuf::from("/baz/foo"),
                command_for_display: vec!["echo \"checking && markers\"".into()],
            })
        );
    }
}
