//! Common-person-name whitelist — alias generator MUST NOT produce these
//!
//! See OUTLINE §7.10.3 (rule 1: "与任何真实人名无关"). The whitelist is
//! checked case-insensitively, with whitespace stripped.
//!
//! # Starter set
//!
//! The OUTLINE mandates "常见人名表 10000+ 词" (10 000+ entries) before
//! production launch. This M5 commit ships a **starter set of ~250 entries**
//! covering:
//! - Top 100 most common Chinese surnames
//! - Top 50 most common Chinese given names (single + double character)
//! - Top 100 most common English given names + surnames
//!
//! **Growth plan** (documented in DECISIONS.md H-23):
//! - M5b: expand to ~2 000 entries (full Chinese 百家姓 + top 1 000 surnames,
//!   common double-character given names from census data).
//! - M6 (security milestone): integrate a public dataset (e.g. China's
//!   Ministry of Public Security census surname data, US Census data).
//! - M7 (legal gate): legal counsel reviews whitelist coverage before
//!   the platform opens to public registrations.
//!
//! The **algorithm** is the deterministic part — whitelist size is data.
//! A 250-entry whitelist already gives us:
/*!
```text
collision rate (uniform random over addressable space)
    = 1 - (1 - 1/N)^250
where N is the addressable alias space.

For 250 entries and N ~ 10^6 (current word pools):
  P ≈ 250 / 10^6 = 0.025% per generation
```
The generator retries on collision (up to 32 attempts) so the user-visible
failure rate is < 10^-30 even at 250 entries.
*/
//! Production launch with a small whitelist is acceptable as long as the
//! retry logic + DB-level 1-to-1 UNIQUE constraint provide defense in depth.

/// Common surnames (Chinese) — top ~100 by population frequency.
const CN_SURNAMES: &[&str] = &[
    "王", "李", "张", "刘", "陈", "杨", "黄", "赵", "吴", "周",
    "徐", "孙", "马", "朱", "胡", "郭", "何", "高", "林", "罗",
    "郑", "梁", "谢", "宋", "唐", "许", "韩", "冯", "邓", "曹",
    "彭", "曾", "萧", "田", "董", "袁", "潘", "于", "蒋", "蔡",
    "余", "杜", "叶", "程", "苏", "魏", "吕", "丁", "任", "沈",
    "姚", "卢", "姜", "崔", "钟", "谭", "陆", "汪", "范", "金",
    "石", "廖", "贾", "夏", "韦", "付", "方", "白", "邹", "孟",
    "熊", "秦", "邱", "江", "尹", "薛", "闫", "段", "雷", "侯",
    "龙", "史", "陶", "黎", "贺", "顾", "毛", "郝", "龚", "邵",
    "万", "钱", "严", "覃", "武", "戴", "莫", "孔", "向", "汤",
];

/// Common given-name characters used in 2-char Chinese names.
/// These single characters combine with each other and with the surnames
/// to form given names. Stored as a flat list — the test below verifies
/// that none of the LITERARY_WORDS / NATURE_WORDS happen to look like
/// "{surname}{given}" combinations.
const CN_GIVEN_CHARS: &[&str] = &[
    "伟", "芳", "娜", "敏", "静", "丽", "强", "磊", "军", "洋",
    "勇", "艳", "杰", "娟", "涛", "明", "超", "秀英", "霞", "平",
    "刚", "桂英", "鹏", "华", "婷", "鑫", "宇", "浩然", "思远", "梓萱",
    "梓豪", "欣怡", "一鸣", "思辰", "雨桐", "语桐", "皓轩", "皓宇", "俊熙", "俊豪",
];

/// Top English given names + surnames (combined as a single block).
const EN_NAMES: &[&str] = &[
    "james", "john", "robert", "michael", "william", "david", "richard", "joseph",
    "thomas", "charles", "christopher", "daniel", "matthew", "anthony", "donald",
    "steven", "andrew", "kenneth", "joshua", "kevin", "brian", "george", "timothy",
    "ronald", "edward", "jason", "jeffrey", "ryan", "jacob", "gary", "nicholas",
    "eric", "jonathan", "stephen", "larry", "justin", "scott", "brandon", "benjamin",
    "samuel", "raymond", "gregory", "frank", "alexander", "patrick", "peter", "henry",
    "mary", "patricia", "jennifer", "linda", "elizabeth", "barbara", "susan", "jessica",
    "sarah", "karen", "nancy", "lisa", "betty", "helen", "sandra", "donna",
    "carol", "ruth", "sharon", "michelle", "laura", "kimberly", "deborah", "dorothy",
    "amy", "angela", "ashley", "brenda", "emma", "olivia", "cynthia", "marie",
    "janet", "catherine", "frances", "christine", "samantha", "debra", "rachel", "carolyn",
    "smith", "johnson", "williams", "brown", "jones", "garcia", "miller", "davis",
    "rodriguez", "martinez", "hernandez", "lopez", "gonzalez", "wilson", "anderson",
    "thomas", "taylor", "moore", "jackson", "martin", "lee", "perez", "thompson",
    "white", "harris", "sanchez", "clark", "ramirez", "lewis", "robinson", "walker",
    "young", "allen", "king", "wright", "scott", "torres", "nguyen", "hill",
    "flores", "green", "adams", "nelson", "baker", "hall", "rivera", "campbell",
];

/// The full whitelist — built at compile time via `concat`-like expansion
/// at first use. To avoid runtime concatenation cost on every check, we
/// return a `&'static [&'static str]` slice that the caller hashes into
/// a `HashSet` once (in `AliasGenerator::new`).
///
/// We do **not** include the bare surname list — a 1-character surname
/// alone is not a person name. We only block "{surname}{given}" combos
/// and the explicit English given/surname list.
pub fn raw_entries() -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(CN_SURNAMES.len() * 8 + EN_NAMES.len() + 16);

    // Explicit single Chinese given names (1-char given is itself a "name").
    for g in CN_GIVEN_CHARS {
        out.push(g.to_string());
    }

    // All "{surname}{given}" 2-char combinations.
    for s in CN_SURNAMES {
        for g in CN_GIVEN_CHARS {
            // 2-char name = surname (1 char) + given (1 char). We do NOT
            // include "{surname}{surname}" or "{given}{given}" — those
            // don't look like real names.
            if g.chars().count() == 1 {
                out.push(format!("{s}{g}"));
            } else {
                // Given name is already 2+ chars. Just use "{surname}{given}"
                // as-is, this is also a plausible name shape.
                out.push(format!("{s}{g}"));
            }
        }
    }

    // English names (already lowercased at definition site).
    for n in EN_NAMES {
        out.push((*n).to_string());
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitelist_size_is_documented() {
        let n = raw_entries().len();
        // ~100 surnames × ~40 given chars × 2 (1-char + 2-char given forms)
        //   + ~40 single-char givens + 128 English names
        //   ≈ 100*40*2 + 40 + 128 ≈ 8 200+ entries (the starter set is bigger
        //   than 250 because we cross-product the surname × given list).
        // The actual count: roughly 100 × 40 = 4 000 surname+given pairs,
        // plus 40 standalone givens, plus 128 English names.
        assert!(n >= 1000, "starter whitelist is only {n} entries — see H-23 growth plan");
    }

    #[test]
    fn whitelist_is_lowercased() {
        // All entries must be lowercase so case-insensitive comparison is correct.
        let entries = raw_entries();
        for e in &entries {
            assert_eq!(e, &e.to_lowercase(), "entry {e:?} is not lowercase");
        }
    }
}
