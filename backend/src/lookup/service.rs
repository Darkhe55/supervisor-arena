//! Service layer for the lookup module.
//!
//! The data lives in 3 static tables (`disciplines`, `colleges`,
//! `rating_dimensions`); the service is a thin wrapper that
//! runs the SELECT and shapes the response.

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use super::error::LookupError;

/// RFC 7231 — the set of language tags we currently understand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptLanguage {
    Zh,
    En,
}

impl AcceptLanguage {
    /// Parse a raw `Accept-Language` header value into one of the
    /// supported tags. We don't do full RFC 7231 q-value ranking
    /// because we only have 2 languages; the first match wins.
    pub fn parse(header: Option<&str>) -> Self {
        let Some(s) = header else { return AcceptLanguage::Zh };
        for tag in s.split(',') {
            let primary = tag.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
            if primary.starts_with("zh") {
                return AcceptLanguage::Zh;
            }
            if primary.starts_with("en") {
                return AcceptLanguage::En;
            }
        }
        AcceptLanguage::Zh
    }

    /// The shorthand key used in the response's `name` field
    /// (per H-56 — the negotiated top-level name).
    pub fn as_str(self) -> &'static str {
        match self {
            AcceptLanguage::Zh => "zh",
            AcceptLanguage::En => "en",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalizedDiscipline {
    pub code: String,
    pub name: String,
    pub name_zh: String,
    pub name_en: String,
    pub category: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalizedCollege {
    pub code: String,
    pub name: String,
    pub name_zh: String,
    pub name_en: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalizedDimension {
    pub code: String,
    pub name: String,
    pub name_zh: String,
    pub name_en: String,
    pub description: String,
    pub description_zh: Option<String>,
    pub description_en: Option<String>,
    pub sort_order: i32,
    pub is_active: bool,
}

#[derive(Clone)]
pub struct LookupService {
    pool: deadpool_postgres::Pool,
}

impl LookupService {
    pub fn new(pool: deadpool_postgres::Pool) -> Self {
        Self { pool }
    }

    pub async fn list_disciplines(
        &self,
        lang: AcceptLanguage,
    ) -> Result<Vec<LocalizedDiscipline>, LookupError> {
        let c = self.pool.get().await?;
        let rows = c
            .query(
                "SELECT code, name_zh, name_en, category, is_active
                 FROM disciplines
                 WHERE is_active = TRUE
                 ORDER BY code",
                &[],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let name_zh: String = r.get(1);
                let name_en: String = r.get(2);
                LocalizedDiscipline {
                    code: r.get(0),
                    name: pick_name(lang, &name_zh, &name_en),
                    name_zh,
                    name_en,
                    category: r.get(3),
                    is_active: r.get(4),
                }
            })
            .collect())
    }

    pub async fn list_colleges(
        &self,
        lang: AcceptLanguage,
    ) -> Result<Vec<LocalizedCollege>, LookupError> {
        let c = self.pool.get().await?;
        let rows = c
            .query(
                "SELECT code, name_zh, name_en, is_active
                 FROM colleges
                 WHERE is_active = TRUE
                 ORDER BY code",
                &[],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let name_zh: String = r.get(1);
                let name_en: String = r.get(2);
                LocalizedCollege {
                    code: r.get(0),
                    name: pick_name(lang, &name_zh, &name_en),
                    name_zh,
                    name_en,
                    is_active: r.get(3),
                }
            })
            .collect())
    }

    pub async fn list_dimensions(
        &self,
        lang: AcceptLanguage,
    ) -> Result<Vec<LocalizedDimension>, LookupError> {
        let c = self.pool.get().await?;
        let rows = c
            .query(
                "SELECT code, name_zh, name_en, description_zh, description_en,
                        sort_order, is_active
                 FROM rating_dimensions
                 WHERE is_active = TRUE
                 ORDER BY sort_order",
                &[],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let name_zh: String = r.get(1);
                let name_en: String = r.get(2);
                let description_zh: Option<String> = r.get(3);
                let description_en: Option<String> = r.get(4);
                LocalizedDimension {
                    code: r.get(0),
                    name: pick_name(lang, &name_zh, &name_en),
                    name_zh,
                    name_en,
                    description: pick_opt_name(lang, &description_zh, &description_en),
                    description_zh,
                    description_en,
                    sort_order: r.get(5),
                    is_active: r.get(6),
                }
            })
            .collect())
    }
}

fn pick_name(lang: AcceptLanguage, zh: &str, en: &str) -> String {
    match lang {
        AcceptLanguage::Zh => zh.to_string(),
        AcceptLanguage::En => en.to_string(),
    }
}

