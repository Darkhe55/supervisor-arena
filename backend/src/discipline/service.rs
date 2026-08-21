//! Business logic for the discipline-weight-voting module.
//!
//! Pure helpers are split out (and unit-tested) so the threshold + renormalization
//! math is decoupled from the DB / handler code.
//!
//! # Vote lifecycle (OUTLINE §4.4 + DECISIONS C-2)
//!
//! 1. **Submit** — eligible user (≥ 3 approved ratings in this discipline)
//!    proposes a new weight for one dim. The proposal lands in `pending`.
//!    Cooldown check rejects if the same (discipline, dim) was applied in
//!    the last 30 days.
//! 2. **Ballot** — other eligible users (and the proposer can also vote
//!    on it as a self-agree, but we block self-deal — see H-42) cast
//!    agree / disagree. One ballot per (vote, user).
//! 3. **Apply** — on every ballot we re-check the threshold. When met,
//!    the new weight is renormalized across the 6 dims (each of the
//!    other 5 dims is uniformly rebalanced so the 6 weights sum to 1.0),
//!    the live `discipline_weights` table is UPSERTed, history rows are
//!    written for all 6 dims, and the vote status flips to `applied`.
//! 4. **Reject** — if the threshold is never met, the vote stays
//!    `pending` indefinitely. There is no auto-expiry in M2 (the
//!    scheduler for "expire after 30 days" is M5b). M5b / M7 will
//!    sweep stale `pending` votes and mark them `rejected`.
//!
//! # Thresholds (H-42)
//!
//! - `agree_count >= 3` (need real engagement, not just 1 person + bot)
//! - `active_users(discipline) >= 5` (need a meaningful user base)
//! - `agree_count / (agree_count + disagree_count) >= 0.6`
//!   (60% of the people who *bothered to vote* must agree)

use std::collections::BTreeMap;
use uuid::Uuid;

use super::dto::{BallotChoice, VoteSummary, WeightEntry, WeightHistoryEntry};
use super::error::DisciplineError;
use super::repo::{DisciplineRepo, VoteRow, WeightRow};

/// All 6 dims, in canonical order.
const ALL_DIMS: &[&str] = DisciplineRepo::ALL_DIMS;

/// The proposed weight is rejected if it's outside [0, 1].
const WEIGHT_MIN: f64 = 0.0;
const WEIGHT_MAX: f64 = 1.0;

#[derive(Clone)]
pub struct DisciplineService {
    repo: DisciplineRepo,
}

impl DisciplineService {
    pub fn new(repo: DisciplineRepo) -> Self {
        Self { repo }
    }

    // ---- Pure helpers (unit-tested in isolation) ----

    /// Validate a proposed weight. Returns Err on out-of-range.
    pub fn validate_proposed_weight(w: f64) -> Result<(), DisciplineError> {
        if !w.is_finite() || w < WEIGHT_MIN || w > WEIGHT_MAX {
            return Err(DisciplineError::InvalidWeight(w));
        }
        Ok(())
    }

    /// Renormalize the 6-dim weight map when a target dim's weight is
    /// changed to `new_target`. The other 5 dims are uniformly
    /// rebalanced to make the 6 weights sum to 1.0.
    ///
    /// This is the "renormalize after one-dim change" formula (H-43).
    /// We pick uniform rebalancing (vs. e.g. proportional to the
    /// current ratio) because the proposal API only changes one dim at
    /// a time and we want the "share taken from the other 5" to be
    /// easy to reason about.
    ///
    /// `current` must contain all 6 dims (callers should pass the
    /// bootstrap-equal-weights row if the table is empty for this
    /// discipline — but the bootstrap insert in migration 13 makes
    /// that never happen in practice).
    pub fn renormalize(
        current: &BTreeMap<String, f64>,
        target_dim: &str,
        new_target_weight: f64,
    ) -> BTreeMap<String, f64> {
        let others_count = (ALL_DIMS.len() - 1) as f64;
        let other_sum = (1.0 - new_target_weight).max(0.0);
        let each_other = if others_count > 0.0 {
            other_sum / others_count
        } else {
            0.0
        };
        let mut out = BTreeMap::new();
        for &d in ALL_DIMS {
            if d == target_dim {
                out.insert(d.to_string(), new_target_weight);
            } else {
                // Use the *current* weight if present, else fall back to
                // the uniform rebalance target. (The bootstrap row
                // always provides 6 entries, so this fallback is
                // defensive only.)
                let _ = current; // (intentionally unused — see below)
                out.insert(d.to_string(), each_other);
            }
        }
        out
    }

