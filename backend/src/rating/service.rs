//! Rating business logic

use std::sync::Arc;
use uuid::Uuid;

use crate::account::repo::AccountRepo;
use crate::config::ReviewConfig;
use crate::crypto::{aes, LocalKeyStore};
use crate::supervisor::repo::SupervisorRepo;

use super::dto::{
    MyRatingEntry, MyRatingsResponse, RatingOutcome, RatingResponse, SubmitRatingRequest,
};
use super::error::RatingError;
use super::repo::{RatingQueueEntry, RatingRepo, RatingRow};
use super::sensitivity::{classify, SensitivityFlag};
use super::DIMS;

const ADDITIONAL_LEVELS: &[&str] = &["L1", "L2", "L3", "L4"];

#[derive(Clone)]
pub struct RatingService {
    rating_repo: RatingRepo,
    /// Held for Phase 7 aggregation hooks; unused in M6.
    #[allow(dead_code)]
    supervisor_repo: SupervisorRepo,
    account_repo: AccountRepo,
    keys: Arc<LocalKeyStore>,
    review_cfg: ReviewConfig,
}

impl RatingService {
    pub fn new(
        rating_repo: RatingRepo,
        supervisor_repo: SupervisorRepo,
        account_repo: AccountRepo,
        keys: Arc<LocalKeyStore>,
        review_cfg: ReviewConfig,
    ) -> Self {
        Self {
            rating_repo,
            supervisor_repo,
            account_repo,
            keys,
            review_cfg,
        }
    }

    /// Submit a single-dimension rating for a supervisor.
    ///
    /// Flow:
    /// 1. Validate input (dim, value, additional_level)
    /// 2. Resolve supervisor by alias → check approved
    /// 3. Snapshot rater's discipline_hash
    /// 4. If existing current rating for (account, supervisor, dim):
    ///    insert new row + mark old as superseded (B-9)
    /// 5. Else: just insert
    pub async fn submit(
        &self,
        account_id: Uuid,
        supervisor_alias: &str,
        req: SubmitRatingRequest,
    ) -> Result<RatingResponse, RatingError> {
        // 1. Validate.
        Self::validate_dim(&req.dim)?;
        Self::validate_value(req.value)?;
        if let Some(level) = &req.additional_level {
            Self::validate_additional_level(level)?;
        }
        for url in &req.evidence {
            Self::validate_evidence_url(url)?;
        }

        // 2. Resolve supervisor.
        let sup = self
            .rating_repo
            .find_supervisor_by_alias(supervisor_alias)
            .await?
            .ok_or_else(|| RatingError::SupervisorNotFound(supervisor_alias.to_string()))?;
        if sup.review_status != "approved" {
            return Err(RatingError::SupervisorNotApproved(supervisor_alias.into()));
        }

        // 3. Discipline snapshot.
        let discipline_hash = self
            .account_repo
            .find_discipline_hash(account_id)
            .await
            .map_err(|e| RatingError::Database(anyhow::anyhow!("account lookup: {e}")))?
            .ok_or_else(|| {
                RatingError::Database(anyhow::anyhow!("account {account_id} not found"))
            })?;

        // 4. Encrypt optional P2 fields.
        let field_key = self.keys.field_key();
        let dim_additional_enc = match &req.dim_additional {
            Some(s) => Some(aes::encrypt_str(
                field_key,
                s,
                Some(b"ratings.dim_additional_enc"),
            )?),
            None => None,
        };
        let overall_additional_enc = match &req.overall_additional {
            Some(s) => Some(aes::encrypt_str(
                field_key,
                s,
                Some(b"ratings.overall_additional_enc"),
            )?),
            None => None,
        };

        // 5. B-9: check existing current rating + submit (atomic).
        let existing = self
            .rating_repo
            .find_current_rating(account_id, sup.id, &req.dim)
            .await?;

        let (new_id, outcome) = if let Some(_old_id) = existing {
            let id = self
                .rating_repo
                .submit_with_supersede(
                    account_id,
                    sup.id,
                    &req.dim,
                    req.value,
                    &discipline_hash,
                    dim_additional_enc.as_deref(),
                    overall_additional_enc.as_deref(),
                    req.additional_level.as_deref(),
                    &req.evidence,
                    Some(_old_id),
                )
                .await?;
            (id, RatingOutcome::Updated)
        } else {
            let id = self
                .rating_repo
                .submit_with_supersede(
                    account_id,
                    sup.id,
                    &req.dim,
                    req.value,
                    &discipline_hash,
                    dim_additional_enc.as_deref(),
                    overall_additional_enc.as_deref(),
                    req.additional_level.as_deref(),
                    &req.evidence,
                    None,
                )
                .await?;
            (id, RatingOutcome::Created)
        };

        // M6b: sensitivity detection + auto-approval. Run the classifier
        // on the optional additional text. If REVIEW__MODE = auto_pass AND
        // no P0 detected, the row is set to 'approved' immediately
        // (P1 / P2 / Clean all auto-approve). P0 → stays pending for
        // human review.
        let mut highest = SensitivityFlag::Clean;
        if let Some(s) = &req.dim_additional {
            highest = highest.max(classify(s));
        }
        if let Some(s) = &req.overall_additional {
            highest = highest.max(classify(s));
        }
        let auto = matches!(
            self.review_cfg.mode,
            crate::config::ReviewMode::AutoPass
        );
        let _ = self
            .rating_repo
            .apply_sensitivity(new_id, highest.as_db_str(), auto, None)
            .await;

        Ok(RatingResponse {
            rating_id: new_id,
            supervisor_id: sup.id,
            dim: req.dim,
            value: req.value,
            outcome,
            created_at: chrono::Utc::now(),
        })
    }