fn pick_opt_name(
    lang: AcceptLanguage,
    zh: &Option<String>,
    en: &Option<String>,
) -> String {
    match lang {
        AcceptLanguage::Zh => zh.clone().or_else(|| en.clone()).unwrap_or_default(),
        AcceptLanguage::En => en.clone().or_else(|| zh.clone()).unwrap_or_default(),
    }
}

// Marker so the chrono import isn't unused (the DateTime<Utc> is
// included via the row_get pattern in case we later add time fields).
#[allow(dead_code)]
fn _phantom(_: DateTime<Utc>, _: Uuid) {}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- AcceptLanguage::parse ----

    #[test]
    fn parse_default_zh_when_no_header() {
        assert_eq!(AcceptLanguage::parse(None), AcceptLanguage::Zh);
    }

    #[test]
    fn parse_default_zh_when_empty_header() {
        assert_eq!(AcceptLanguage::parse(Some("")), AcceptLanguage::Zh);
    }

    #[test]
    fn parse_zh_simple() {
        assert_eq!(AcceptLanguage::parse(Some("zh")), AcceptLanguage::Zh);
    }

    #[test]
    fn parse_zh_with_region() {
        assert_eq!(AcceptLanguage::parse(Some("zh-CN")), AcceptLanguage::Zh);
        assert_eq!(AcceptLanguage::parse(Some("zh-Hans")), AcceptLanguage::Zh);
        assert_eq!(AcceptLanguage::parse(Some("zh-Hant-HK")), AcceptLanguage::Zh);
    }

    #[test]
    fn parse_en_simple() {
        assert_eq!(AcceptLanguage::parse(Some("en")), AcceptLanguage::En);
    }

    #[test]
    fn parse_en_with_region() {
        assert_eq!(AcceptLanguage::parse(Some("en-US")), AcceptLanguage::En);
        assert_eq!(AcceptLanguage::parse(Some("en-GB")), AcceptLanguage::En);
    }

    #[test]
    fn parse_first_match_wins() {
        // First tag wins (no full q-value ranking per H-56).
        assert_eq!(AcceptLanguage::parse(Some("en-US,zh-CN")), AcceptLanguage::En);
        assert_eq!(AcceptLanguage::parse(Some("zh-CN,en-US")), AcceptLanguage::Zh);
    }

    #[test]
    fn parse_unknown_tag_falls_back_to_zh() {
        assert_eq!(AcceptLanguage::parse(Some("fr")), AcceptLanguage::Zh);
        assert_eq!(AcceptLanguage::parse(Some("ja-JP")), AcceptLanguage::Zh);
        assert_eq!(AcceptLanguage::parse(Some("fr,en")), AcceptLanguage::En);
    }

    #[test]
    fn parse_with_quality_value() {
        // q-value annotation is stripped (we only care about the tag).
        assert_eq!(AcceptLanguage::parse(Some("en;q=0.9")), AcceptLanguage::En);
        assert_eq!(AcceptLanguage::parse(Some("zh-CN;q=1.0,en-US;q=0.5")), AcceptLanguage::Zh);
    }

    #[test]
    fn parse_case_insensitive() {
        assert_eq!(AcceptLanguage::parse(Some("EN")), AcceptLanguage::En);
        assert_eq!(AcceptLanguage::parse(Some("ZH-TW")), AcceptLanguage::Zh);
    }

    // ---- pick_name ----

    #[test]
    fn pick_name_returns_zh_when_lang_zh() {
        assert_eq!(pick_name(AcceptLanguage::Zh, "计算机", "Computer Science"), "计算机");
    }

    #[test]
    fn pick_name_returns_en_when_lang_en() {
        assert_eq!(pick_name(AcceptLanguage::En, "计算机", "Computer Science"), "Computer Science");
    }

    // ---- pick_opt_name ----

    #[test]
    fn pick_opt_name_falls_back_across_languages() {
        // EN selected, EN description missing → falls back to ZH.
        assert_eq!(
            pick_opt_name(AcceptLanguage::En, &Some("学科适配性".into()), &None),
            "学科适配性"
        );
        // ZH selected, ZH description missing → falls back to EN.
        assert_eq!(
            pick_opt_name(AcceptLanguage::Zh, &None, &Some("Subject Fit".into())),
            "Subject Fit"
        );
    }

    #[test]
    fn pick_opt_name_returns_empty_when_both_none() {
        assert_eq!(pick_opt_name(AcceptLanguage::Zh, &None, &None), "");
    }

    #[test]
    fn accept_language_as_str() {
        assert_eq!(AcceptLanguage::Zh.as_str(), "zh");
        assert_eq!(AcceptLanguage::En.as_str(), "en");
    }
}