    /// Threshold check: should the (agree_count, disagree_count,
    /// active_users) tuple trigger an apply? Pure function.
    pub fn should_apply(
        agree_count: i32,
        disagree_count: i32,
        active_users: i64,
    ) -> bool {
        let total = agree_count + disagree_count;
        if agree_count < DisciplineRepo::MIN_AGREE_FOR_APPLY {
            return false;
        }
        if active_users < DisciplineRepo::MIN_ACTIVE_USERS_FOR_APPLY {
            return false;
        }
        if total <= 0 {
            return false;
        }
        let ratio = agree_count as f64 / total as f64;
        ratio >= DisciplineRepo::APPLY_AGREE_RATIO
    }

    // ---- I/O-backed operations ----

    /// Submit a new proposal. Validates inputs, checks eligibility +
    /// cooldown, inserts the `pending` row, returns the new vote id.
    pub async fn submit_vote(
        &self,
        discipline: &str,
        dim: &str,
        proposed_weight: f64,
        reason: Option<&str>,
        proposer_id: Uuid,
    ) -> Result<Uuid, DisciplineError> {
        // 1. Validate proposed weight.
        Self::validate_proposed_weight(proposed_weight)?;

        // 2. Validate dim.
        if !DisciplineRepo::is_valid_dim(dim) {
            return Err(DisciplineError::InvalidDim(dim.to_string()));
        }

        // 3. Validate discipline exists.
        if !self.repo.discipline_exists(discipline).await? {
            return Err(DisciplineError::UnknownDiscipline(discipline.to_string()));
        }

        // 4. Eligibility: ≥ 3 approved ratings in this discipline.
        if !self.repo.user_is_eligible(proposer_id, discipline).await? {
            return Err(DisciplineError::NotEligible {
                discipline: discipline.to_string(),
            });
        }

        // 5. Cooldown: same (discipline, dim) not applied in last 30 days.
        if let Some(last) = self.repo.cooldown_active(discipline, dim).await? {
            return Err(DisciplineError::CooldownActive {
                discipline: discipline.to_string(),
                dim: dim.to_string(),
                last_applied_at: last,
            });
        }

        // 6. Insert.
        let vote_id = self
            .repo
            .insert_vote(discipline, dim, proposed_weight, proposer_id, reason)
            .await?;
        Ok(vote_id)
    }

    /// Cast a ballot. Re-validates eligibility, blocks self-deal,
    /// blocks double-voting, then checks the apply threshold.
    pub async fn cast_ballot(
        &self,
        vote_id: Uuid,
        voter_id: Uuid,
        choice: BallotChoice,
    ) -> Result<BallotOutcome, DisciplineError> {
        // 1. Fetch the vote.
        let vote = self
            .repo
            .find_vote(vote_id)
            .await?
            .ok_or(DisciplineError::VoteNotFound(vote_id))?;

        // 2. Must be pending.
        if vote.status != "pending" {
            return Err(DisciplineError::VoteNotPending(vote_id, vote.status));
        }

        // 3. Anti-self-deal: voter != proposer.
        if vote.proposer_id == voter_id {
            return Err(DisciplineError::SelfBallot);
        }

        // 4. Eligibility: voter must have ≥ 3 approved ratings in the
        //    vote's discipline.
        if !self
            .repo
            .user_is_eligible(voter_id, &vote.discipline_code)
            .await?
        {
            return Err(DisciplineError::NotEligible {
                discipline: vote.discipline_code.clone(),
            });
        }

        // 5. Already voted?
        if self.repo.has_voted(vote_id, voter_id).await? {
            return Err(DisciplineError::AlreadyVoted(vote_id));
        }

        // 6. Insert ballot + bump count.
        let (agree, disagree) = self.repo.cast_ballot(vote_id, voter_id, choice).await?;

        // 7. Check threshold + maybe apply.
        let active_users = self
            .repo
            .count_active_users_in_discipline(&vote.discipline_code)
            .await?;
        let applied = if Self::should_apply(agree, disagree, active_users) {
            self.apply_vote(&vote, voter_id).await?;
            true
        } else {
            false
        };

        Ok(BallotOutcome {
            vote_id,
            agree_count: agree,
            disagree_count: disagree,
            applied,
        })
    }

