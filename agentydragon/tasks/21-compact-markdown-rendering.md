---
id: 21
title: Compact Markdown Rendering Option
status: Not started  # one of: Not started, Started, Needs manual review, Done, Cancelled
summary: Provide an option to render Markdown without blank lines between headings and content for more vertical packing.
goal: |
  Add a configuration flag to control Markdown rendering in the chat UI and logs so that headings render immediately adjacent to their content with no separating blank line.

## Acceptance Criteria

- Introduce a config flag `markdown_compact = true|false` under the UI settings.
- When enabled, the renderer omits the default blank line between headings (lines starting with `#`) and their subsequent content.
- The flag applies globally to all Markdown rendering (diffs, docs, help messages).
- Default behavior remains unchanged (blank lines preserved) when `markdown_compact` is false or unset.
- Add tests to verify both compact and default rendering modes across heading levels.

## Implementation

**How it was implemented**  
- Extend the Markdown-to-TUI formatter to check `markdown_compact` and collapse heading/content spacing.
- Implement a post-processing step that removes blank lines immediately following heading tokens (`^#{1,6} `) when `markdown_compact` is true.
- Expose the new flag via the config parser and default it to `false`.
- Add unit tests covering H1–H6 headings, verifying absence of blank line in compact mode and presence in default mode.

## Notes

- This option improves vertical density for screens with limited height.
- Ensure compatibility with existing Markdown features like lists and code blocks; only target heading-content spacing.
