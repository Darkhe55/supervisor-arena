//! Sensitivity detection for rating additional text (G-12).
//!
//! 4-level classifier applied to `dim_additional` + `overall_additional`:
//! - `Clean`     — nothing flagged; auto-approve in M1 mode
//! - `P2Warn`    — emotional / harsh language; auto-approve, flagged for
//!                  review triage
//! - `P1Redact`  — identifying but not direct (e.g. "张教授实验室",
//!                  "MIT AI Lab"); auto-approve, but write a `redacted_*`
//!                  copy with the identifying parts masked
//! - `P0Strict`  — direct identifier (real name + contact, address,
//!                  phone, email, "我是张伟 13800138000"); stays
//!                  `pending_review` for human review
//!
//! M6b implementation: simple keyword + regex match. M7+ can swap in a
//! proper NLP classifier (e.g. a small Chinese NER model) without
//! changing the call sites — `classify(text) -> SensitivityFlag` is the
//! stable interface.
//!
//! Reference: OUTLINE §7.12 (G-12) + DECISIONS H-37.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensitivityFlag {
    /// No flags. Auto-approve in `auto_pass` mode.
    Clean,
    /// Emotional / harsh language. Auto-approve but flag for triage.
    /// `sensitivity_flags` column will be `"P2_warn"`.
    P2Warn,
    /// Identifying but not direct (lab/project). Auto-approve + redact.
    /// `sensitivity_flags` column will be `"P1_redact"`.
    P1Redact,
    /// Direct identifier. Stays `pending_review` for human review.
    /// `sensitivity_flags` column will be `"P0_strict"`.
    P0Strict,
}

impl SensitivityFlag {
    /// Short string used in the `sensitivity_flags` DB column.
    pub fn as_db_str(self) -> &'static str {
        match self {
            SensitivityFlag::Clean => "clean",
            SensitivityFlag::P2Warn => "P2_warn",
            SensitivityFlag::P1Redact => "P1_redact",
            SensitivityFlag::P0Strict => "P0_strict",
        }
    }

    /// Parse the DB column back. Returns `Clean` for unknown / null.
    pub fn from_db_str(s: &str) -> Self {
        match s {
            "P0_strict" => Self::P0Strict,
            "P1_redact" => Self::P1Redact,
            "P2_warn" => Self::P2Warn,
            _ => Self::Clean,
        }
    }

    /// Whether this flag should block auto-approval.
    pub fn blocks_auto_approval(self) -> bool {
        matches!(self, SensitivityFlag::P0Strict)
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SensitivityError {
    #[error("regex compilation failed: {0}")]
    Regex(String),
}

/// Classify the sensitivity of one piece of additional text.
///
/// Returns the **highest** flag found across all patterns (P0 > P1 > P2 > Clean).
pub fn classify(text: &str) -> SensitivityFlag {
    use SensitivityFlag::*;
    let mut highest = Clean;

    // P0: direct identifiers.
    for pat in regex_cache::p0() {
        if pat.is_match(text) {
            return P0Strict;
        }
    }

    // P1: identifying but not direct (lab / project names).
    for pat in regex_cache::p1() {
        if pat.is_match(text) {
            highest = P1Redact;
        }
    }

    // P2: emotional / harsh language.
    for pat in regex_cache::p2() {
        if pat.is_match(text) {
            if highest == Clean {
                highest = P2Warn;
            }
        }
    }

    highest
}

/// P0 patterns: direct identifiers (real name + contact, phone, email,
/// "I'm <name>", address-like).
///
/// We intentionally use a small, conservative set of patterns to keep
/// false positives low. The reviewer queue catches anything we miss.
const P0_PATTERNS: &[&str] = &[
    // Email (loose).
    r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}",
    // Chinese mobile (11 digits starting with 1).
    r"(?x) \b 1 [3-9] \d{9} \b",
    // "我是<name>" / "我叫<name>" / "my name is <name>"
    r"我是.{1,8}",
    r"我叫.{1,8}",
    r"(?i)\bmy name is\b",
    r"(?i)\bi am\b",
    // ID card (18 digits, last may be X).
    r"\b\d{17}[\dXx]\b",
    // Address markers.
    r"地址[:：]",
    r"(?i)\baddress[:：]",
    // WeChat / QQ.
    r"(?i)\bwechat[:： ]",
    r"(?i)\bqq[:： ]\d{5,}",
];

/// P1 patterns: identifying but not direct (lab names, project names,
/// internal team).
const P1_PATTERNS: &[&str] = &[
    // Chinese 实验室 / 课题组 / 导师组.
    r"实验室",
    r"课题组",
    r"导师组",
    // English lab / group.
    r"(?i)\b\w+\s*(lab|group|team)\b",
    // Research project acronyms (3-5 caps).
    r"\b[A-Z]{3,5}\b",
];

