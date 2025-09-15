use crate::bash::extract_words_from_command_node;
use crate::bash::find_first_command_node;
use crate::bash::remainder_start_after_wrapper_operator;
use crate::bash::try_parse_bash;
use crate::bash::try_parse_word_only_commands_sequence;
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
/// - command_for_display:
///   - If the remainder is a single exec-able command, return it tokenized
///     (e.g., ["rg", "--files"]).
///   - Otherwise, wrap the script as ["bash", "-lc", "<script>"] so that
///     [`crate::parse_command::parse_command`] (which currently checks for
///     bash -lc) can handle it. Even if the original used zsh, we standardize
///     to bash here to match [`crate::parse_command::parse_command`]’s
///     expectations.
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

    let idx = remainder_start_after_wrapper_operator(first_cmd, &shell_script)?;
    let remainder = shell_script[idx..].to_string();

    // For parse_command consumption: if the remainder is a single exec-able
    // command, return it as argv tokens; otherwise, wrap it in ["bash","-lc",..].
    let command_for_display = try_parse_bash(&remainder)
        .and_then(|tree| try_parse_word_only_commands_sequence(&tree, &remainder))
        .and_then(|mut cmds| match cmds.len() {
            1 => cmds.pop(),
            _ => None,
        })
        .unwrap_or_else(|| vec!["bash".to_string(), "-lc".to_string(), remainder.clone()]);

    // Compute new cwd only after confirming the wrapper/operator shape
    let dir = &words[1];
    let new_cwd = if Path::new(dir).is_absolute() {
        PathBuf::from(dir)
    } else {
        // TODO(mbolin): Is there anything to worry about if `dir` is a
        // symlink or contains `..` components?
        cwd.join(dir)
    };

    Some(NormalizedCommand {
        command: vec![invocation.executable, invocation.flag, remainder],
        cwd: new_cwd,
        command_for_display,
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
    matches!(command, [first, _dir] if first == "cd")
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
                command_for_display: vec!["echo".into(), "hi".into()],
            })
        );
    }

    #[test]
    fn cd_shell_var_not_matched() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cd $SOME_DIR && ls".to_string(),
        ];
        let cwd = PathBuf::from("/baz");
        let norm = try_normalize_command(&command, &cwd);
        assert_eq!(
            norm, None,
            "Not a classic wrapper (cd arg is a variable), so no normalization."
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
                command_for_display: vec!["bash".into(), "-lc".into(), "cd bar && ls".into(),],
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
                command_for_display: vec!["bash".into(), "-lc".into(), "ls && git status".into(),],
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
                command_for_display: vec!["bash".into(), "-lc".into(), "ls | rg foo".into(),],
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
                command_for_display: vec!["bash".into(), "-lc".into(), "ls; git status".into(),],
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
    fn pushd_prefix_is_not_normalized() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "pushd foo && ls".to_string(),
        ];
        let cwd = PathBuf::from("/baz");
        // We do not normalize pushd because omitting pushd from the execution
        // would have different semantics than the original command.
        let norm = try_normalize_command(&command, &cwd);
        assert_eq!(norm, None);
    }

    #[test]
    fn supports_zsh_and_bin_bash_and_split_flags() {
        // zsh -lc
        let zsh = vec!["zsh".into(), "-lc".into(), "cd foo && ls".into()];
        let cwd = PathBuf::from("/baz");
        let norm = try_normalize_command(&zsh, &cwd).unwrap();
        assert_eq!(
            norm.command,
            ["zsh", "-lc", "ls"]
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(norm.cwd, PathBuf::from("/baz/foo"));
        assert_eq!(norm.command_for_display, vec!["ls".to_string()]);

        // /bin/bash -l -c <cmd>
        let bash_split = vec![
            "/bin/bash".into(),
            "-l".into(),
            "-c".into(),
            "cd foo && ls".into(),
        ];
        let norm2 = try_normalize_command(&bash_split, &cwd).unwrap();
        assert_eq!(
            norm2.command,
            ["/bin/bash", "-lc", "ls"]
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(norm2.cwd, PathBuf::from("/baz/foo"));
        assert_eq!(norm2.command_for_display, vec!["ls".to_string()]);
    }

    #[test]
    fn supports_dash_c_flag() {
        let command = vec!["bash".into(), "-c".into(), "cd foo && ls".into()];
        let cwd = PathBuf::from("/baz");
        let norm = try_normalize_command(&command, &cwd).unwrap();
        assert_eq!(
            norm.command,
            ["bash", "-c", "ls"]
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(norm.cwd, PathBuf::from("/baz/foo"));
        assert_eq!(norm.command_for_display, vec!["ls".to_string()]);
    }

    #[test]
    fn rejects_pipe_operator_immediately_after_cd() {
        let command = vec!["bash".into(), "-lc".into(), "cd foo | rg bar".into()];
        let cwd = PathBuf::from("/baz");
        let norm = try_normalize_command(&command, &cwd);
        assert_eq!(norm, None);
    }

    #[test]
    fn rejects_unknown_shell_and_bad_flag_order() {
        // Unknown shell
        let unknown = vec!["sh".into(), "-lc".into(), "cd foo && ls".into()];
        let cwd = PathBuf::from("/baz");
        assert_eq!(try_normalize_command(&unknown, &cwd), None);

        // Bad flag order -cl (unsupported)
        let bad_flag = vec!["bash".into(), "-cl".into(), "cd foo && ls".into()];
        assert_eq!(try_normalize_command(&bad_flag, &cwd), None);
    }

    #[test]
    fn absolute_directory_sets_absolute_cwd() {
        let command = vec!["bash".into(), "-lc".into(), "cd /tmp && ls".into()];
        let cwd = PathBuf::from("/baz");
        let norm = try_normalize_command(&command, &cwd).unwrap();
        assert_eq!(norm.cwd, PathBuf::from("/tmp"));
        assert_eq!(
            norm.command,
            ["bash", "-lc", "ls"]
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(norm.command_for_display, vec!["ls".to_string()]);
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
                command_for_display: vec![
                    "bash".into(),
                    "-lc".into(),
                    "ls || echo 'fallback'".into(),
                ],
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
                    "echo \"checking && markers\"".into(),
                ],
                cwd: PathBuf::from("/baz/foo"),
                command_for_display: vec!["echo".into(), "checking && markers".into(),],
            })
        );
    }
}
