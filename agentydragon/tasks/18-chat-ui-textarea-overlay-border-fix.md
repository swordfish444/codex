+++
id = "18"
title = "Chat UI Textarea Overlay and Border Styling Fix"
status = "Not started"
dependencies = ""
last_updated = "2025-06-25T01:40:09.514379"
+++

# Task 18: Chat UI Textarea Overlay and Border Styling Fix

---
id: 18
title: Chat UI Textarea Overlay and Border Styling Fix
status: Not started
summary: Fix overlay of waiting messages and streamline borders between chat window and input area to improve visibility and reclaim terminal space.
goal: |
  Adjust the TUI chat interface so that waiting/status messages no longer overlay the first line of the input textarea (ensuring user drafts remain visible), and merge/remove borders as follows:
    - Merge the bottom border of the chat history window with the top border of the input textarea.
    - Remove the left, right, and bottom overall borders around the chat interface to reduce wasted space.
---

> *This task is specific to codex-rs.*

## Acceptance Criteria

- Waiting/status messages (e.g. "Thinking...", "Typing...", etc.) appear above the textarea rather than overlaying the first line of the input area.
- User draft text remains visible at all times, even when agent messages or status indicators are rendered.
- The bottom border of the chat history pane and the top border of the textarea are unified into a single border line.
- The left, right, and bottom borders around the entire chat UI are removed, reclaiming columns/rows in the terminal.
- Manual or automated visual verification steps demonstrate correct layout in a variety of terminal widths.

## Implementation

**How it was implemented**  
*(Not implemented yet)*

**How it works**  
*(Not implemented yet)*

## Notes

- This involves updating the rendering logic in the TUI modules (likely under `tui/src/` in `codex-rs`).
- Ensure layout changes do not break existing tests or rendering in unusual terminal sizes.
- Consider writing a simple snapshot test or manual demo script to validate border and overlay behavior.
