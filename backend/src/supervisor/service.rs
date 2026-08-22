//! Supervisor business logic — orchestrates validation, crypto, repo, alias gen, k-anon.

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use uuid::Uuid;

use crate::aggregation::AggregationService;
use crate::config::ReviewConfig;
use crate::crypto::{aes, hmac, SharedKeyStore};
use crate::discipline::DisciplineRepo;

use super::alias::{AliasGenerator, AliasInput};
use super::dto::{
    CreateSupervisorRequest, CreateSupervisorResponse, PendingReviewEntry, SearchEntry,
    SearchResponse, SupervisorPublicView, SupervisorRequestStatus,
};
use super::error::SupervisorError;
use super::repo::{SupervisorRepo, SupervisorRow};

const K_ANON_THRESHOLD: i32 = 10;

#[derive(Clone)]
pub struct SupervisorService {
    repo: SupervisorRepo,
    keys: SharedKeyStore,
    alias_gen: AliasGenerator,
    review_cfg: ReviewConfig,
    aggregation: AggregationService,
    /// M2: source of per-discipline weights. Optional because
    /// SupervisorService is also constructed in unit tests where we don't
    /// have a discipline repo.
    discipline_repo: Option<DisciplineRepo>,
}

impl SupervisorService {
    pub fn new(
        repo: SupervisorRepo,
        keys: SharedKeyStore,
        alias_gen: AliasGenerator,
        review_cfg: ReviewConfig,
        aggregation: AggregationService,
    ) -> Self {
        Self {
            repo,
            keys,
            alias_gen,
            review_cfg,
            aggregation,
            discipline_repo: None,
        }
    }

    /// Builder: attach a `DisciplineRepo` so the aggregation path can
    /// use the live per-discipline weight map (M2).
    pub fn with_discipline_repo(mut self, drepo: DisciplineRepo) -> Self {
        self.discipline_repo = Some(drepo);
        self
    }

