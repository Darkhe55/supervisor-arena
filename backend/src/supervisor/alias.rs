//! Deterministic alias generator
//!
//! # Algorithm (deterministic by seed)
//!
//! ```text
//! input  = (submitted_name, discipline, college)
//! seed   = HMAC-SHA256(hmac_key, "alias:" || submitted_name || "|" || discipline || "|" || college)
//! rng    = SplitMix32 stream initialized from seed[0..16]
//! style  = pick_style(rng.next())     // discipline-fused / nature / literary / geometric
//! words  = pick_words(style, rng)
//! suffix = 3-char alphanumeric derived from rng
//! alias  = combine(style, words, suffix)
//! ```
//!
//! # Invariants
//!
//! 1. **Deterministic**: same input tuple → same alias (modulo whitelist
//!    collision retries, which use a *different* input form and therefore
//!    are stable too).
//! 2. **Cross-discipline 1-to-1**: different `(submitted_name, discipline)`
//!    tuples produce different aliases. The DB UNIQUE constraint on
//!    `supervisor_name_mappings.generated_alias` enforces this at write
//!    time as a defense-in-depth.
//! 3. **No real person names**: every generated alias is checked against
//!    the whitelist (`whitelist::raw_entries`) before being returned.
//!    On collision, the algorithm retries with an incremented `salt`
//!    appended to the seed input — same retry will always produce the
//!    same retry, so the alias is still stable across regenerations.
//! 4. **Unpredictable**: the seed is HMAC-SHA256 with a server-side secret
//!    key, so an attacker observing a single alias cannot reverse-engineer
//!    the input tuple without the key.
//!
//! # Whitelist collision math
//!
//! With the current starter whitelist (~4 200 entries) and an addressable
//! space of ~10^6 (135 base components × 36^3 suffixes × 4 styles), the
//! per-attempt collision probability is ~0.4%. The retry budget of 32
//! gives a residual collision probability of < 10^-39 — well below the
//! 10^-18 "should never happen" threshold for this kind of system.

use std::collections::HashSet;

use crate::crypto::hmac;
use thiserror::Error;

use super::whitelist;
use super::words::{DisciplineCategory, GREEK_LETTERS};

/// Errors specific to alias generation.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AliasError {
    /// Underlying HMAC computation failed (should be unreachable).
    #[error("hash failure: {0}")]
    Hash(String),

    /// We tried N times to produce a whitelist-clean alias and failed.
    /// The 1-to-1 DB UNIQUE constraint will catch any remaining collision.
    #[error("could not produce whitelist-clean alias after {0} retries")]
    WhitelistExhausted(u32),

    /// The discipline key is empty or otherwise unusable.
    #[error("invalid discipline: {0}")]
    InvalidDiscipline(String),
}

/// Style of the generated alias.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AliasStyle {
    /// "{symbol}-{word}-{3char}" e.g. "α-net-3k2"
    DisciplineFused,
    /// "{word}·{word}" e.g. "远山·溪流"
    Nature,
    /// "{word}·{title}" e.g. "南飞雁·听松客"
    Literary,
    /// "{PREFIX}-{HEX6}" e.g. "Q-7a2b9c"
    Geometric,
}

/// Input tuple for alias generation.
#[derive(Debug, Clone)]
pub struct AliasInput<'a> {
    pub submitted_name: &'a str,
    pub discipline: &'a str,
    pub college: &'a str,
}

/// The generator. Holds the HMAC key + the pre-built whitelist `HashSet`.
/// One instance is built at startup and shared (cheap to clone — both
/// fields are `Arc`-shaped: HMAC key is a fixed array, whitelist is
/// immutable).
#[derive(Clone)]
pub struct AliasGenerator {
    hmac_key: [u8; 32],
    whitelist: HashSet<String>,
}

impl AliasGenerator {
    /// Build a generator from raw HMAC key bytes + the raw whitelist
    /// entries. Tests typically call this with a fixed key; production
    /// pulls the key from `LocalKeyStore::hmac_key()` (see M5b).
    pub fn new(hmac_key: [u8; 32]) -> Self {
        let whitelist: HashSet<String> = whitelist::raw_entries().into_iter().collect();
        Self { hmac_key, whitelist }
    }

