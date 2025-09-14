use crate::bash::extract_words_from_command_node;
use crate::bash::find_first_command_node;
use crate::bash::remainder_start_after_and;
use crate::bash::try_parse_bash;

/// If one exists, returns a copy of `command` that reads more naturally in logs
/// and error messages.
///
/// When the command is a classic shell wrapper such as `bash -lc "cd repo &&
/// git status"`, the returned value contains only the prettified script (with
/// the leading `cd`/`pushd` removed) and excludes the `bash -lc` wrapper.
/// Any other command returns `None`.
pub fn prettify_command_for_display(command: &[String]) -> Option<Vec<String>> {
    let shell_script = parse_shell_script_from_shell_invocation(command)?;

    let tree = try_parse_bash(&shell_script)?;

    // Find the earliest command node in source order.
    let first_cmd = find_first_command_node(&tree)?;

    // Verify the first command is `cd <dir>` or `pushd <dir>` (exactly 2 words).
    let words = extract_words_from_command_node(first_cmd, &shell_script)?;
    if !is_command_cd_to_directory(&words) {
        return None;
    }

    // Determine textual remainder using sibling tokens in the parse tree.
    let idx = remainder_start_after_and(first_cmd, &shell_script)?;
    let remainder = shell_script[idx..].to_string();
    Some(vec![remainder])
}

/// This is similar to [`crate::shell::strip_bash_lc`] and should potentially
/// be unified with it.
fn parse_shell_script_from_shell_invocation(command: &[String]) -> Option<String> {
    match command {
        // exactly three items
        [first, second, third]
            // first two must be "bash", "-lc"
            if first == "bash" && second == "-lc" =>
        {
            Some(third.clone())
        }
        _ => None,
    }
}

fn is_command_cd_to_directory(command: &[String]) -> bool {
    matches!(command, [first, _dir] if first == "cd" || first == "pushd")
}

// Helper moved to crate::bash

#[cfg(test)]
mod tests {
    use super::prettify_command_for_display;
    use pretty_assertions::assert_eq;

    #[test]
    fn cd_prefix_in_bash_script_is_hidden() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cd foo && echo hi".to_string(),
        ];
        let display = prettify_command_for_display(&command);
        assert_eq!(display, Some(vec!["echo hi".to_string()]));
    }

    #[test]
    fn cd_prefix_with_quoted_path_is_hidden() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "  cd 'foo bar' && ls".to_string(),
        ];
        let display = prettify_command_for_display(&command);
        assert_eq!(display, Some(vec!["ls".to_string()]));
    }

    #[test]
    fn cd_prefix_with_additional_cd_is_preserved() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cd foo && cd bar && ls".to_string(),
        ];
        let display = prettify_command_for_display(&command);
        assert_eq!(display, Some(vec!["cd bar && ls".to_string()]));
    }

    #[test]
    fn cd_prefix_with_or_connector_is_preserved() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cd and_and || echo \"couldn't find the dir for &&\"".to_string(),
        ];
        let display = prettify_command_for_display(&command);
        // Not a classic wrapper (uses ||), so no prettified form.
        assert_eq!(display, None);
    }

    #[test]
    fn cd_prefix_preserves_operators_between_remaining_commands() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cd foo && ls && git status".to_string(),
        ];
        let display = prettify_command_for_display(&command);
        assert_eq!(display, Some(vec!["ls && git status".to_string()]));
    }

    #[test]
    fn cd_prefix_preserves_pipelines() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cd foo && ls | rg foo".to_string(),
        ];
        let display = prettify_command_for_display(&command);
        assert_eq!(display, Some(vec!["ls | rg foo".to_string()]));
    }

    #[test]
    fn cd_prefix_preserves_sequence_operator() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cd foo && ls; git status".to_string(),
        ];
        let display = prettify_command_for_display(&command);
        assert_eq!(display, Some(vec!["ls; git status".to_string()]));
    }

    #[test]
    fn non_shell_command_is_returned_unmodified() {
        let command = vec!["rg".to_string(), "--files".to_string()];
        let display = prettify_command_for_display(&command);
        // Not a shell wrapper, so no prettified form.
        assert_eq!(display, None);
    }

    #[test]
    fn cd_prefix_with_or_operator_is_preserved() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cd missing && ls || echo 'fallback'".to_string(),
        ];
        let display = prettify_command_for_display(&command);
        assert_eq!(display, Some(vec!["ls || echo 'fallback'".to_string()]));
    }

    #[test]
    fn cd_prefix_with_ampersands_in_string_is_hidden() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cd foo && echo \"checking && markers\"".to_string(),
        ];
        let display = prettify_command_for_display(&command);
        assert_eq!(
            display,
            Some(vec!["echo \"checking && markers\"".to_string()])
        );
    }
}
