# Task Template

# Valid status values: Not started | Started | Done | Cancelled

---
id: <NN>
title: <Task Title>
status: Not started  # one of: Not started, Started, Done, Cancelled
summary: Brief summary of current status.
goal: |
  Describe the objective of the task here.
---

> *This task is specific to codex-rs.*

## Acceptance Criteria

List measurable criteria for completion.

## Implementation

**How it was implemented**  
Provide details on code modules, design decisions, and steps taken.

**How it works**  
Explain runtime behavior and overall operation.

## Notes

Any additional notes or references.

---
Run the frontmatter linter to ensure conformance:
```bash
python3 ../tools/check_task_frontmatter.py
```