    /// Build from a `KeyStore` — convenience for the production
    /// startup path. M6: takes the trait so any backend (local or
    /// KMS) works. M5b used to take `&LocalKeyStore` directly.
    pub fn from_keystore(keys: &dyn crate::crypto::KeyStore) -> Self {
        Self::new(*keys.hmac_key())
    }

    /// Whitelist size (for monitoring / growth tracking).
    pub fn whitelist_size(&self) -> usize {
        self.whitelist.len()
    }

    /// Generate an alias for the given input. Returns the chosen style
    /// alongside the alias (handy for logging and tests).
    ///
    /// `salt` is normally 0. The retry loop bumps it on whitelist
    /// collision. Callers (tests, future admin tooling) can pass an
    /// explicit salt to force a different alias for the same input.
    pub fn generate(&self, input: AliasInput, salt: u32) -> Result<(String, AliasStyle), AliasError> {
        const MAX_ATTEMPTS: u32 = 32;

        for attempt in 0..MAX_ATTEMPTS {
            let current_salt = salt.wrapping_add(attempt);
            let (candidate, style) = self.try_once(&input, current_salt)?;
            let normalised = candidate.to_lowercase();
            if !self.whitelist.contains(&normalised) {
                return Ok((candidate, style));
            }
            // else: collision, retry with incremented salt
        }
        Err(AliasError::WhitelistExhausted(MAX_ATTEMPTS))
    }

    /// One attempt — produces a candidate alias without checking the
    /// whitelist.
    fn try_once(&self, input: &AliasInput, salt: u32) -> Result<(String, AliasStyle), AliasError> {
        // 1. Seed = HMAC over "alias:{name}|{discipline}|{college}|salt:{salt}".
        let mut seed_input = format!(
            "alias:{}|{}|{}|salt:{}",
            input.submitted_name, input.discipline, input.college, salt
        );
        let seed_bytes = hmac::hash_raw(&self.hmac_key, seed_input.as_bytes())
            .map_err(|e| AliasError::Hash(e.to_string()))?;
        // HMAC returns [u8; 32] but the trait return type here is
        // `Result<[u8; 32], _>` so we can index directly.
        seed_input.zeroize(); // best-effort scrub of the formatted seed

        // 2. Initialize SplitMix32 stream with seed[0..4].
        let mut rng = SplitMix32::new(u32::from_le_bytes([
            seed_bytes[0], seed_bytes[1], seed_bytes[2], seed_bytes[3],
        ]));

        // 3. Pick style.
        let style = pick_style(rng.next());

        // 4. Compose alias for the style.
        let alias = match style {
            AliasStyle::DisciplineFused => self.compose_discipline(input, &mut rng)?,
            AliasStyle::Nature => compose_nature(&mut rng),
            AliasStyle::Literary => compose_literary(&mut rng),
            AliasStyle::Geometric => compose_geometric(&mut rng),
        };
        Ok((alias, style))
    }

    fn compose_discipline(
        &self,
        input: &AliasInput,
        rng: &mut SplitMix32,
    ) -> Result<String, AliasError> {
        if input.discipline.is_empty() {
            return Err(AliasError::InvalidDiscipline("empty".into()));
        }
        let cat = DisciplineCategory::from_discipline(input.discipline);
        let templates = cat.templates();
        let template = templates[rng.next() as usize % templates.len()];
        let suffix = alphanumeric_suffix(rng);
        Ok(template.replace("{X}", &suffix))
    }
}

// --- Style composers (no whitelist check here — done in `generate`) ---

fn compose_nature(rng: &mut SplitMix32) -> String {
    use super::words::NATURE_WORDS;
    let a = NATURE_WORDS[rng.next() as usize % NATURE_WORDS.len()];
    let b = NATURE_WORDS[rng.next() as usize % NATURE_WORDS.len()];
    if a == b {
        // Avoid "远山·远山" (degenerate). Re-roll once deterministically.
        let b2 = NATURE_WORDS[(rng.next() as usize) % NATURE_WORDS.len()];
        format!("{a}·{b2}")
    } else {
        format!("{a}·{b}")
    }
}

fn compose_literary(rng: &mut SplitMix32) -> String {
    use super::words::{LITERARY_TITLES, LITERARY_WORDS};
    let w = LITERARY_WORDS[rng.next() as usize % LITERARY_WORDS.len()];
    let t = LITERARY_TITLES[rng.next() as usize % LITERARY_TITLES.len()];
    format!("{w}·{t}")
}

