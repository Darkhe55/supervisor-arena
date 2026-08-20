//! Word lists used to compose supervisor aliases
//!
//! See OUTLINE §7.10.3 for the style taxonomy. Lists are intentionally
//! **embedded as constants** (not loaded from disk at runtime) so that:
//! 1. The binary is self-contained — no deployment-time asset wiring.
//! 2. The lists are versioned with the code (any change is a code change).
//! 3. Unit tests can pin the exact word pool for reproducibility.
//!
//! **All strings are deliberately not real human names** — see
//! `whitelist.rs` for the 10000+ name collision check. If a future
//! editor adds a string here that *looks like* a real name, the
//! integration test in `alias.rs` will catch it (whitelist membership).
//!
//! # Growth plan
//! Current size: ~25 literary + 25 nature + 24 geometric + 60 discipline-fused
//! = ~135 base components. With template slots (prefix/suffix/numeric), the
//! total addressable aliases are well above 10^6. OUTLINE §7.10.7 says
//! "词库 10000+" before production launch — track this in DECISIONS H-23.

/// Literary style words — Chinese two-character poetic nouns.
/// Style: "{word}" or "{word}·{title}" — e.g. "南飞雁", "青松·听松客"
pub const LITERARY_WORDS: &[&str] = &[
    "南飞雁", "青松", "听松客", "远山客", "墨客", "青简", "诗酒", "云隐", "月明",
    "听泉", "观星", "栖霞", "卧云", "枕流", "寻梅", "访菊", "问竹", "踏雪",
    "望月", "归鸿", "流光", "静夜", "寒山", "白石", "碧云", "清溪", "暮云",
];

/// Literary honorific titles (used as second slot in literary style).
pub const LITERARY_TITLES: &[&str] = &[
    "先生", "居士", "山人", "散人", "翁", "客", "主人", "主人翁",
];

/// Nature style — single-word nature nouns, combined as "{word}·{word2}".
pub const NATURE_WORDS: &[&str] = &[
    "远山", "溪流", "星辰", "海风", "松涛", "竹影", "月光", "雪原", "秋林",
    "夏萤", "春晓", "冬岭", "晨雾", "晚霞", "银河", "原野", "荒原", "林间",
    "山岚", "云海", "潮声", "落叶", "寒鸦", "白露", "霜天",
];

/// Geometric code prefix — Latin letters, 1-2 chars.
pub const GEOMETRIC_PREFIXES: &[&str] = &[
    "Q", "AURORA", "ECHO", "NOVA", "OMEGA", "KAIROS", "ATLAS", "ORION",
    "VEIL", "LUMEN", "ARC", "PRISM",
];

/// Greek letters (math/science-flavoured symbols).
/// Listed in full so editors can see what's available even if not all
/// are used by the current discipline template set.
pub const GREEK_LETTERS: &[&str] = &[
    "α", "β", "γ", "δ", "ε", "ζ", "η", "θ", "λ", "μ", "π", "σ", "τ", "φ", "χ", "ψ", "ω",
    "Γ", "Δ", "Θ", "Λ", "Π", "Σ", "Φ", "Ψ", "Ω",
];

/// Math / set-theory symbols.
pub const MATH_SYMBOLS: &[&str] = &["Δ", "Σ", "∫", "π", "Ω", "ℝ", "ℕ", "ℂ", "ℤ", "∇", "∂"];

/// Discipline category. Maps a `disciplines.key` value (e.g. "computer_science")
/// to one of 6 abstract buckets. Unmapped disciplines fall back to `General`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisciplineCategory {
    /// CS / AI / data science
    Computing,
    /// Literature / history / philosophy
    Humanities,
    /// Math / physics / chemistry
    Sciences,
    /// Medicine / biology
    LifeSciences,
    /// Engineering (mech / elec / civil / ...)
    Engineering,
    /// Economics / management / finance
    Business,
    /// Fallback for unmapped disciplines
    General,
}

