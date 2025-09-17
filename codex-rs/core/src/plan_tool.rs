use std::collections::BTreeMap;
use std::sync::LazyLock;

use crate::codex::Session;
use crate::openai_tools::JsonSchema;
use crate::openai_tools::OpenAiTool;
use crate::openai_tools::ResponsesApiTool;
use crate::protocol::Event;
use crate::protocol::EventMsg;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ResponseInputItem;

// Use the canonical plan tool types from the protocol crate to ensure
// type-identity matches events transported via `codex_protocol`.
pub use codex_protocol::plan_tool::PlanItemArg;
pub use codex_protocol::plan_tool::StepStatus;
pub use codex_protocol::plan_tool::UpdatePlanArgs;

// Types for the TODO tool arguments matching codex-vscode/todo-mcp/src/main.rs

pub(crate) static PLAN_TOOL: LazyLock<OpenAiTool> = LazyLock::new(|| {
    let mut plan_item_props = BTreeMap::new();
    plan_item_props.insert("step".to_string(), JsonSchema::String { description: None });
    plan_item_props.insert(
        "status".to_string(),
        JsonSchema::String {
            description: Some("One of: pending, in_progress, completed".to_string()),
        },
    );

    let plan_items_schema = JsonSchema::Array {
        description: Some("The list of steps".to_string()),
        items: Box::new(JsonSchema::Object {
            properties: plan_item_props,
            required: Some(vec!["step".to_string(), "status".to_string()]),
            additional_properties: Some(false),
        }),
    };

    let mut properties = BTreeMap::new();
    properties.insert(
        "explanation".to_string(),
        JsonSchema::String { description: None },
    );
    properties.insert("plan".to_string(), plan_items_schema);

    OpenAiTool::Function(ResponsesApiTool {
        name: "update_plan".to_string(),
        description: r#"Updates the task plan.
Provide an optional explanation and a list of plan items, each with a step and status.
At most one step can be in_progress at a time.
"#
        .to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["plan".to_string()]),
            additional_properties: Some(false),
        },
    })
});

/// This function doesn't do anything useful. However, it gives the model a structured way to record its plan that clients can read and render.
/// So it's the _inputs_ to this function that are useful to clients, not the outputs and neither are actually useful for the model other
/// than forcing it to come up and document a plan (TBD how that affects performance).
pub(crate) async fn handle_update_plan(
    session: &Session,
    arguments: String,
    sub_id: String,
    call_id: String,
) -> ResponseInputItem {
    match parse_update_plan_arguments(arguments, &call_id) {
        Ok(args) => {
            let output = ResponseInputItem::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload {
                    content: "Plan updated".to_string(),
                    success: Some(true),
                },
            };
            session
                .send_event(Event {
                    id: sub_id.to_string(),
                    msg: EventMsg::PlanUpdate(args),
                })
                .await;
            output
        }
        Err(output) => *output,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedStep {
    pub step: String,
    pub position: usize,
    pub total: usize,
}

pub fn newly_completed_steps(
    previous: &[PlanItemArg],
    current: &[PlanItemArg],
) -> Vec<CompletedStep> {
    use std::collections::HashMap;

    let mut prev_status: HashMap<&str, &StepStatus> = HashMap::new();
    for item in previous {
        let step = item.step.trim();
        if !step.is_empty() {
            prev_status.insert(step, &item.status);
        }
    }

    let total = current.len();
    let mut completed = Vec::new();
    for (idx, item) in current.iter().enumerate() {
        let step = item.step.trim();
        if step.is_empty() || !matches!(item.status, StepStatus::Completed) {
            continue;
        }
        let was_completed = prev_status
            .get(step)
            .map(|status| matches!(status, StepStatus::Completed))
            .unwrap_or(false);
        if !was_completed {
            completed.push(CompletedStep {
                step: step.to_owned(),
                position: idx + 1,
                total,
            });
        }
    }

    completed
}

fn parse_update_plan_arguments(
    arguments: String,
    call_id: &str,
) -> Result<UpdatePlanArgs, Box<ResponseInputItem>> {
    match serde_json::from_str::<UpdatePlanArgs>(&arguments) {
        Ok(args) => Ok(args),
        Err(e) => {
            let output = ResponseInputItem::FunctionCallOutput {
                call_id: call_id.to_string(),
                output: FunctionCallOutputPayload {
                    content: format!("failed to parse function arguments: {e}"),
                    success: None,
                },
            };
            Err(Box::new(output))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_step(step: &str, status: StepStatus) -> PlanItemArg {
        PlanItemArg {
            step: step.to_string(),
            status,
        }
    }

    #[test]
    fn detects_newly_completed_steps() {
        let prev = vec![
            make_step("Explore", StepStatus::Completed),
            make_step("Implement", StepStatus::InProgress),
        ];
        let current = vec![
            make_step("Explore", StepStatus::Completed),
            make_step("Implement", StepStatus::Completed),
            make_step("Document", StepStatus::Pending),
        ];

        let completed = newly_completed_steps(&prev, &current);
        assert_eq!(
            completed,
            vec![CompletedStep {
                step: "Implement".to_string(),
                position: 2,
                total: 3,
            }]
        );
    }

    #[test]
    fn ignores_already_completed_steps() {
        let prev = vec![make_step("Explore", StepStatus::Completed)];
        let current = vec![make_step("Explore", StepStatus::Completed)];

        let completed = newly_completed_steps(&prev, &current);
        assert!(completed.is_empty());
    }

    #[test]
    fn trims_whitespace_in_step_names() {
        let prev = vec![make_step(" Implement ", StepStatus::InProgress)];
        let current = vec![make_step("Implement", StepStatus::Completed)];

        let completed = newly_completed_steps(&prev, &current);
        assert_eq!(
            completed,
            vec![CompletedStep {
                step: "Implement".to_string(),
                position: 1,
                total: 1,
            }]
        );
    }
}