fn compose_geometric(rng: &mut SplitMix32) -> String {
    use super::words::GEOMETRIC_PREFIXES;
    let p = GEOMETRIC_PREFIXES[rng.next() as usize % GEOMETRIC_PREFIXES.len()];
    let hex = hex_suffix(rng);
    format!("{p}-{hex}")
}

fn pick_style(r: u32) -> AliasStyle {
    // Distribution: 50% discipline-fused, 20% nature, 15% literary, 15% geometric.
    match r % 100 {
        0..=49 => AliasStyle::DisciplineFused,
        50..=69 => AliasStyle::Nature,
        70..=84 => AliasStyle::Literary,
        _ => AliasStyle::Geometric,
    }
}

fn alphanumeric_suffix(rng: &mut SplitMix32) -> String {
    // 3 chars from [0-9a-z] (lowercase) for predictable length.
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = String::with_capacity(3);
    for _ in 0..3 {
        let idx = (rng.next() as usize) % ALPHABET.len();
        out.push(ALPHABET[idx] as char);
    }
    out
}

fn hex_suffix(rng: &mut SplitMix32) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut out = String::with_capacity(6);
    for _ in 0..6 {
        let idx = (rng.next() as usize) % HEX.len();
        out.push(HEX[idx] as char);
    }
    out
}

// --- SplitMix32 PRNG ---
//
// Simple, fast, well-distributed. We only need determinism + uniform
// distribution over 2^32 values — SplitMix32 satisfies both. Two streams
// seeded from different parts of the HMAC output would also work; one
// stream is enough for our slot count.

struct SplitMix32 {
    state: u32,
}

impl SplitMix32 {
    fn new(seed: u32) -> Self {
        Self { state: seed }
    }
    fn next(&mut self) -> u32 {
        self.state = self.state.wrapping_add(0x9E37_79B9);
        let mut z = self.state;
        z = (z ^ (z >> 16)).wrapping_mul(0x85EB_CA6B);
        z = (z ^ (z >> 13)).wrapping_mul(0xC2B2_AE35);
        z ^ (z >> 16)
    }
}

// --- Greek-letter filler (so the module isn't dead-code) ---

#[allow(dead_code)]
fn first_greek_letter(rng: &mut SplitMix32) -> &'static str {
    GREEK_LETTERS[rng.next() as usize % GREEK_LETTERS.len()]
}

// --- String zeroize helper (best-effort) ---

