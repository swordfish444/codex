use crate::bash::try_parse_bash;
use crate::bash::try_parse_word_only_commands_sequence;
use serde::Deserialize;
use serde::Serialize;
use shlex::split as shlex_split;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum ParsedCommand {
    Read {
        cmd: Vec<String>,
        name: String,
    },
    Python {
        cmd: Vec<String>,
    },
    GitStatus {
        cmd: Vec<String>,
    },
    GitLog {
        cmd: Vec<String>,
    },
    GitDiff {
        cmd: Vec<String>,
    },
    Ls {
        cmd: Vec<String>,
        path: Option<String>,
    },
    Rg {
        cmd: Vec<String>,
        query: Option<String>,
        path: Option<String>,
        files_only: bool,
    },
    Shell {
        cmd: Vec<String>,
        display: String,
    },
    Pnpm {
        cmd: Vec<String>,
        pnpm_cmd: String,
    },
    Unknown {
        cmd: Vec<String>,
    },
}

pub fn parse_command(command: &[String]) -> Vec<ParsedCommand> {
    let main_cmd = extract_main_cmd_tokens(command);

    // 1) Try the "bash -lc <script>" path: leverage the existing parser so we
    //    can get each sub-command (words-only) precisely.
    if let [bash, flag, script] = command {
        if bash == "bash" && flag == "-lc" {
            if let Some(tree) = try_parse_bash(script) {
                if let Some(all_commands) = try_parse_word_only_commands_sequence(&tree, script) {
                    if !all_commands.is_empty() {
                        // Tokenize the entire script once; used to preserve full context for certain summaries.
                        let script_tokens = shlex_split(script).unwrap_or_else(|| {
                            vec!["bash".to_string(), flag.clone(), script.clone()]
                        });
                        let commands: Vec<ParsedCommand> = all_commands
                            .into_iter()
                            .map(|tokens| {
                                match summarize_main_tokens(&tokens) {
                                    // For ls within a bash -lc script, preserve the full script tokens for display.
                                    ParsedCommand::Ls { path, .. } => ParsedCommand::Ls {
                                        cmd: script_tokens.clone(),
                                        path,
                                    },
                                    other => other,
                                }
                            })
                            .collect();

                        return commands;
                    }
                }
            }

            // If we couldn't parse with the bash parser, conservatively treat the
            // whole thing as one opaque shell command and mark unsafe.
            let display = script.clone();
            let commands = vec![ParsedCommand::Shell {
                cmd: main_cmd.clone(),
                display,
            }];
            return commands;
        }
    }

    // 2) Not a "bash -lc" form. If there are connectors, split locally.
    let has_connectors = main_cmd
        .iter()
        .any(|t| t == "&&" || t == "||" || t == "|" || t == ";");

    let split_subcommands = |tokens: &[String]| -> Vec<Vec<String>> {
        let mut out: Vec<Vec<String>> = Vec::new();
        let mut cur: Vec<String> = Vec::new();
        for t in tokens {
            if t == "&&" || t == "||" || t == "|" || t == ";" {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            } else {
                cur.push(t.clone());
            }
        }
        if !cur.is_empty() {
            out.push(cur);
        }
        out
    };

    let commands_tokens: Vec<Vec<String>> = if has_connectors {
        split_subcommands(&main_cmd)
    } else {
        vec![main_cmd.clone()]
    };

    // 3) Summarize each sub-command.
    let commands: Vec<ParsedCommand> = commands_tokens
        .into_iter()
        .map(|tokens| summarize_main_tokens(&tokens))
        .collect();

    commands
}

/// Returns true if `arg` matches /^(\d+,)?\d+p$/
fn is_valid_sed_n_arg(arg: Option<&str>) -> bool {
    let s = match arg {
        Some(s) => s,
        None => return false,
    };
    let core = match s.strip_suffix('p') {
        Some(rest) => rest,
        None => return false,
    };
    let parts: Vec<&str> = core.split(',').collect();
    match parts.as_slice() {
        [num] => !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()),
        [a, b] => {
            !a.is_empty()
                && !b.is_empty()
                && a.chars().all(|c| c.is_ascii_digit())
                && b.chars().all(|c| c.is_ascii_digit())
        }
        _ => false,
    }
}

fn extract_main_cmd_tokens(cmd: &[String]) -> Vec<String> {
    match cmd {
        [first, pipe, rest @ ..] if (first == "yes" || first == "y") && pipe == "|" => {
            let s = rest.join(" ");
            shlex_split(&s).unwrap_or_else(|| rest.to_vec())
        }
        [first, pipe, rest @ ..] if (first == "no" || first == "n") && pipe == "|" => {
            let s = rest.join(" ");
            shlex_split(&s).unwrap_or_else(|| rest.to_vec())
        }
        [bash, flag, script] if bash == "bash" && (flag == "-c" || flag == "-lc") => {
            shlex_split(script)
                .unwrap_or_else(|| vec!["bash".to_string(), flag.clone(), script.clone()])
        }
        _ => cmd.to_vec(),
    }
}