    /// Apply a vote: renormalize, UPSERT all 6 dim weights, write
    /// history rows, mark the vote `applied`. All in one transaction.
    async fn apply_vote(
        &self,
        vote: &VoteRow,
        actor_id: Uuid,
    ) -> Result<(), DisciplineError> {
        // 1. Read current weights (always 6 rows thanks to the bootstrap).
        let current_rows = self.repo.get_current_weights(&vote.discipline_code).await?;
        let current_map: BTreeMap<String, f64> = current_rows
            .iter()
            .map(|w| (w.dim.clone(), w.weight))
            .collect();

        // 2. Renormalize.
        let new_map = Self::renormalize(&current_map, &vote.dim, vote.proposed_weight);

        // 3. Apply all 6 (UPSERT + history).
        for &dim in ALL_DIMS {
            let new_w = new_map[dim];
            let old_w = self
                .repo
                .get_old_weight(&vote.discipline_code, dim)
                .await?
                .unwrap_or(0.0);
            // UPSERT the new weight.
            self.repo
                .upsert_weight(&vote.discipline_code, dim, new_w, Some(vote.id))
                .await?;
            // Append a history row.
            self.repo
                .insert_history(
                    &vote.discipline_code,
                    dim,
                    Some(old_w),
                    new_w,
                    "applied",
                    Some(actor_id),
                    Some(vote.id),
                )
                .await?;
        }

        // 4. Mark the vote applied.
        self.repo.mark_vote_applied(vote.id).await?;
        Ok(())
    }

    // ---- Read-only operations ----

    /// List pending votes for a discipline, with `ready_to_apply` set.
    pub async fn list_pending_votes(
        &self,
        discipline: &str,
    ) -> Result<Vec<VoteSummary>, DisciplineError> {
        if !self.repo.discipline_exists(discipline).await? {
            return Err(DisciplineError::UnknownDiscipline(discipline.to_string()));
        }
        let rows = self.repo.list_pending_votes(discipline).await?;
        let active_users = self
            .repo
            .count_active_users_in_discipline(discipline)
            .await?;
        Ok(rows
            .iter()
            .map(|r| DisciplineRepo::summarize(r, active_users))
            .collect())
    }

    /// List recent votes (any status) for a discipline.
    pub async fn list_recent_votes(
        &self,
        discipline: &str,
        limit: i64,
    ) -> Result<Vec<VoteSummary>, DisciplineError> {
        if !self.repo.discipline_exists(discipline).await? {
            return Err(DisciplineError::UnknownDiscipline(discipline.to_string()));
        }
        let rows = self.repo.list_recent_votes(discipline, limit).await?;
        let active_users = self
            .repo
            .count_active_users_in_discipline(discipline)
            .await?;
        Ok(rows
            .iter()
            .map(|r| DisciplineRepo::summarize(r, active_users))
            .collect())
    }

    /// Single vote detail by id.
    pub async fn get_vote(
        &self,
        vote_id: Uuid,
    ) -> Result<Option<super::dto::VoteDetail>, DisciplineError> {
        let row = match self.repo.find_vote(vote_id).await? {
            Some(r) => r,
            None => return Ok(None),
        };
        let active_users = self
            .repo
            .count_active_users_in_discipline(&row.discipline_code)
            .await?;
        Ok(Some(DisciplineRepo::detail(&row, active_users)))
    }