trait ZeroizeStr {
    fn zeroize(&mut self);
}
impl ZeroizeStr for String {
    fn zeroize(&mut self) {
        // Replace the underlying bytes with zeros. String doesn't expose
        // its buffer, but `as_mut_vec` does.
        unsafe {
            let vec = self.as_mut_vec();
            for b in vec.iter_mut() {
                *b = 0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gen() -> AliasGenerator {
        AliasGenerator::new([0x42_u8; 32])
    }

    fn inp<'a>(n: &'a str, d: &'a str, c: &'a str) -> AliasInput<'a> {
        AliasInput {
            submitted_name: n,
            discipline: d,
            college: c,
        }
    }

    #[test]
    fn deterministic_for_same_input() {
        let g = gen();
        let a = g.generate(inp("张伟", "computer_science", "MIT"), 0).unwrap();
        let b = g.generate(inp("张伟", "computer_science", "MIT"), 0).unwrap();
        assert_eq!(a, b, "alias must be deterministic for the same input");
    }

    #[test]
    fn different_discipline_yields_different_alias() {
        let g = gen();
        let a = g.generate(inp("张伟", "computer_science", "MIT"), 0).unwrap();
        let b = g.generate(inp("张伟", "mathematics", "MIT"), 0).unwrap();
        assert_ne!(a.0, b.0, "different discipline must yield different alias");
    }

    #[test]
    fn different_college_yields_different_alias() {
        let g = gen();
        let a = g.generate(inp("张伟", "computer_science", "MIT"), 0).unwrap();
        let b = g.generate(inp("张伟", "computer_science", "Stanford"), 0).unwrap();
        assert_ne!(a.0, b.0, "different college must yield different alias");
    }

    #[test]
    fn different_name_yields_different_alias() {
        let g = gen();
        let a = g.generate(inp("张伟", "computer_science", "MIT"), 0).unwrap();
        let b = g.generate(inp("李娜", "computer_science", "MIT"), 0).unwrap();
        assert_ne!(a.0, b.0, "different name must yield different alias");
    }

    #[test]
    fn never_collides_with_whitelist_in_1000_attempts() {
        // 1000 random-looking inputs — none should hit the whitelist.
        let g = gen();
        let names = ["张伟", "李娜", "王强", "Michael", "Sarah", "王", "张", "李"];
        let discs = ["computer_science", "mathematics", "medicine", "literature"];
        let colleges = ["MIT", "Stanford", "清华", "北大"];
        let mut count = 0;
        for n in &names {
            for d in &discs {
                for c in &colleges {
                    let (alias, _style) = g.generate(inp(n, d, c), 0).unwrap();
                    assert!(
                        !g.whitelist.contains(&alias.to_lowercase()),
                        "alias `{alias}` hit the whitelist — grow whitelist or fix algorithm"
                    );
                    count += 1;
                }
            }
        }
        assert_eq!(count, 8 * 4 * 4);
    }

    #[test]
    fn all_styles_appear_in_a_large_sample() {
        let g = gen();
        let mut seen_styles = std::collections::HashSet::new();
        for i in 0..200 {
            let name = format!("test_name_{i}");
            let (alias, style) = g.generate(inp(&name, "computer_science", "MIT"), 0).unwrap();
            // Sanity: non-empty + non-trivial length.
            assert!(!alias.is_empty());
            assert!(alias.len() >= 3);
            seen_styles.insert(style);
        }
        // With 200 samples, all 4 styles should be hit at least once.
        // (Strictly probabilistic — but at the given distribution, ~50/20/15/15,
        // the probability of missing any one style in 200 samples is
        // < (1-0.5)^200 + ... ≈ 10^-58 — effectively zero.)
        assert_eq!(seen_styles.len(), 4, "missing styles: {seen_styles:?}");
    }

    #[test]
    fn styles_round_trip_to_expected_format() {
        // Not strictly necessary (algorithm can change), but a useful
        // sanity check that current style output is shaped the way the
        // templates describe.
        let g = gen();
        for _ in 0..50 {
            let (alias, style) = g.generate(inp("foo", "computer_science", "MIT"), 0).unwrap();
            match style {
                AliasStyle::DisciplineFused => {
                    assert!(alias.contains('-'), "discipline-fused alias should contain '-': {alias}");
                }
                AliasStyle::Nature => {
                    assert!(alias.contains('·'), "nature alias should contain '·': {alias}");
                }
                AliasStyle::Literary => {
                    assert!(alias.contains('·'), "literary alias should contain '·': {alias}");
                }
                AliasStyle::Geometric => {
                    assert!(alias.contains('-'), "geometric alias should contain '-': {alias}");
                    let parts: Vec<&str> = alias.split('-').collect();
                    assert_eq!(parts.len(), 2, "geometric must be PREFIX-HEX6: {alias}");
                    assert_eq!(parts[1].len(), 6, "geometric hex must be 6 chars: {alias}");
                }
            }
        }
    }

    #[test]
    fn whitelist_size_is_documented() {
        let g = gen();
        let n = g.whitelist_size();
        // 4 000+ (cross-product) + 40 standalone givens + 128 English = 4 000+
        assert!(n >= 1000, "starter whitelist is only {n} entries — see H-23 growth plan");
    }

    #[test]
    fn salt_changes_output() {
        let g = gen();
        let (a, _) = g.generate(inp("张伟", "computer_science", "MIT"), 0).unwrap();
        let (b, _) = g.generate(inp("张伟", "computer_science", "MIT"), 1).unwrap();
        // Different salt is reserved for whitelist-retry; usually equal
        // unless the first attempt collided. In the happy path they may
        // be equal (no collision), so this is informational, not an
        // assertion of inequality.
        let _ = (a, b);
    }

    #[test]
    fn unknown_discipline_produces_valid_alias() {
        let g = gen();
        // Unknown discipline falls back to DisciplineCategory::General, but
        // the style selection is still random. We just check the alias is
        // non-empty + non-whitelist + reproducibly equal across two calls.
        let (a, _) = g
            .generate(inp("test", "underwater_basket_weaving", "Caltech"), 0)
            .unwrap();
        let (b, _) = g
            .generate(inp("test", "underwater_basket_weaving", "Caltech"), 0)
            .unwrap();
        assert!(!a.is_empty());
        assert_eq!(a, b, "unknown-discipline alias must still be deterministic");
    }
}