    /// User-submitted request to create a supervisor entry.
    ///
    /// 1. Validate (non-empty, length caps, discipline/college exist)
    /// 2. Compute dedup hash triple (HMAC over submitted_name, discipline, college)
    /// 3. If a mapping already exists for this triple — return its alias
    ///    (status = Deduplicated, no new request created)
    /// 4. Otherwise:
    ///    a. Encrypt the submitted_name (P0)
    ///    b. Generate the alias deterministically
    ///    c. Insert creation request (pending_review)
    ///    d. Return alias + request id (status = PendingReview)
    pub async fn create_request(
        &self,
        submitter_id: Uuid,
        req: CreateSupervisorRequest,
    ) -> Result<CreateSupervisorResponse, SupervisorError> {
        // 1. Validate.
        let submitted_name = req.submitted_name.trim().to_string();
        if submitted_name.is_empty() {
            return Err(SupervisorError::InvalidInput(
                "submitted_name is empty".into(),
            ));
        }
        if submitted_name.len() > 200 {
            return Err(SupervisorError::InvalidInput(
                "submitted_name longer than 200 chars".into(),
            ));
        }
        if req.discipline.is_empty() || req.discipline.len() > 64 {
            return Err(SupervisorError::InvalidInput(
                "discipline must be 1..=64 chars".into(),
            ));
        }
        if req.college.is_empty() || req.college.len() > 64 {
            return Err(SupervisorError::InvalidInput(
                "college must be 1..=64 chars".into(),
            ));
        }
        if !self.repo.discipline_exists(&req.discipline).await? {
            return Err(SupervisorError::UnknownDiscipline(req.discipline));
        }
        if !self.repo.college_exists(&req.college).await? {
            return Err(SupervisorError::UnknownCollege(req.college));
        }

        // 2. Dedup hash triple.
        let hmac_key = self.keys.hmac_key();
        let field_key = self.keys.field_key();
        let name_hash = hmac::hash_str(hmac_key, &submitted_name)?.into_bytes();
        let disc_hash = hmac::hash_str(hmac_key, &req.discipline)?.into_bytes();
        let coll_hash = hmac::hash_str(hmac_key, &req.college)?.into_bytes();

        // 3. Check existing mapping.
        if let Some(existing) = self
            .repo
            .find_mapping_by_dedup(&name_hash, &disc_hash, &coll_hash)
            .await?
        {
            return Ok(CreateSupervisorResponse {
                request_id: Uuid::nil(), // dedup path: no new request created
                alias: existing.alias,
                status: SupervisorRequestStatus::Deduplicated,
                discipline: req.discipline,
                college: req.college,
            });
        }

        // 4. New entry: encrypt, generate alias, queue for review.
        let name_enc = aes::encrypt_str(
            field_key,
            &submitted_name,
            Some(b"supervisor_name_mappings.submitted_name_enc"),
        )?;
        let (alias, _style) = self
            .alias_gen
            .generate(
                AliasInput {
                    submitted_name: &submitted_name,
                    discipline: &req.discipline,
                    college: &req.college,
                },
                0,
            )
            .map_err(|e| SupervisorError::AliasGeneration(e.to_string()))?;

        // 4c. Insert creation request (only — supervisor + mapping inserted
        // on approval). submitted_name stored as plaintext here on purpose
        // (审核员可见, G-15 / G-16).
        let sla = compute_sla(&self.review_cfg, Utc::now());
        let request_id = self
            .repo
            .insert_creation_request(submitter_id, &submitted_name, &req.discipline, &req.college, sla)
            .await?;

        // Pre-stage the encrypted name + hashes so the reviewer approval
        // can write them straight into the mapping. M5b stores these in
        // a "pending mapping" inline (we re-encrypt on approval using
        // the same seed input) — keeping it simple: re-encrypt at approval
        // time. Drop the encrypt call here.
        drop(name_enc);

        // M4 UI (H-54): in dev with REVIEW__MODE=auto_pass, the new request
        // is auto-approved so the web UI's "create supervisor → land on
        // detail page" flow works end-to-end without a manual review step.
        // Production deployments switch REVIEW__MODE=manual to gate this.
        let mut status = SupervisorRequestStatus::PendingReview;
        if self.review_cfg.mode == crate::config::ReviewMode::AutoPass {
            // Re-encrypt the name now (we dropped it above) so the approve
            // call can write the mapping row without re-running the
            // heavy hash triple logic.
            let name_enc = aes::encrypt_str(
                field_key,
                &submitted_name,
                Some(b"supervisor_name_mappings.submitted_name_enc"),
            )?;
            // approve() inserts the supervisor + mapping rows and marks
            // the request resolved. We use the submitter as the reviewer
            // since this is a system auto-approval (audit log still picks
            // up the caller_id from the handler, not here).
            self.approve(request_id, submitter_id, Some("auto-pass"))
                .await?;
            status = SupervisorRequestStatus::Approved;
            drop(name_enc); // owned by approve path
        }

        Ok(CreateSupervisorResponse {
            request_id,
            alias,
            status,
            discipline: req.discipline,
            college: req.college,
        })
    }

