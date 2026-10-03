const OMITTED_HANDOFF_MATERIAL: &str =
    "\n\n[Earlier Mitos handoff material omitted to fit the context limit.]\n\n";

/// Keeps both the opening framing and the most recent material.
pub fn bounded_context(context: String, max_bytes: usize) -> String {
    if context.len() <= max_bytes {
        return context;
    }
    if max_bytes == 0 {
        return String::new();
    }
    let omission = if max_bytes > OMITTED_HANDOFF_MATERIAL.len() {
        OMITTED_HANDOFF_MATERIAL
    } else {
        ""
    };
    let retained = max_bytes.saturating_sub(omission.len());
    let prefix_end = char_boundary_at_or_before(&context, retained / 2);
    let suffix_start = char_boundary_at_or_after(&context, context.len() - (retained - prefix_end));
    format!(
        "{}{}{}",
        &context[..prefix_end],
        omission,
        &context[suffix_start..]
    )
}

fn char_boundary_at_or_before(text: &str, mut index: usize) -> usize {
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn char_boundary_at_or_after(text: &str, mut index: usize) -> usize {
    while !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_limit_preserves_valid_utf8_and_both_ends() {
        const LIMIT: usize = 48 * 1024;
        let context = format!("start-{}-end", "💡".repeat(LIMIT));
        let bounded = bounded_context(context, LIMIT);

        assert!(bounded.len() <= LIMIT);
        assert!(bounded.starts_with("start-"));
        assert!(bounded.ends_with("-end"));
        assert!(bounded.contains("Earlier Mitos handoff material omitted"));
    }

    #[test]
    fn context_limit_never_exceeds_a_tiny_configured_cap() {
        assert!(bounded_context("a long handoff".into(), 1).len() <= 1);
        assert_eq!(bounded_context("a long handoff".into(), 0), "");
    }
}
