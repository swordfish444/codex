/// Simple case-insensitive subsequence matcher used for fuzzy filtering.
///
/// Returns the indices (character positions) of the matched characters in the
/// ORIGINAL `haystack` string and a score where smaller is better.
///
/// Unicode correctness: we perform the match on a lowercased copy of the
/// haystack and needle but maintain a mapping from each character in the
/// lowercased haystack back to the original character index in `haystack`.
/// This ensures the returned indices can be safely used with
/// `str::chars().enumerate()` consumers for highlighting, even when
/// lowercasing expands certain characters (e.g., ß → ss, İ → i̇).
pub fn fuzzy_match(haystack: &str, needle: &str) -> Option<(Vec<usize>, i32)> {
    if needle.is_empty() {
        return Some((Vec::new(), i32::MAX));
    }

    // Build a lowercased view of the haystack alongside a mapping from the
    // lowered-character indices back to the ORIGINAL character indices.
    let mut lowered_chars: Vec<char> = Vec::new();
    let mut lowered_to_orig_char_idx: Vec<usize> = Vec::new();
    for (orig_idx, ch) in haystack.chars().enumerate() {
        // Lowercasing may yield multiple chars; map each back to the same
        // original char index so we can highlight correctly.
        for lc in ch.to_lowercase() {
            lowered_chars.push(lc);
            lowered_to_orig_char_idx.push(orig_idx);
        }
    }

    let lowered_needle: Vec<char> = needle.to_lowercase().chars().collect();

    // Greedy subsequence match over the lowered chars.
    let mut result_orig_indices: Vec<usize> = Vec::with_capacity(lowered_needle.len());
    let mut last_lower_pos: Option<usize> = None;
    let mut cur = 0usize; // current search start in lowered_chars
    for &nc in lowered_needle.iter() {
        let mut found_at: Option<usize> = None;
        while cur < lowered_chars.len() {
            if lowered_chars[cur] == nc {
                found_at = Some(cur);
                cur += 1; // next search starts after this position
                break;
            }
            cur += 1;
        }
        let pos = found_at?;
        // Map the lowered pos back to the original character index.
        result_orig_indices.push(lowered_to_orig_char_idx[pos]);
        last_lower_pos = Some(pos);
    }

    // Score: window length (in lowered char positions) minus lowered needle
    // length (tighter is better), with a strong bonus for prefix match.
    let first_lower_pos = if result_orig_indices.is_empty() {
        0usize
    } else {
        let target_orig = result_orig_indices[0];
        lowered_to_orig_char_idx
            .iter()
            .position(|&oi| oi == target_orig)
            .unwrap_or(0)
    };
    let last_lower_pos = last_lower_pos.unwrap_or(first_lower_pos);
    let window =
        (last_lower_pos as i32 - first_lower_pos as i32 + 1) - (lowered_needle.len() as i32);
    let mut score = window.max(0);
    if first_lower_pos == 0 {
        score -= 100; // strong bonus for prefix match
    }

    // Ensure indices are unique and sorted for robust highlighting consumers.
    result_orig_indices.sort_unstable();
    result_orig_indices.dedup();
    Some((result_orig_indices, score))
}

/// Convenience wrapper to get only the indices for a fuzzy match.
pub fn fuzzy_indices(haystack: &str, needle: &str) -> Option<Vec<usize>> {
    fuzzy_match(haystack, needle).map(|(mut idx, _)| {
        // Already sorted/deduped, but keep invariant local to this helper too.
        idx.sort_unstable();
        idx.dedup();
        idx
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_basic_indices() {
        let (idx, _score) = fuzzy_match("hello", "hl").expect("match");
        assert_eq!(idx, vec![0, 2]);
    }

    #[test]
    fn unicode_dotted_i_istanbul_highlighting() {
        // 'İ' lowercases to two chars (i + combining dot). Ensure index 0 is highlighted once.
        let (idx, _score) = fuzzy_match("İstanbul", "is").expect("match");
        assert_eq!(idx, vec![0, 1]);
    }

    #[test]
    fn unicode_german_sharp_s_casefold() {
        // Ensure we at least don't panic with non-ASCII inputs; lowercase-based
        // matching does not equate ß with "ss", so this should be None.
        assert!(fuzzy_match("straße", "strasse").is_none());
    }
}
