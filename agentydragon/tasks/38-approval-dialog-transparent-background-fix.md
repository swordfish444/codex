+++
id = "38"
title = "Fix Approval Dialog Transparent Background"
status = "Not started"
dependencies = ""
summary = "The approval dialog background is transparent, causing prompt text underneath to overlap and become unreadable."
last_updated = "2025-06-25T00:00:00Z"
+++

> *UI bug:* When the approval dialog appears, its background is transparent and any partially entered prompt text shows through, overlapping and confusing the dialog.

## Status

**General Status**: Not started  
**Summary**: Identify the CSS/ANSI background settings for dialogs and enforce an opaque backdrop before text rendering.

## Goal

Ensure the approval dialog is drawn with a solid background color (matching the dialog border or theming) so that any underlying text does not bleed through.

## Acceptance Criteria

- Approval dialogs block underlying prompt text (solid background).
- Existing unit/integration tests validate dialog visual rendering.

## Implementation

- Update dialog background color in TUI rendering code (`.tui` component).
- Add a test to verify no transparent cells in dialog region.

## Notes

<!-- Any implementation notes -->
