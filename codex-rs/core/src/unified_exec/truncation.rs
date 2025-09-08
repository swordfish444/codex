const TRUNCATION_MARKER: &str = "[TRUNCATED CONTENT]";

pub(crate) fn truncate_middle(s: &str, max_bytes: usize) -> (String, Option<u64>) {
    if s.len() <= max_bytes {
        return (s.to_string(), None);
    }

    let est_tokens = (s.len() as u64).div_ceil(4);
    if max_bytes == 0 {
        return (TRUNCATION_MARKER.to_string(), Some(est_tokens));
    }

    fn truncate_on_boundary(input: &str, max_len: usize) -> &str {
        if input.len() <= max_len {
            return input;
        }
        let mut end = max_len;
        while end > 0 && !input.is_char_boundary(end) {
            end -= 1;
        }
        &input[..end]
    }

    fn pick_prefix_end(s: &str, left_budget: usize) -> usize {
        if let Some(head) = s.get(..left_budget)
            && let Some(i) = head.rfind('\n')
        {
            return i + 1;
        }
        truncate_on_boundary(s, left_budget).len()
    }

    fn pick_suffix_start(s: &str, right_budget: usize) -> usize {
        let start_tail = s.len().saturating_sub(right_budget);
        if let Some(tail) = s.get(start_tail..)
            && let Some(i) = tail.find('\n')
        {
            return start_tail + i + 1;
        }
        let mut idx = start_tail.min(s.len());
        while idx < s.len() && !s.is_char_boundary(idx) {
            idx += 1;
        }
        idx
    }

    let marker_len = TRUNCATION_MARKER.len();
    if marker_len >= max_bytes {
        return (TRUNCATION_MARKER.to_string(), Some(est_tokens));
    }

    let keep_budget = max_bytes.saturating_sub(marker_len);
    if keep_budget == 0 {
        return (TRUNCATION_MARKER.to_string(), Some(est_tokens));
    }

    let left_budget = keep_budget / 2;
    let right_budget = keep_budget - left_budget;
    let prefix_end = pick_prefix_end(s, left_budget);
    let mut suffix_start = pick_suffix_start(s, right_budget);
    if suffix_start < prefix_end {
        suffix_start = prefix_end;
    }

    let mut out = String::with_capacity(marker_len + prefix_end + (s.len() - suffix_start));
    out.push_str(&s[..prefix_end]);
    out.push_str(TRUNCATION_MARKER);
    out.push_str(&s[suffix_start..]);

    (out, Some(est_tokens))
}