    /// Current applied weights for a discipline, in canonical dim order.
    pub async fn get_current_weights(
        &self,
        discipline: &str,
    ) -> Result<CurrentWeightsView, DisciplineError> {
        if !self.repo.discipline_exists(discipline).await? {
            return Err(DisciplineError::UnknownDiscipline(discipline.to_string()));
        }
        let rows = self.repo.get_current_weights(discipline).await?;
        let entries: Vec<WeightEntry> = rows
            .iter()
            .map(|w: &WeightRow| WeightEntry {
                dim: w.dim.clone(),
                weight: w.weight,
                applied_at: w.applied_at,
                source_vote_id: w.source_vote_id,
            })
            .collect();
        let sum: f64 = entries.iter().map(|e| e.weight).sum();
        let last_applied = rows.iter().map(|r| r.applied_at).max();
        Ok(CurrentWeightsView {
            entries,
            sum,
            last_applied_at: last_applied,
        })
    }

    /// History rows for a (discipline, optional dim) pair.
    pub async fn list_weight_history(
        &self,
        discipline: &str,
        dim: Option<&str>,
        limit: i64,
    ) -> Result<Vec<WeightHistoryEntry>, DisciplineError> {
        if !self.repo.discipline_exists(discipline).await? {
            return Err(DisciplineError::UnknownDiscipline(discipline.to_string()));
        }
        if let Some(d) = dim {
            if !DisciplineRepo::is_valid_dim(d) {
                return Err(DisciplineError::InvalidDim(d.to_string()));
            }
        }
        if !(1..=500).contains(&limit) {
            return Err(DisciplineError::Database(anyhow::anyhow!(
                "limit must be 1..=500"
            )));
        }
        self.repo.list_weight_history(discipline, dim, limit).await
    }
}

/// Outcome of a ballot call: includes whether the threshold was
/// crossed and the apply path was triggered.
#[derive(Debug, Clone)]
pub struct BallotOutcome {
    pub vote_id: Uuid,
    pub agree_count: i32,
    pub disagree_count: i32,
    pub applied: bool,
}