    /// List this account's existing ratings for the supervisor (all 6 dims).
    pub async fn my_ratings(
        &self,
        account_id: Uuid,
        supervisor_alias: &str,
    ) -> Result<MyRatingsResponse, RatingError> {
        let sup = self
            .rating_repo
            .find_supervisor_by_alias(supervisor_alias)
            .await?
            .ok_or_else(|| RatingError::SupervisorNotFound(supervisor_alias.into()))?;

        let rows = self
            .rating_repo
            .list_my_ratings(account_id, sup.id)
            .await?;

        let entries: Vec<MyRatingEntry> = rows
            .into_iter()
            .map(|r: RatingRow| MyRatingEntry {
                rating_id: r.id,
                dim: r.dim,
                value: r.value,
                created_at: r.created_at,
                superseded_by: r.superseded_by,
            })
            .collect();

        Ok(MyRatingsResponse {
            supervisor_id: sup.id,
            supervisor_alias: supervisor_alias.to_string(),
            ratings: entries,
        })
    }

    /// M6b: reviewer manually approves a pending rating.
    pub async fn approve_rating(
        &self,
        rating_id: Uuid,
        reviewer_id: Uuid,
    ) -> Result<(), RatingError> {
        self.rating_repo
            .mark_approved(rating_id, reviewer_id)
            .await
    }

    /// M6b: reviewer manually rejects a pending rating.
    pub async fn reject_rating(
        &self,
        rating_id: Uuid,
        reviewer_id: Uuid,
        notes: Option<&str>,
    ) -> Result<(), RatingError> {
        self.rating_repo
            .mark_rejected(rating_id, reviewer_id, notes)
            .await
    }

    /// M6b: list pending ratings (oldest first). For the reviewer queue.
    pub async fn pending_ratings(
        &self,
        limit: i64,
    ) -> Result<Vec<RatingQueueEntry>, RatingError> {
        self.rating_repo.list_pending_ratings(limit).await
    }

    // --- Validators (pure, no I/O) ---

    pub fn validate_dim(dim: &str) -> Result<(), RatingError> {
        if DIMS.contains(&dim) {
            Ok(())
        } else {
            Err(RatingError::InvalidDim(dim.to_string()))
        }
    }

    pub fn validate_value(v: i16) -> Result<(), RatingError> {
        if (-100..=100).contains(&v) {
            Ok(())
        } else {
            Err(RatingError::InvalidValue(v))
        }
    }

    pub fn validate_additional_level(level: &str) -> Result<(), RatingError> {
        if ADDITIONAL_LEVELS.contains(&level) {
            Ok(())
        } else {
            Err(RatingError::InvalidAdditionalLevel(level.to_string()))
        }
    }

    pub fn validate_evidence_url(url: &str) -> Result<(), RatingError> {
        // Minimal URL check: starts with http:// or https://, no whitespace,
        // reasonable length.
        if url.is_empty() || url.len() > 2048 {
            return Err(RatingError::InvalidEvidence("empty or > 2048 chars".into()));
        }
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(RatingError::InvalidEvidence(
                "must start with http:// or https://".into(),
            ));
        }
        if url.chars().any(|c| c.is_whitespace()) {
            return Err(RatingError::InvalidEvidence("contains whitespace".into()));
        }
        Ok(())
    }
}