impl DisciplineCategory {
    /// Map a discipline key to a category. Conservative — any unknown
    /// key falls back to `General` rather than being silently miscategorised.
    pub fn from_discipline(discipline: &str) -> Self {
        match discipline {
            "computer_science" | "artificial_intelligence" | "data_science" | "informatics"
            | "software_engineering" => Self::Computing,

            "literature" | "history" | "philosophy" | "linguistics" | "arts"
            | "religious_studies" => Self::Humanities,

            "mathematics" | "physics" | "chemistry" | "astronomy" | "earth_sciences"
            | "statistics" => Self::Sciences,

            "medicine" | "biology" | "pharmacy" | "public_health" | "nursing"
            | "biotechnology" => Self::LifeSciences,

            "mechanical_engineering" | "electrical_engineering" | "civil_engineering"
            | "chemical_engineering" | "materials_science" | "aerospace_engineering" => {
                Self::Engineering
            }

            "economics" | "management" | "finance" | "accounting" | "marketing"
            | "business_administration" => Self::Business,

            _ => Self::General,
        }
    }

    /// Template pool for the discipline-fused style. Each template has one
    /// `{X}` placeholder that will be filled with a deterministic 3-char
    /// alphanumeric suffix derived from the seed.
    pub const fn templates(self) -> &'static [&'static str] {
        match self {
            Self::Computing => &[
                "α-net-{X}", "Σ-{X}", "λ-qubit-{X}", "β-layer-{X}", "γ-clust-{X}",
                "ψ-net-{X}", "Ω-graph-{X}", "μ-func-{X}",
            ],
            Self::Humanities => &[
                "诗酒-远山-{X}", "青简-墨客-{X}", "云隐-听泉-{X}", "月明-归鸿-{X}",
                "诗-远山-{X}", "墨-{X}", "青简-{X}",
            ],
            Self::Sciences => &[
                "Δ-{X}", "∫-curve-{X}", "ℝ-{X}", "π-field-{X}", "Ω-tensor-{X}",
                "∇-{X}", "Σ-set-{X}",
            ],
            Self::LifeSciences => &[
                "cell-Δ-{X}", "Helix-{X}", "gene-π-{X}", "Ω-bio-{X}", "μ-cell-{X}",
                "tissue-σ-{X}", "Helix-9k-{X}",
            ],
            Self::Engineering => &[
                "Ω-relay-{X}", "vector-{X}", "Ω-link-{X}", "μ-torque-{X}",
                "σ-struct-{X}", "Ω-flow-{X}", "Δ-stress-{X}",
            ],
            Self::Business => &[
                "σ-market-{X}", "μ-curve-{X}", "Ω-index-{X}", "π-yield-{X}",
                "Δ-portfolio-{X}", "σ-rate-{X}", "μ-elasticity-{X}",
            ],
            Self::General => &[
                "α-{X}", "β-{X}", "γ-{X}", "λ-{X}", "μ-{X}", "σ-{X}", "π-{X}", "ω-{X}",
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discipline_categories_have_templates() {
        // Every category must have at least one template.
        for cat in [
            DisciplineCategory::Computing,
            DisciplineCategory::Humanities,
            DisciplineCategory::Sciences,
            DisciplineCategory::LifeSciences,
            DisciplineCategory::Engineering,
            DisciplineCategory::Business,
            DisciplineCategory::General,
        ] {
            assert!(
                !cat.templates().is_empty(),
                "{cat:?} has no templates"
            );
        }
    }

    #[test]
    fn templates_have_exactly_one_placeholder() {
        // The {X} placeholder is mandatory — bad templates are caught here
        // before they ship, since filling without one will silently misformat.
        let all: Vec<&[&str]> = vec![
            DisciplineCategory::Computing.templates(),
            DisciplineCategory::Humanities.templates(),
            DisciplineCategory::Sciences.templates(),
            DisciplineCategory::LifeSciences.templates(),
            DisciplineCategory::Engineering.templates(),
            DisciplineCategory::Business.templates(),
            DisciplineCategory::General.templates(),
        ];
        for cat_templates in all {
            for tmpl in cat_templates {
                let count = tmpl.matches("{X}").count();
                assert_eq!(
                    count, 1,
                    "template `{tmpl}` has {count} {{X}} placeholders, expected 1"
                );
            }
        }
    }

    #[test]
    fn known_disciplines_map_correctly() {
        assert_eq!(
            DisciplineCategory::from_discipline("computer_science"),
            DisciplineCategory::Computing
        );
        assert_eq!(
            DisciplineCategory::from_discipline("mathematics"),
            DisciplineCategory::Sciences
        );
        assert_eq!(
            DisciplineCategory::from_discipline("medicine"),
            DisciplineCategory::LifeSciences
        );
    }

    #[test]
    fn unknown_discipline_falls_back_to_general() {
        assert_eq!(
            DisciplineCategory::from_discipline("underwater_basket_weaving"),
            DisciplineCategory::General
        );
    }
}