/// P2 patterns: harsh / emotional language.
const P2_PATTERNS: &[&str] = &[
    r"最差",
    r"垃圾",
    r"骗子",
    r"滚",
    r"废物",
    r"(?i)\b(worst|garbage|scam|terrible|awful)\b",
    r"傻逼",
    r"弱智",
];

/// Lazy-compiled regexes (built on first use, cached).
///
/// We don't use the `lazy_static` / `once_cell` crates here — instead we
/// compile on first call inside `classify()`. For M6b the cost is
/// negligible (compiled once per process). If profiling shows it matters,
/// switch to `once_cell::sync::Lazy`.
mod regex_cache {
    use super::{P0_PATTERNS, P1_PATTERNS, P2_PATTERNS};
    use std::sync::OnceLock;

    static P0: OnceLock<Vec<regex::Regex>> = OnceLock::new();
    static P1: OnceLock<Vec<regex::Regex>> = OnceLock::new();
    static P2: OnceLock<Vec<regex::Regex>> = OnceLock::new();

    pub fn p0() -> &'static [regex::Regex] {
        P0.get_or_init(|| {
            P0_PATTERNS
                .iter()
                .map(|p| regex::Regex::new(p).expect("P0 regex"))
                .collect()
        })
    }
    pub fn p1() -> &'static [regex::Regex] {
        P1.get_or_init(|| {
            P1_PATTERNS
                .iter()
                .map(|p| regex::Regex::new(p).expect("P1 regex"))
                .collect()
        })
    }
    pub fn p2() -> &'static [regex::Regex] {
        P2.get_or_init(|| {
            P2_PATTERNS
                .iter()
                .map(|p| regex::Regex::new(p).expect("P2 regex"))
                .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_text_returns_clean() {
        assert_eq!(
            classify("导师认真负责,推荐。"),
            SensitivityFlag::Clean
        );
        assert_eq!(
            classify("Great advisor, very helpful."),
            SensitivityFlag::Clean
        );
    }

    #[test]
    fn p0_email_blocks_approval() {
        assert_eq!(
            classify("可以联系 zhang@example.com"),
            SensitivityFlag::P0Strict
        );
    }

    #[test]
    fn p0_phone_blocks_approval() {
        assert_eq!(
            classify("我是张伟 13800138000"),
            SensitivityFlag::P0Strict
        );
    }

    #[test]
    fn p0_my_name_phrase() {
        assert_eq!(
            classify("My name is Zhang Wei and I'm a student"),
            SensitivityFlag::P0Strict
        );
    }

    #[test]
    fn p1_lab_name() {
        assert_eq!(
            classify("在李教授实验室工作过"),
            SensitivityFlag::P1Redact
        );
    }

    #[test]
    fn p2_harsh_chinese() {
        assert_eq!(
            classify("最差的导师,完全不行"),
            SensitivityFlag::P2Warn
        );
    }

    #[test]
    fn p2_harsh_english() {
        assert_eq!(
            classify("This is the worst advisor ever, terrible."),
            SensitivityFlag::P2Warn
        );
    }

    #[test]
    fn p0_takes_precedence_over_p1() {
        // If both P0 (email) and P1 (lab) match, return P0.
        let text = "在 zhang@example.com 实验室工作";
        assert_eq!(classify(text), SensitivityFlag::P0Strict);
    }

    #[test]
    fn p1_takes_precedence_over_p2() {
        let text = "这个课题组最差";
        assert_eq!(classify(text), SensitivityFlag::P1Redact);
    }

    #[test]
    fn blocks_auto_approval_only_for_p0() {
        assert!(!SensitivityFlag::Clean.blocks_auto_approval());
        assert!(!SensitivityFlag::P2Warn.blocks_auto_approval());
        assert!(!SensitivityFlag::P1Redact.blocks_auto_approval());
        assert!(SensitivityFlag::P0Strict.blocks_auto_approval());
    }

    #[test]
    fn db_str_roundtrip() {
        for flag in [
            SensitivityFlag::Clean,
            SensitivityFlag::P2Warn,
            SensitivityFlag::P1Redact,
            SensitivityFlag::P0Strict,
        ] {
            assert_eq!(
                SensitivityFlag::from_db_str(flag.as_db_str()),
                flag
            );
        }
        // Unknown → Clean
        assert_eq!(
            SensitivityFlag::from_db_str("garbage"),
            SensitivityFlag::Clean
        );
    }
}
