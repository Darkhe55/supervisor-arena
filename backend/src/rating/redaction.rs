//! P1 redaction — replace identifying but-not-direct markers in rating
//! additional text with `[REDACTED]`.
//!
//! Used in M6c to write the public-safe version of dim_additional /
//! overall_additional into the `redacted_*_enc` columns (G-12).
//!
//! # Rules (mirrors the P1 patterns in `sensitivity.rs`)
//!
//! | Match                  | Replacement       |
//! |------------------------|-------------------|
//! | `实验室`                | `[REDACTED]`      |
//! | `课题组`                | `[REDACTED]`      |
//! | `<Word> Lab`           | `[REDACTED] Lab`  |
//! | `<Word> Group`         | `[REDACTED] Group`|
//! | `<Word> Team`          | `[REDACTED] Team` |
//!
//! Skipped on purpose (too risky to auto-redact):
//! - 3-5 caps project acronyms (e.g. `MIT`, `AI`) — false-positive heavy
//! - Lab director names — requires NLP
//!
//! # Design notes
//!
//! - We do **not** redact P0 patterns (email, phone, ID). If a rating
//!   contains P0 markers, the whole rating is held for human review
//!   anyway (per M6b H-38). Redacting inside a P0-flagged rating would
//!   mask identifiers the reviewer needs to see.
//! - Replacement is case-sensitive on Chinese tokens (`实验室`) and
//!   case-insensitive on the English `<Word> Lab` pattern.
//! - Each pattern runs as a `regex::Regex::replace_all`. Order doesn't
//!   matter (no overlapping matches in practice).

use std::sync::OnceLock;

/// One compiled regex + its replacement string.
struct RedactPattern {
    regex: regex::Regex,
    replacement: String,
}

impl RedactPattern {
    fn new(re: &str, replacement: &str) -> Self {
        Self {
            regex: regex::Regex::new(re).expect("redact regex"),
            replacement: replacement.to_string(),
        }
    }

    fn apply<'a>(&self, text: &'a str) -> std::borrow::Cow<'a, str> {
        self.regex.replace_all(text, self.replacement.as_str())
    }
}

/// Compile the P1 patterns once on first use.
static P1_PATTERNS: OnceLock<Vec<RedactPattern>> = OnceLock::new();

fn p1_patterns() -> &'static [RedactPattern] {
    P1_PATTERNS.get_or_init(|| {
        vec![
            // Chinese: 实验室 / 课题组 → [REDACTED]
            RedactPattern::new(r"实验室", "[REDACTED]"),
            RedactPattern::new(r"课题组", "[REDACTED]"),
            // English: "<Word> Lab|Group|Team" → "[REDACTED] <suffix>"
            // The qualifier is replaced but the generic suffix stays so the
            // sentence is still readable.
            RedactPattern::new(r"(?i)\b(\w+)\s+(Lab|Group|Team)\b", "[REDACTED] $2"),
        ]
    })
}

/// Apply P1 redaction rules to `text`. Returns the redacted string.
///
/// The function is pure — no I/O, no allocations beyond the returned
/// `String`. Safe to call inside hot paths.
pub fn redact_p1(text: &str) -> String {
    let mut out: String = text.to_string();
    for pat in p1_patterns() {
        // `into_owned()` materializes the Cow so the next pattern can
        // operate on a `String` again.
        out = pat.apply(&out).into_owned();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_returns_empty() {
        assert_eq!(redact_p1(""), "");
    }

    #[test]
    fn clean_text_unchanged() {
        let s = "导师认真负责,推荐。Great advisor, very helpful.";
        assert_eq!(redact_p1(s), s);
    }

    #[test]
    fn redacts_chinese_lab() {
        assert_eq!(redact_p1("在李教授实验室工作过"), "在李教授[REDACTED]工作过");
    }

    #[test]
    fn redacts_chinese_research_group() {
        assert_eq!(redact_p1("加入张老师的课题组"), "加入张老师的[REDACTED]");
    }

    #[test]
    fn redacts_english_lab() {
        assert_eq!(
            redact_p1("Worked at the Vision Lab"),
            "Worked at the [REDACTED] Lab"
        );
    }

    #[test]
    fn redacts_english_group() {
        assert_eq!(
            redact_p1("Joined the NLP Group"),
            "Joined the [REDACTED] Group"
        );
    }

    #[test]
    fn redacts_english_team() {
        // "AI Safety Team" → only "Safety" (the word immediately before
        // "Team") is redacted; "AI" stays because it's a broader field
        // identifier, not the specific team name. This is more precise
        // than redacting the whole phrase.
        assert_eq!(
            redact_p1("Part of the AI Safety Team"),
            "Part of the AI [REDACTED] Team"
        );
    }

    #[test]
    fn case_insensitive_english() {
        assert_eq!(redact_p1("the VISION lab"), "the [REDACTED] lab");
    }

    #[test]
    fn multiple_redactions_in_one_text() {
        // MIT alone is not a "<Word> Lab" pattern (no space + Lab), so
        // it stays. The Chinese 实验室 still matches → [REDACTED].
        let s = "曾在 MIT 实验室和 Vision Lab 工作,后加入 AI Group";
        let expected =
            "曾在 MIT [REDACTED]和 [REDACTED] Lab 工作,后加入 [REDACTED] Group";
        assert_eq!(redact_p1(s), expected);
    }

    #[test]
    fn p0_patterns_not_redacted() {
        // P0 (email, phone) is handled at the sensitivity layer — we do
        // NOT redact it here, so reviewers can see the original.
        let s = "Contact zhang@example.com or 13800138000";
        assert_eq!(redact_p1(s), s);
    }
}
