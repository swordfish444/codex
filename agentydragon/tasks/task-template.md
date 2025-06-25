+++
id = "<NN>"
title = "<Task Title>"
status = "<<<!!! MANAGER: SET VALID STATUS  - Not started? !!!>>>"
freeform_status = "<<<!!! MANAGER/DEVELOPER: Freeform status text, optional. E.g. progress notes or developer comments. !!!>>>"
dependencies = [<<<!!! MANAGER: LIST TASK IDS THAT MUST BE COMPLETED BEFORE STARTING; SEPARATED BY COMMAS, E.G. "02","05" !!!>>>] # <!-- Manager rationale: explain why these dependencies are required and why other tasks are not. -->
last_updated = "<timestamp in ISO format>"
+++

# Task Template

# Valid status values: Not started | In progress | Needs input | Needs manual review | Done | Cancelled | Merged


> *This task is specific to codex-rs.*

## Acceptance Criteria

List measurable criteria for completion.

## Implementation
**How it was implemented**  
Provide details on code modules, design decisions, and steps taken.  
*If this section is left blank or contains only placeholder text, the implementing developer should first populate it with a concise high-level plan before writing code.*

**How it works**  
Explain runtime behavior and overall operation.  
*If this section is left blank or contains only placeholder text, the implementing developer should update it to describe the intended runtime behavior.*

## Notes

Any additional notes or references.