    /// Reviewer: approve a creation request. Inserts the supervisor row,
    /// the mapping row (with encrypted name + hashes), marks the request
    /// resolved, and recomputes k-anonymity count for the (disc, coll)
    /// bucket.
    pub async fn approve(
        &self,
        request_id: Uuid,
        reviewer_id: Uuid,
        notes: Option<&str>,
    ) -> Result<Uuid, SupervisorError> {
        let req = self
            .repo
            .find_request_by_id(request_id)
            .await?
            .ok_or(SupervisorError::NotFound)?;
        if req.review_status != "pending_review" {
            return Err(SupervisorError::InvalidInput(format!(
                "request is already {}",
                req.review_status
            )));
        }

        let hmac_key = self.keys.hmac_key();
        let field_key = self.keys.field_key();
        let name_hash = hmac::hash_str(hmac_key, &req.submitted_name)?.into_bytes();
        let disc_hash = hmac::hash_str(hmac_key, &req.discipline)?.into_bytes();
        let coll_hash = hmac::hash_str(hmac_key, &req.college)?.into_bytes();
        let name_enc = aes::encrypt_str(
            field_key,
            &req.submitted_name,
            Some(b"supervisor_name_mappings.submitted_name_enc"),
        )?;
        // Re-derive the alias from the (name, disc, college) triple so the
        // stored public_code matches the deterministic algorithm.
        let (alias, _style) = self
            .alias_gen
            .generate(
                AliasInput {
                    submitted_name: &req.submitted_name,
                    discipline: &req.discipline,
                    college: &req.college,
                },
                0,
            )
            .map_err(|e| SupervisorError::AliasGeneration(e.to_string()))?;

        let _ = notes; // accepted but not stored on the supervisor row
        self.repo
            .approve_request(
                request_id,
                req.submitter_id, // created_by audit field
                reviewer_id,
                &alias,
                &req.discipline,
                &req.college,
                &name_enc,
                &name_hash,
                &disc_hash,
                &coll_hash,
            )
            .await
    }

    /// Reviewer: reject a creation request.
    pub async fn reject(
        &self,
        request_id: Uuid,
        reviewer_id: Uuid,
        notes: Option<&str>,
    ) -> Result<(), SupervisorError> {
        self.repo.reject_request(request_id, reviewer_id, notes).await
    }

    /// List pending reviews.
    pub async fn pending_reviews(&self, limit: i64) -> Result<Vec<PendingReviewEntry>, SupervisorError> {
        self.repo.list_pending_review(limit).await
    }

    /// Public view of a supervisor by alias. Honors k-anonymity:
    /// if the supervisor is approved but k_count < threshold, we return
    /// the row but with `visible: false`.
    ///
    /// The score + radar are computed lazily from approved ratings via
    /// the `AggregationService` (H-33). Until ratings are approved (M6b
    /// adds the sensitivity filter / auto-approve flow), composite_score
    /// will be `None` and radar dims will be `None`.
    pub async fn public_view_by_alias(
        &self,
        alias: &str,
    ) -> Result<Option<SupervisorPublicView>, SupervisorError> {
        let row = match self.repo.find_by_alias(alias).await? {
            Some(r) => r,
            None => return Ok(None),
        };
        let visible = is_public_visible(&row);
        // M2: pull per-discipline weights (if a DisciplineRepo is wired)
        // and pass them to the aggregation. If the repo isn't wired
        // (e.g. in unit tests) or the discipline has no rows yet, fall
        // back to equal weights inside the aggregation service.
        let weights: Option<HashMap<String, f64>> = match &self.discipline_repo {
            Some(dr) => match dr.get_current_weights(&row.discipline).await {
                Ok(rows) if rows.len() == 6 => Some(
                    rows.iter().map(|w| (w.dim.clone(), w.weight)).collect(),
                ),
                Ok(_) => None, // less than 6 rows (shouldn't happen due to bootstrap)
                Err(e) => {
                    return Err(SupervisorError::Database(anyhow::anyhow!(
                        "discipline weights: {e}"
                    )));
                }
            },
            None => None,
        };
        let score = self
            .aggregation
            .compute_with_weights(row.id, weights.as_ref())
            .await
            .map_err(|e| SupervisorError::Database(anyhow::anyhow!("aggregation: {e}")))?;
        let rating_count = score.approved_rating_count as i32;
        Ok(Some(SupervisorPublicView {
            alias: row.public_code,
            discipline: row.discipline,
            college: row.college,
            visible,
            k_anonymity_count: row.k_anonymity_count,
            composite_score: score.composite,
            radar: score.radar,
            rating_count,
            created_at: row.created_at,
        }))
    }

