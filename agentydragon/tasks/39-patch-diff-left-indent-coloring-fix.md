+++
id = "39"
title = "Fix Coloring of Left-Indented Patch Diffs"
status = "Not started"
dependencies = ""
summary = "Patch diffs rendered with left indentation mode are not colored correctly, losing syntax highlighting."
last_updated = "2025-06-25T00:00:00Z"
+++

# Task 39: Fix Coloring of Left-Indented Patch Diffs

> *UI bug:* When patch diffs are rendered in left-indented mode, the ANSI color codes are misaligned, resulting in lost or incorrect coloring.

## Status

**General Status**: Not started  
**Summary**: Diagnose offset logic in diff renderer and adjust color processing to account for indentation.

## Goal

Ensure diff lines maintain proper ANSI color highlighting even when indented on the left by a fixed margin.

## Acceptance Criteria

- Diff render tests pass for both default and indented modes.
- Visual manual check confirms colored diff alignment.

## Implementation

- Update diff renderer to strip indentation before applying color logic, then reapply indentation.
- Add unit tests for multiline indented diffs.

## Notes

<!-- Any implementation notes -->
