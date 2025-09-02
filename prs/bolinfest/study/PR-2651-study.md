**DOs**
- **Allow Empty Lines**: Permit empty content in added/changed lines by using `(.*)` instead of `(.+)` in the Lark grammar.
```lark
add_line: "+" /(.*)/ LF        -> line
change_line: ("+" | "-" | " ") /(.*)/ LF
```

- **Keep Grammar In Its Own File**: Store the grammar in a `.lark` file and include it from Rust with `include_str!`.
```rust
// codex-rs/core/src/tool_apply_patch.rs
const APPLY_PATCH_LARK_GRAMMAR: &str = include_str!("tool_apply_patch.lark");

definition: APPLY_PATCH_LARK_GRAMMAR.to_string(),
```
```lark
# codex-rs/core/src/tool_apply_patch.lark
start: begin_patch hunk+ end_patch
begin_patch: "*** Begin Patch" LF
end_patch: "*** End Patch" LF?
hunk: add_hunk | delete_hunk | update_hunk
add_hunk: "*** Add File: " filename LF add_line+
delete_hunk: "*** Delete File: " filename LF
update_hunk: "*** Update File: " filename LF change_move? change?
filename: /(.+)/
add_line: "+" /(.*)/ LF                 -> line
change_move: "*** Move to: " filename LF
change: (change_context | change_line)+ eof_line?
change_context: ("@@" | "@@ " /(.+)/) LF
change_line: ("+" | "-" | " ") /(.*)/ LF
eof_line: "*** End of File" LF
%import common.LF
```

- **Add A Minimal Parser Test**: If Rust crates are lacking, write a small Python/Lark smoke test that parses an empty-line patch.
```python
from lark import Lark

grammar = open("codex-rs/core/src/tool_apply_patch.lark").read()
parser = Lark(grammar, start="start")

patch = """*** Begin Patch
*** Add File: a.txt
+
*** End Patch
"""
parser.parse(patch)  # succeeds if empty added line is allowed
```

- **Rebase To Drop Unrelated Changes**: Keep the PR focused on grammar; remove stray diffs like config toggles.
```bash
git fetch origin
git rebase origin/main
git restore --source=origin/main codex-rs/core/src/config.rs
git add -p
git rebase --continue
```

- **Mirror Parser Behavior In Tests**: Include cases for added, removed, and context empty lines to prevent regressions.
```python
cases = ["+\n", "-\n", " \n"]
for line in cases:
    parser.parse(f"""*** Begin Patch
*** Update File: a.txt
@@
{line}*** End Patch
""")
```

**DON’Ts**
- **Don’t Require Non‑Empty Content**: Avoid `(.+)` which rejects valid empty lines.
```lark
# WRONG
add_line: "+" /(.+)/ LF
change_line: ("+" | "-" | " ") /(.+)/ LF
```

- **Don’t Inline Huge Grammar Strings In Rust**: Skip embedding long raw strings; externalize and `include_str!` instead.
```rust
// WRONG
definition: r#"
start: begin_patch hunk+ end_patch
...
"#.to_string();
```

- **Don’t Mix Unrelated Diffs**: Exclude config or feature toggles from a parser-only PR.
```diff
-     pub tools: Option<ToolsToml>,
+     pub tools: Option<ToolsToml>,
+     /// Include an experimental plan tool...
+     pub include_plan_tool: Option<bool>,   // Unrelated to grammar fix
```

- **Don’t Skip Parser Tests**: Changes to grammars are fragile; always add at least one smoke test.
```bash
# At minimum, run a parser over a sample patch containing empty lines
pytest -q  # or run the Python script shown above
```

- **Don’t Use `include!()` For Text Grammars**: `include!` expects Rust code; use `include_str!` for data files.
```rust
// WRONG
const GRAMMAR: &str = include!("tool_apply_patch.lark");
// RIGHT
const GRAMMAR: &str = include_str!("tool_apply_patch.lark");
```