    /// Public search by (discipline, college). Returns k-anon-gated entries
    /// (only visible rows where k_count ≥ threshold). Ordered by
    /// composite_score DESC NULLS LAST, then created_at DESC.
    pub async fn search(
        &self,
        discipline: &str,
        college: &str,
        limit: i64,
        offset: i64,
    ) -> Result<SearchResponse, SupervisorError> {
        // Validate inputs.
        if discipline.is_empty() || discipline.len() > 64 {
            return Err(SupervisorError::InvalidInput("discipline".into()));
        }
        if college.is_empty() || college.len() > 64 {
            return Err(SupervisorError::InvalidInput("college".into()));
        }
        if !(1..=100).contains(&limit) {
            return Err(SupervisorError::InvalidInput("limit must be 1..=100".into()));
        }
        if !(0..=10000).contains(&offset) {
            return Err(SupervisorError::InvalidInput("offset must be 0..=10000".into()));
        }

        // Validate discipline/college exist in lookup tables.
        if !self.repo.discipline_exists(discipline).await? {
            return Err(SupervisorError::UnknownDiscipline(discipline.to_string()));
        }
        if !self.repo.college_exists(college).await? {
            return Err(SupervisorError::UnknownCollege(college.to_string()));
        }

        // Total count of approved+k-anon-passing entries in this bucket.
        let total = self
            .repo
            .count_visible(discipline, college, K_ANON_THRESHOLD)
            .await?;

        // Page of visible rows.
        let rows = self
            .repo
            .list_visible(discipline, college, K_ANON_THRESHOLD, limit, offset)
            .await?;

        // M2: pull the per-discipline weight map once (all entries in
        // this page are in the same discipline) and reuse for every row.
        let weights: Option<HashMap<String, f64>> = match &self.discipline_repo {
            Some(dr) => match dr.get_current_weights(discipline).await {
                Ok(rows) if rows.len() == 6 => Some(
                    rows.iter().map(|w| (w.dim.clone(), w.weight)).collect(),
                ),
                Ok(_) => None,
                Err(e) => {
                    return Err(SupervisorError::Database(anyhow::anyhow!(
                        "discipline weights: {e}"
                    )));
                }
            },
            None => None,
        };

        // Compute score + radar for each entry. M7c will cache this; for
        // now we recompute on every search call.
        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            let score = self
                .aggregation
                .compute_with_weights(row.id, weights.as_ref())
                .await
                .map_err(|e| SupervisorError::Database(anyhow::anyhow!("aggregation: {e}")))?;
            results.push(SearchEntry {
                alias: row.public_code,
                discipline: row.discipline,
                college: row.college,
                composite_score: score.composite,
                radar: score.radar,
                rating_count: score.approved_rating_count as i32,
                created_at: row.created_at,
            });
        }

        Ok(SearchResponse {
            discipline: discipline.to_string(),
            college: college.to_string(),
            total,
            limit,
            offset,
            results,
        })
    }

    /// K-anonymity threshold (for handler to expose in headers / health).
    pub fn k_anonymity_threshold() -> i32 {
        K_ANON_THRESHOLD
    }
}

fn is_public_visible(row: &SupervisorRow) -> bool {
    row.review_status == "approved" && row.k_anonymity_count >= K_ANON_THRESHOLD
}

fn compute_sla(cfg: &ReviewConfig, now: DateTime<Utc>) -> DateTime<Utc> {
    // Simplified SLA: pick the workday SLA if Mon-Fri 09:00-18:00 UTC,
    // else offhours. M5b stub — real production would use a holiday
    // calendar and the submitter's timezone.
    use chrono::{Datelike, Duration, Timelike};
    let weekday = now.weekday().num_days_from_monday(); // 0=Mon, 6=Sun
    let hour = now.hour();
    let on_duty = weekday < 5 && (9..18).contains(&hour);
    let hours = if on_duty {
        cfg.sla_hours_workday
    } else {
        cfg.sla_hours_offhours
    };
    now + Duration::hours(hours as i64)
}
