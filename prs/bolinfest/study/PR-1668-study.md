**DOs**
- **Parse With tree-sitter-bash**: Use `try_parse_bash` + `try_parse_word_only_commands_sequence` to vet `bash -lc "..."` scripts, then validate each extracted command against `is_safe_to_call_with_exec`.
```rust
use codex_core::bash::{try_parse_bash, try_parse_word_only_commands_sequence};

let src = "ls | wc -l";
let tree = try_parse_bash(src).expect("parse bash");
let cmds = try_parse_word_only_commands_sequence(&tree, src).expect("only plain commands");
assert!(cmds.iter().all(|c| is_safe_to_call_with_exec(c)));
```
- **Allow Only Safe Operators**: Accept sequences joined by `&&`, `||`, `;`, `|` when every simple command is safe.
```rust
assert!(is_known_safe_command(&vec!["bash".into(), "-lc".into(), r#"grep -R "Cargo.toml" -n || true"#.into()]));
assert!(is_known_safe_command(&vec!["bash".into(), "-lc".into(), "ls && pwd".into()]));
assert!(is_known_safe_command(&vec!["bash".into(), "-lc".into(), "echo 'hi' ; ls".into()]));
assert!(is_known_safe_command(&vec!["bash".into(), "-lc".into(), "ls | wc -l".into()]));
```
- **Accept Only “Plain” Words**: Permit bare words, numbers, and simple quoted strings (no interpolation).
```rust
assert!(is_known_safe_command(&vec!["bash".into(), "-lc".into(), r#"echo "hello world""#.into()]));
assert!(is_known_safe_command(&vec!["bash".into(), "-lc".into(), "echo 'hi there'".into()]));
assert!(is_known_safe_command(&vec!["bash".into(), "-lc".into(), "echo 123 456".into()]));
```
- **Require Every Command To Be Safe**: If any command in the sequence is unsafe, reject the whole script.
```rust
assert!(!is_known_safe_command(&vec!["bash".into(), "-lc".into(), "ls && rm -rf /".into()]));
```
- **Keep Helpers In `core::bash`**: Centralize parsing helpers and call them from `is_known_safe_command`.
```rust
if let [bash, flag, script] = &command[..] {
  if bash == "bash" && flag == "-lc" {
    if let Some(tree) = try_parse_bash(script) {
      if let Some(cmds) = try_parse_word_only_commands_sequence(&tree, script) {
        if cmds.iter().all(|c| is_safe_to_call_with_exec(c)) { return true; }
      }
    }
  }
}
```
- **Match On Node Kinds Via Strings**: Treat `node.kind()` as an external string API; use tight allowlists.
```rust
const ALLOWED_KINDS: &[&str] = &[
  "program","list","pipeline","command","command_name",
  "word","string","string_content","raw_string","number",
];
const ALLOWED_PUNCT: &[&str] = &["&&","||",";","|","\"","'"];
```
- **Fail Closed On Parse Errors**: If the tree has errors or unexpected nodes/tokens, return `None` and reject.
```rust
assert!(!is_known_safe_command(&vec!["bash".into(), "-lc".into(), "ls &&".into()]));
```

**DON’Ts**
- **No Subshells/Grouping**: Reject parentheses and similar grouping; subshells aren’t supported yet.
```rust
assert!(!is_known_safe_command(&vec!["bash".into(), "-lc".into(), "(ls)".into()]));
assert!(!is_known_safe_command(&vec!["bash".into(), "-lc".into(), "ls || (pwd && echo hi)".into()]));
```
- **No Redirections/Backgrounding**: Disallow `>`, `<`, `>>`, `2>`, `&`, etc.
```rust
assert!(!is_known_safe_command(&vec!["bash".into(), "-lc".into(), "ls > out.txt".into()]));
```
- **No Substitutions Or Expansions**: Disallow `$()`, backticks, `$VAR`, or interpolation inside strings.
```rust
assert!(!is_known_safe_command(&vec!["bash".into(), "-lc".into(), "echo $(pwd)".into()]));
assert!(!is_known_safe_command(&vec!["bash".into(), "-lc".into(), "echo `pwd`".into()]));
assert!(!is_known_safe_command(&vec!["bash".into(), "-lc".into(), "echo $HOME".into()]));
assert!(!is_known_safe_command(&vec!["bash".into(), "-lc".into(), r#"echo "hi $USER""#.into()]));
```
- **No Assignment Prefixes**: Reject `FOO=bar cmd` forms.
```rust
assert!(!is_known_safe_command(&vec!["bash".into(), "-lc".into(), "FOO=bar ls".into()]));
```
- **Don’t “Sanitize” Unsafe Commands With Safe Operators**: `&&`, `||`, `;`, `|` don’t make unsafe commands safe.
```rust
assert!(!is_known_safe_command(&vec!["bash".into(), "-lc".into(), "find . -name file.txt -delete".into()]));
assert!(!is_known_safe_command(&vec!["bash".into(), "-lc".into(), "true || rm -rf /".into()]));
```
- **Don’t Depend On Extraction Order**: The order of extracted `command` nodes is not semantically meaningful; always validate all of them.
- **Don’t Loosen Allowlists Without Tests**: Any expansion of accepted nodes/operators must come with targeted tests for both allowed and rejected cases.