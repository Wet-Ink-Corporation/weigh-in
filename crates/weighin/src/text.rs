//! Text helpers shared by the command output, the context packs and the hooks.

/// `s` cut to at most `n` characters, the last of them an ellipsis when anything was cut.
#[must_use]
pub(crate) fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_owned()
    } else {
        s.chars().take(n.saturating_sub(1)).chain(['…']).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::clip;

    #[test]
    fn short_text_is_kept_and_long_text_ends_in_an_ellipsis() {
        assert_eq!(clip("abc", 3), "abc");
        assert_eq!(clip("abcdef", 4), "abc…");
        assert_eq!(clip("abc", 0), "…");
        assert_eq!(clip("a\nb", 3), "a\nb");
    }
}