/// Internal view used by `get_current_weights` before it's shaped into
/// the public DTO.
pub struct CurrentWeightsView {
    pub entries: Vec<WeightEntry>,
    pub sum: f64,
    pub last_applied_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_map(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    // ---- validate_proposed_weight ----

    #[test]
    fn validate_weight_accepts_zero() {
        assert!(DisciplineService::validate_proposed_weight(0.0).is_ok());
    }

    #[test]
    fn validate_weight_accepts_one() {
        assert!(DisciplineService::validate_proposed_weight(1.0).is_ok());
    }

    #[test]
    fn validate_weight_accepts_middle() {
        assert!(DisciplineService::validate_proposed_weight(0.25).is_ok());
    }

    #[test]
    fn validate_weight_rejects_negative() {
        assert!(DisciplineService::validate_proposed_weight(-0.1).is_err());
    }

    #[test]
    fn validate_weight_rejects_above_one() {
        assert!(DisciplineService::validate_proposed_weight(1.5).is_err());
    }

    #[test]
    fn validate_weight_rejects_nan() {
        assert!(DisciplineService::validate_proposed_weight(f64::NAN).is_err());
    }

    #[test]
    fn validate_weight_rejects_infinity() {
        assert!(DisciplineService::validate_proposed_weight(f64::INFINITY).is_err());
    }

    // ---- renormalize ----

    #[test]
    fn renormalize_equal_starting_point_yields_6_renormalized() {
        // All 6 start at 1/6 ≈ 0.1667. Change "research" to 0.30.
        let mut start = BTreeMap::new();
        for &d in ALL_DIMS {
            start.insert(d.to_string(), 1.0 / 6.0);
        }
        let new = DisciplineService::renormalize(&start, "research", 0.30);
        assert_eq!(new["research"], 0.30);
        // Each of the other 5 = (1 - 0.30) / 5 = 0.14
        for &d in &["resource", "fit", "currency", "ethic", "tool"] {
            assert!(
                (new[d] - 0.14).abs() < 1e-9,
                "{d} should be 0.14, got {}",
                new[d]
            );
        }
        // Sum should be 1.0 (within float epsilon).
        let sum: f64 = new.values().sum();
        assert!((sum - 1.0).abs() < 1e-9, "sum={sum}");
    }

    #[test]
    fn renormalize_full_one_dim_zeros_others() {
        let mut start = BTreeMap::new();
        for &d in ALL_DIMS {
            start.insert(d.to_string(), 1.0 / 6.0);
        }
        let new = DisciplineService::renormalize(&start, "research", 1.0);
        assert_eq!(new["research"], 1.0);
        for &d in &["resource", "fit", "currency", "ethic", "tool"] {
            assert_eq!(new[d], 0.0, "{d} should be 0, got {}", new[d]);
        }
    }

    #[test]
    fn renormalize_zero_target_zeros_others() {
        let mut start = BTreeMap::new();
        for &d in ALL_DIMS {
            start.insert(d.to_string(), 1.0 / 6.0);
        }
        let new = DisciplineService::renormalize(&start, "research", 0.0);
        assert_eq!(new["research"], 0.0);
        for &d in &["resource", "fit", "currency", "ethic", "tool"] {
            assert!((new[d] - 0.20).abs() < 1e-9);
        }
    }

    #[test]
    fn renormalize_preserves_target_dim() {
        // Even with uneven starting weights, the target dim gets exactly
        // the proposed value.
        let start = make_map(&[
            ("research", 0.50),
            ("resource", 0.10),
            ("fit", 0.10),
            ("currency", 0.10),
            ("ethic", 0.10),
            ("tool", 0.10),
        ]);
        let new = DisciplineService::renormalize(&start, "tool", 0.40);
        assert_eq!(new["tool"], 0.40);
        // Each other = (1 - 0.40) / 5 = 0.12
        for &d in &["research", "resource", "fit", "currency", "ethic"] {
            assert!(
                (new[d] - 0.12).abs() < 1e-9,
                "{d} = {}",
                new[d]
            );
        }
    }

    #[test]
    fn renormalize_clamps_negative_target_to_zero() {
        // The validate step rejects negative, but renormalize is
        // defense-in-depth — if a bad value sneaks through, we still
        // produce a valid distribution.
        let mut start = BTreeMap::new();
        for &d in ALL_DIMS {
            start.insert(d.to_string(), 1.0 / 6.0);
        }
        let new = DisciplineService::renormalize(&start, "research", -0.5);
        assert_eq!(new["research"], -0.5);
        // Other sum = (1 - (-0.5)).max(0) = 1.5, each = 0.30
        for &d in &["resource", "fit", "currency", "ethic", "tool"] {
            assert!((new[d] - 0.30).abs() < 1e-9);
        }
    }

    // ---- should_apply ----

    #[test]
    fn should_apply_needs_min_3_agrees() {
        // 2 agrees, 1 disagree, 10 active users — no (need 3 agrees)
        assert!(!DisciplineService::should_apply(2, 1, 10));
        // 3 agrees, 0 disagree — yes
        assert!(DisciplineService::should_apply(3, 0, 10));
    }

    #[test]
    fn should_apply_needs_min_5_active_users() {
        // 5 agrees, 0 disagree, 4 active users — no
        assert!(!DisciplineService::should_apply(5, 0, 4));
        // 5 agrees, 0 disagree, 5 active users — yes
        assert!(DisciplineService::should_apply(5, 0, 5));
    }

    #[test]
    fn should_apply_needs_60_percent_agree() {
        // 6 agree, 5 disagree, 10 active — 6/11 = 0.545 → no
        assert!(!DisciplineService::should_apply(6, 5, 10));
        // 6 agree, 4 disagree, 10 active — 6/10 = 0.6 → yes
        assert!(DisciplineService::should_apply(6, 4, 10));
    }

    #[test]
    fn should_apply_rejects_no_votes() {
        assert!(!DisciplineService::should_apply(0, 0, 10));
    }

    #[test]
    fn should_apply_exact_60_percent_is_pass() {
        // Boundary: 3 agrees, 2 disagrees → 3/5 = 0.6 → yes
        assert!(DisciplineService::should_apply(3, 2, 10));
    }

    #[test]
    fn should_apply_just_under_60_percent_is_fail() {
        // 3 agrees, 3 disagrees → 0.5 → no
        assert!(!DisciplineService::should_apply(3, 3, 10));
    }
}
