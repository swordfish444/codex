//! Utilities for computing approximate token usage and remaining context percentage
//! in the TUI, mirroring the JS heuristics in `calculateContextPercentRemaining`.

use codex_core::ContentItem;
use codex_core::ResponseItem;

/// Roughly estimate number of model tokens represented by the given response items.
/// Counts characters in text and function-call items, divides by 4 and rounds up.
pub fn approximate_tokens_used(items: &[ResponseItem]) -> usize {
    let mut char_count = 0;
    for item in items {
        match item {
            ResponseItem::Message { role, content }
                if role.eq_ignore_ascii_case("user") || role.eq_ignore_ascii_case("assistant") =>
            {
                for ci in content {
                    match ci {
                        ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                            char_count += text.len()
                        }
                        _ => {}
                    }
                }
            }
            ResponseItem::FunctionCall {
                name, arguments, ..
            } => {
                char_count += name.len();
                char_count += arguments.len();
            }
            ResponseItem::FunctionCallOutput { output, .. } => {
                char_count += output.content.len();
            }
            _ => {}
        }
    }
    (char_count + 3) / 4
}

/// Return the model's max context size in tokens, using known limits or heuristics.
pub fn max_tokens_for_model(model: &str) -> usize {
    // Known OpenAI model limits
    match model {
        // 4k context models
        m if m.eq_ignore_ascii_case("gpt-3.5-turbo") => 4096,
        m if m.eq_ignore_ascii_case("gpt-4o") => 8192,
        // 8k context
        m if m.to_lowercase().contains("8k") => 8192,
        // 16k context
        m if m.to_lowercase().contains("16k") => 16384,
        // 32k context
        m if m.to_lowercase().contains("32k") => 32768,
        // Fallback default
        _ => 131072,
    }
}

/// Compute the percentage of tokens remaining in context for a given model.
/// Returns a floating-point percent (0.0–100.0).
pub fn calculate_context_percent_remaining(items: &[ResponseItem], model: &str) -> f64 {
    let used = approximate_tokens_used(items);
    let max = max_tokens_for_model(model);
    let remaining = max.saturating_sub(used);
    (remaining as f64) / (max as f64) * 100.0
}