fn summarize_main_tokens(main_cmd: &[String]) -> ParsedCommand {
    let cut_at_connector = |tokens: &[String]| -> Vec<String> {
        let idx = tokens
            .iter()
            .position(|t| t == "|" || t == "&&" || t == "||")
            .unwrap_or(tokens.len());
        tokens[..idx].to_vec()
    };

    let truncate_file_path_for_display = |path: &str| -> String {
        let mut parts = path.split('/').rev().filter(|p| {
            !p.is_empty() && *p != "build" && *p != "dist" && *p != "node_modules" && *p != "src"
        });
        parts
            .next()
            .map(|s| s.to_string())
            .unwrap_or_else(|| path.to_string())
    };

    match main_cmd.split_first() {
        Some((head, tail)) if head == "ls" => {
            let path = tail
                .iter()
                .find(|p| !p.starts_with('-'))
                .map(|p| truncate_file_path_for_display(p));
            ParsedCommand::Ls {
                cmd: main_cmd.to_vec(),
                path,
            }
        }
        Some((head, tail)) if head == "rg" => {
            let args_no_connector = cut_at_connector(tail);
            let files_only = args_no_connector.iter().any(|a| a == "--files");
            let non_flags: Vec<&String> = args_no_connector
                .iter()
                .filter(|p| !p.starts_with('-'))
                .collect();
            let (query, path) = if files_only {
                let p = non_flags.first().map(|s| truncate_file_path_for_display(s));
                (None, p)
            } else {
                let q = non_flags.first().map(|s| truncate_file_path_for_display(s));
                let p = non_flags.get(1).map(|s| truncate_file_path_for_display(s));
                (q, p)
            };
            ParsedCommand::Rg {
                cmd: main_cmd.to_vec(),
                query,
                path,
                files_only,
            }
        }
        Some((head, tail)) if head == "grep" => {
            let args_no_connector = cut_at_connector(tail);
            let non_flags: Vec<&String> = args_no_connector
                .iter()
                .filter(|p| !p.starts_with('-'))
                .collect();
            let query = non_flags.first().map(|s| truncate_file_path_for_display(s));
            let path = non_flags.get(1).map(|s| truncate_file_path_for_display(s));
            ParsedCommand::Rg {
                cmd: main_cmd.to_vec(),
                query,
                path,
                files_only: false,
            }
        }
        Some((head, tail)) if head == "cat" && tail.len() == 1 => {
            let name = truncate_file_path_for_display(&tail[0]);
            ParsedCommand::Read {
                cmd: main_cmd.to_vec(),
                name,
            }
        }
        Some((head, tail))
            if head == "head"
                && tail.len() >= 3
                && tail[0] == "-n"
                && tail[1].chars().all(|c| c.is_ascii_digit()) =>
        {
            let name = truncate_file_path_for_display(&tail[2]);
            ParsedCommand::Read {
                cmd: main_cmd.to_vec(),
                name,
            }
        }
        Some((head, tail))
            if head == "tail" && tail.len() >= 3 && tail[0] == "-n" && {
                let n = &tail[1];
                let s = n.strip_prefix('+').unwrap_or(n);
                !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
            } =>
        {
            let name = truncate_file_path_for_display(&tail[2]);
            ParsedCommand::Read {
                cmd: main_cmd.to_vec(),
                name,
            }
        }
        Some((head, tail))
            if head == "sed"
                && tail.len() >= 3
                && tail[0] == "-n"
                && is_valid_sed_n_arg(tail.get(1).map(|s| s.as_str())) =>
        {
            if let Some(path) = tail.get(2) {
                let name = truncate_file_path_for_display(path);
                ParsedCommand::Read {
                    cmd: main_cmd.to_vec(),
                    name,
                }
            } else {
                ParsedCommand::Unknown {
                    cmd: main_cmd.to_vec(),
                }
            }
        }
        Some((head, _tail)) if head == "python" => ParsedCommand::Python {
            cmd: main_cmd.to_vec(),
        },
        Some((first, rest)) if first == "git" => match rest.first().map(|s| s.as_str()) {
            Some("status") => ParsedCommand::GitStatus {
                cmd: main_cmd.to_vec(),
            },
            Some("log") => ParsedCommand::GitLog {
                cmd: main_cmd.to_vec(),
            },
            Some("diff") => ParsedCommand::GitDiff {
                cmd: main_cmd.to_vec(),
            },
            _ => ParsedCommand::Unknown {
                cmd: main_cmd.to_vec(),
            },
        },
        Some((tool, rest)) if (tool == "pnpm" || tool == "npm") => {
            let mut r = rest;
            let mut has_r = false;
            if let Some(flag) = r.first() {
                if flag == "-r" {
                    has_r = true;
                    r = &r[1..];
                }
            }
            if r.first().map(|s| s.as_str()) == Some("run") {
                let args = r[1..].to_vec();
                // For display, only include the script name before any "--" forwarded args.
                let script_name = args.first().cloned().unwrap_or_default();
                let pnpm_cmd = script_name;
                let mut full = vec![tool.clone()];
                if has_r {
                    full.push("-r".to_string());
                }
                full.push("run".to_string());
                full.extend(args.clone());
                ParsedCommand::Pnpm {
                    cmd: full,
                    pnpm_cmd,
                }
            } else {
                ParsedCommand::Unknown {
                    cmd: main_cmd.to_vec(),
                }
            }
        }
        _ => ParsedCommand::Unknown {
            cmd: main_cmd.to_vec(),
        },
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn vec_str(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn git_status_summary() {
        let out = parse_command(&vec_str(&["git", "status"]));
        assert_eq!(
            out,
            vec![ParsedCommand::GitStatus {
                cmd: vec_str(&["git", "status"]),
            }]
        );
    }

    #[test]
    fn handles_complex_bash_command() {
        let inner =
            "rg --version && node -v && pnpm -v && rg --files | wc -l && rg --files | head -n 40";
        let out = parse_command(&vec_str(&["bash", "-lc", inner]));
        assert_eq!(
            out,
            vec![
                ParsedCommand::Unknown {
                    cmd: vec_str(&["head", "-n", "40"])
                },
                ParsedCommand::Rg {
                    cmd: vec_str(&["rg", "--files"]),
                    query: None,
                    path: None,
                    files_only: true,
                },
                ParsedCommand::Unknown {
                    cmd: vec_str(&["wc", "-l"])
                },
                ParsedCommand::Rg {
                    cmd: vec_str(&["rg", "--files"]),
                    query: None,
                    path: None,
                    files_only: true,
                },
                ParsedCommand::Unknown {
                    cmd: vec_str(&["pnpm", "-v"])
                },
                ParsedCommand::Unknown {
                    cmd: vec_str(&["node", "-v"])
                },
                ParsedCommand::Rg {
                    cmd: vec_str(&["rg", "--version"]),
                    query: None,
                    path: None,
                    files_only: false,
                },
            ]
        );
    }

    #[test]
    fn supports_searching_for_navigate_to_route() {
        let inner = "rg -n \"navigate-to-route\" -S";
        let out = parse_command(&vec_str(&["bash", "-lc", inner]));
        assert_eq!(
            out,
            vec![ParsedCommand::Rg {
                cmd: shlex_split(inner).unwrap(),
                query: Some("navigate-to-route".to_string()),
                path: None,
                files_only: false,
            }]
        );
    }

    #[test]
    fn supports_rg_files_with_path_and_pipe() {
        let inner = "rg --files webview/src | sed -n";
        let out = parse_command(&vec_str(&["bash", "-lc", inner]));
        assert_eq!(
            out,
            vec![
                ParsedCommand::Unknown {
                    cmd: vec_str(&["sed", "-n"])
                },
                ParsedCommand::Rg {
                    cmd: vec_str(&["rg", "--files", "webview/src"]),
                    query: None,
                    path: Some("webview".to_string()),
                    files_only: true,
                },
            ]
        );
    }

    #[test]
    fn supports_rg_files_then_head() {
        let inner = "rg --files | head -n 50";
        let out = parse_command(&vec_str(&["bash", "-lc", inner]));
        assert_eq!(
            out,
            vec![
                ParsedCommand::Unknown {
                    cmd: vec_str(&["head", "-n", "50"])
                },
                ParsedCommand::Rg {
                    cmd: vec_str(&["rg", "--files"]),
                    query: None,
                    path: None,
                    files_only: true,
                },
            ]
        );
    }

    #[test]
    fn supports_cat() {
        let inner = "cat webview/README.md";
        let out = parse_command(&vec_str(&["bash", "-lc", inner]));
        assert_eq!(
            out,
            vec![ParsedCommand::Read {
                cmd: shlex_split(inner).unwrap(),
                name: "README.md".to_string(),
            }]
        );
    }

    #[test]
    fn supports_ls_with_pipe() {
        let inner = "ls -la | sed -n '1,120p'";
        let out = parse_command(&vec_str(&["bash", "-lc", inner]));
        assert_eq!(
            out,
            vec![
                ParsedCommand::Unknown {
                    cmd: vec_str(&["sed", "-n", "1,120p"])
                },
                ParsedCommand::Ls {
                    cmd: shlex_split(inner).unwrap(),
                    path: None,
                },
            ]
        );
    }

    #[test]
    fn supports_head_n() {
        let inner = "head -n 50 Cargo.toml";
        let out = parse_command(&vec_str(&["bash", "-lc", inner]));
        assert_eq!(
            out,
            vec![ParsedCommand::Read {
                cmd: shlex_split(inner).unwrap(),
                name: "Cargo.toml".to_string(),
            },]
        );
    }

    #[test]
    fn supports_tail_n_plus() {
        let inner = "tail -n +522 README.md";
        let out = parse_command(&vec_str(&["bash", "-lc", inner]));
        assert_eq!(
            out,
            vec![ParsedCommand::Read {
                cmd: shlex_split(inner).unwrap(),
                name: "README.md".to_string(),
            }]
        );
    }

    #[test]
    fn supports_tail_n_last_lines() {
        let inner = "tail -n 30 README.md";
        let out = parse_command(&vec_str(&["bash", "-lc", inner]));
        assert_eq!(
            out,
            vec![ParsedCommand::Read {
                cmd: shlex_split(inner).unwrap(),
                name: "README.md".to_string(),
            }]
        );
    }

    #[test]
    fn supports_npm_run_build() {
        let out = parse_command(&vec_str(&["npm", "run", "build"]));
        assert_eq!(
            out,
            vec![ParsedCommand::Pnpm {
                cmd: vec_str(&["npm", "run", "build"]),
                pnpm_cmd: "build".to_string(),
            }]
        );
    }

    #[test]
    fn supports_npm_run_with_forwarded_args() {
        let out = parse_command(&vec_str(&[
            "npm",
            "run",
            "lint",
            "--",
            "--max-warnings",
            "0",
            "--format",
            "json",
        ]));
        assert_eq!(
            out,
            vec![ParsedCommand::Pnpm {
                cmd: vec_str(&[
                    "npm",
                    "run",
                    "lint",
                    "--",
                    "--max-warnings",
                    "0",
                    "--format",
                    "json",
                ]),
                pnpm_cmd: "lint".to_string(),
            }]
        );
    }

    #[test]
    fn supports_grep_recursive_current_dir() {
        let out = parse_command(&vec_str(&[
            "grep",
            "-R",
            "CODEX_SANDBOX_ENV_VAR",
            "-n",
            ".",
        ]));
        assert_eq!(
            out,
            vec![ParsedCommand::Rg {
                cmd: vec_str(&["grep", "-R", "CODEX_SANDBOX_ENV_VAR", "-n", "."]),
                query: Some("CODEX_SANDBOX_ENV_VAR".to_string()),
                path: Some(".".to_string()),
                files_only: false,
            }]
        );
    }

    #[test]
    fn supports_grep_recursive_specific_file() {
        let out = parse_command(&vec_str(&[
            "grep",
            "-R",
            "CODEX_SANDBOX_ENV_VAR",
            "-n",
            "core/src/spawn.rs",
        ]));
        assert_eq!(
            out,
            vec![ParsedCommand::Rg {
                cmd: vec_str(&[
                    "grep",
                    "-R",
                    "CODEX_SANDBOX_ENV_VAR",
                    "-n",
                    "core/src/spawn.rs",
                ]),
                query: Some("CODEX_SANDBOX_ENV_VAR".to_string()),
                path: Some("spawn.rs".to_string()),
                files_only: false,
            }]
        );
    }

    #[test]
    fn supports_grep_weird_backtick_in_query() {
        let out = parse_command(&vec_str(&["grep", "-R", "COD`EX_SANDBOX", "-n"]));
        assert_eq!(
            out,
            vec![ParsedCommand::Rg {
                cmd: vec_str(&["grep", "-R", "COD`EX_SANDBOX", "-n"]),
                query: Some("COD`EX_SANDBOX".to_string()),
                path: None,
                files_only: false,
            }]
        );
    }

    #[test]
    fn supports_cd_and_rg_files() {
        let out = parse_command(&vec_str(&["cd", "codex-rs", "&&", "rg", "--files"]));
        assert_eq!(
            out,
            vec![
                ParsedCommand::Unknown {
                    cmd: vec_str(&["cd", "codex-rs"]),
                },
                ParsedCommand::Rg {
                    cmd: vec_str(&["rg", "--files"]),
                    query: None,
                    path: None,
                    files_only: true,
                },
            ]
        );
    }
}
