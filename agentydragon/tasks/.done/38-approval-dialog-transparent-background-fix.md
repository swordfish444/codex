+++
id = "38"
title = "Fix Approval Dialog Transparent Background"
status = "Done"
dependencies = ""
summary = "The approval dialog background is transparent, causing prompt text underneath to overlap and become unreadable."
last_updated = "2025-06-25T23:00:00.000000"
+++

> *UI bug:* When the approval dialog appears, its background is transparent and any partially entered prompt text shows through, overlapping and confusing the dialog.

## Status

**General Status**: Done  
**Summary**: Identify and implement an opaque background for the approval dialog to prevent underlying text bleed-through.

## Goal

Ensure the approval dialog is drawn with a solid background color (matching the dialog border or theming) so that any underlying text does not bleed through.

## Acceptance Criteria

- Approval dialogs block underlying prompt text (solid background).
- Existing unit/integration tests validate dialog visual rendering.

## Implementation

- Updated `render_ref` in `codex-rs/tui/src/user_approval_widget.rs` to fill the entire dialog area with a `DarkGray` background before drawing the border and content.
- Implemented nested loops over the dialog `Rect` calling `buf[(col, row)].set_bg(Color::DarkGray)` on each cell.
- Added unit test `render_approval_dialog_fills_background` in `tui/src/user_approval_widget.rs` to render the widget onto a buffer pre-filled with a red background and verify no cell in the dialog region remains transparent or retains the sentinel background.

## Notes

<!-- Any implementation notes -->
