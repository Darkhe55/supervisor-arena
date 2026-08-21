//! Aggregation service: pure functions to compute composite_score + radar
//!
//! These are intentionally decoupled from the DB (they take `&[ApprovedRating]`
//! as input) so we can unit-test them without a live DB.

use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

use super::error::AggregationError;
use super::repo::{ApprovedRating, RatingRepo};
#[allow(unused_imports)]
use super::RADAR_DIMS;

/// Radar dimensions for the public view. Always has all 6 keys; values
/// are `None` for dimensions with no approved ratings.
#[derive(Debug, Clone, Serialize)]
pub struct RadarDimensions {
    pub research: Option<f64>,
    pub resource: Option<f64>,
    pub fit: Option<f64>,
    pub currency: Option<f64>,
    pub ethic: Option<f64>,
    pub tool: Option<f64>,
}

/// Aggregated score for one supervisor. `None` everywhere if no approved
/// ratings exist yet (pre-launch state).
#[derive(Debug, Clone)]
pub struct SupervisorScore {
    pub composite: Option<f64>,
    pub radar: RadarDimensions,
    pub approved_rating_count: usize,
}

#[derive(Clone)]
pub struct AggregationService {
    rating_repo: RatingRepo,
}

impl AggregationService {
    pub fn new(rating_repo: RatingRepo) -> Self {
        Self { rating_repo }
    }

    /// Compute the score for a supervisor, with no per-discipline
    /// weighting (equal weights 1/6 for every dim). This is the M7
    /// default and the right call when the supervisor's discipline
    /// isn't known or has no `discipline_weights` row yet.
    pub async fn compute(
        &self,
        supervisor_id: Uuid,
    ) -> Result<SupervisorScore, AggregationError> {
        let ratings = self.rating_repo.list_approved(supervisor_id).await?;
        Ok(compute_from_approved(&ratings, &equal_weights()))
    }

    /// Compute the score for a supervisor using a per-discipline weight
    /// map (see `discipline::service::renormalize`). Pass `None` for
    /// equal weights (same as `compute`).
    pub async fn compute_with_weights(
        &self,
        supervisor_id: Uuid,
        weights: Option<&HashMap<String, f64>>,
    ) -> Result<SupervisorScore, AggregationError> {
        let ratings = self.rating_repo.list_approved(supervisor_id).await?;
        let w = weights
            .cloned()
            .unwrap_or_else(equal_weights);
        Ok(compute_from_approved(&ratings, &w))
    }
}

/// Equal-weight map (1/6 for every dim) — used when no per-discipline
/// weights are available (M7 default; also the bootstrap before any
/// weight vote has been applied for a discipline).
pub fn equal_weights() -> HashMap<String, f64> {
    let mut m = HashMap::with_capacity(6);
    for &d in RADAR_DIMS {
        m.insert(d.to_string(), 1.0 / 6.0);
    }
    m
}

/// Pure function: aggregate the (dim, value) pairs into score + radar.
///
/// Public clamping: every per-dim mean is `max(0, mean)` (negative means
/// never surface), as is the composite.
///
/// Returns `composite: None` (and all radar dims `None`) when no approved
/// ratings exist.
///
/// `weights` maps each dim → weight. Composite is a normalized weighted
/// sum over the dims that have data: `sum(w[dim] * mean[dim]) /
/// sum(w[dim])` for dims that have data. We renormalize on the fly so
/// that missing dims do not bias the result.
pub fn compute_from_approved(
    ratings: &[ApprovedRating],
    weights: &HashMap<String, f64>,
) -> SupervisorScore {
    if ratings.is_empty() {
        return SupervisorScore {
            composite: None,
            radar: empty_radar(),
            approved_rating_count: 0,
        };
    }

    // Group values by dim (preserving RADAR_DIMS order in the output).
    let mut by_dim: BTreeMap<&str, Vec<i16>> = BTreeMap::new();
    for r in ratings {
        by_dim.entry(r.dim.as_str()).or_default().push(r.value);
    }

    // Per-dim mean, clamped to >= 0.
    let radar = RadarDimensions {
        research: by_dim.remove("research").map(mean_clamp),
        resource: by_dim.remove("resource").map(mean_clamp),
        fit: by_dim.remove("fit").map(mean_clamp),
        currency: by_dim.remove("currency").map(mean_clamp),
        ethic: by_dim.remove("ethic").map(mean_clamp),
        tool: by_dim.remove("tool").map(mean_clamp),
    };

    // Composite: weighted mean over the dims that have data.
    // We normalize by the sum of weights for *data-bearing* dims only.
    let dim_means: [(Option<f64>, &str); 6] = [
        (radar.research, "research"),
        (radar.resource, "resource"),
        (radar.fit, "fit"),
        (radar.currency, "currency"),
        (radar.ethic, "ethic"),
        (radar.tool, "tool"),
    ];
    let mut num: f64 = 0.0;
    let mut den: f64 = 0.0;
    for (m, d) in &dim_means {
        if let Some(v) = m {
            let w = weights.get(*d).copied().unwrap_or(0.0);
            num += w * v;
            den += w;
        }
    }
    let composite = if den > 0.0 {
        Some((num / den).max(0.0))
    } else {
        // No data-bearing dim had a non-zero weight — fall back to
        // equal-weight average over data-bearing dims (the M7 default).
        let data_means: Vec<f64> = dim_means.iter().filter_map(|(m, _)| *m).collect();
        if data_means.is_empty() {
            None
        } else {
            let raw = data_means.iter().sum::<f64>() / data_means.len() as f64;
            Some(raw.max(0.0))
        }
    };

    SupervisorScore {
        composite,
        radar,
        approved_rating_count: ratings.len(),
    }
}

fn empty_radar() -> RadarDimensions {
    RadarDimensions {
        research: None,
        resource: None,
        fit: None,
        currency: None,
        ethic: None,
        tool: None,
    }
}

fn mean_clamp(values: Vec<i16>) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let sum: i64 = values.iter().map(|&v| v as i64).sum();
    let mean = sum as f64 / values.len() as f64;
    mean.max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(dim: &str, v: i16) -> ApprovedRating {
        ApprovedRating { dim: dim.to_string(), value: v }
    }

    fn ew() -> HashMap<String, f64> {
        equal_weights()
    }

    #[test]
    fn empty_ratings_yield_none() {
        let s = compute_from_approved(&[], &ew());
        assert!(s.composite.is_none());
        assert_eq!(s.approved_rating_count, 0);
        assert!(s.radar.research.is_none());
        assert!(s.radar.tool.is_none());
    }

    #[test]
    fn single_dim_single_value_clamps() {
        let s = compute_from_approved(&[r("research", 85)], &ew());
        assert_eq!(s.composite, Some(85.0));
        assert_eq!(s.radar.research, Some(85.0));
        assert_eq!(s.approved_rating_count, 1);
    }

    #[test]
    fn negative_value_clamps_to_zero() {
        let s = compute_from_approved(&[r("research", -50)], &ew());
        assert_eq!(s.composite, Some(0.0));
        assert_eq!(s.radar.research, Some(0.0));
    }

    #[test]
    fn mixed_dims_averaged() {
        let ratings = vec![
            r("research", 80),
            r("research", 100),
            r("resource", 60),
            r("fit", 50),
            r("currency", 70),
            r("ethic", 90),
            r("tool", 40),
        ];
        let s = compute_from_approved(&ratings, &ew());
        // Per-dim means (all positive, so clamp is a no-op):
        assert_eq!(s.radar.research, Some(90.0));
        assert_eq!(s.radar.resource, Some(60.0));
        assert_eq!(s.radar.fit, Some(50.0));
        assert_eq!(s.radar.currency, Some(70.0));
        assert_eq!(s.radar.ethic, Some(90.0));
        assert_eq!(s.radar.tool, Some(40.0));
        // Composite = mean of all 6 = (90+60+50+70+90+40)/6 = 400/6 ≈ 66.67
        let expected = (90.0 + 60.0 + 50.0 + 70.0 + 90.0 + 40.0) / 6.0;
        assert!((s.composite.unwrap() - expected).abs() < 0.001);
    }

    #[test]
    fn missing_dims_are_null() {
        // Only research + resource have data; other dims are None.
        let s = compute_from_approved(&[r("research", 80), r("resource", 60)], &ew());
        assert!(s.radar.research.is_some());
        assert!(s.radar.resource.is_some());
        assert!(s.radar.fit.is_none());
        assert!(s.radar.currency.is_none());
        assert!(s.radar.ethic.is_none());
        assert!(s.radar.tool.is_none());
        // Composite is still computed from the 2 dims that have data.
        assert_eq!(s.composite, Some(70.0));
    }

    #[test]
    fn negative_dominant_clamps_composite() {
        // All values negative — composite should clamp to 0, not be negative.
        let ratings = vec![r("research", -30), r("resource", -50)];
        let s = compute_from_approved(&ratings, &ew());
        // Per-dim means clamped to 0.
        assert_eq!(s.radar.research, Some(0.0));
        assert_eq!(s.radar.resource, Some(0.0));
        // Composite of [0, 0] is 0.
        assert_eq!(s.composite, Some(0.0));
    }

    #[test]
    fn all_six_dims_present_in_radar() {
        let s = compute_from_approved(&[], &ew());
        let json = serde_json::to_value(&s.radar).unwrap();
        let obj = json.as_object().unwrap();
        // All 6 keys present, all null.
        for dim in RADAR_DIMS {
            assert!(obj.contains_key(*dim), "radar missing key {dim}");
        }
    }

    #[test]
    fn many_values_per_dim_averaged() {
        let ratings = vec![
            r("research", 50), r("research", 70), r("research", 90),
            r("research", 100),
        ];
        let s = compute_from_approved(&ratings, &ew());
        // (50+70+90+100)/4 = 310/4 = 77.5
        assert_eq!(s.radar.research, Some(77.5));
        assert_eq!(s.composite, Some(77.5));
    }

    // ---- Weighted aggregation (M2) ----

    /// Helper: 4 ratings, one per dim, values 80/60/40/20. Easy to
    /// reason about the weighted mean.
    fn four_dim_ratings() -> Vec<ApprovedRating> {
        vec![
            r("research", 80),
            r("resource", 60),
            r("tool", 40),
            r("ethic", 20),
        ]
    }

    #[test]
    fn weighted_composite_with_equal_weights_matches_unweighted() {
        // All weights equal → same result as M7.
        let s = compute_from_approved(&four_dim_ratings(), &ew());
        // (80 + 60 + 40 + 20) / 4 = 50
        let v = s.composite.unwrap();
        assert!((v - 50.0).abs() < 1e-9, "expected ~50.0, got {v}");
    }

    #[test]
    fn weighted_composite_skews_toward_high_weight_dim() {
        // Up-weight "research" to 0.5, down-weight the others to 0.1 each.
        // (sum = 0.5 + 0.1*3 = 0.8, normalized)
        // weighted = 0.5*80 + 0.1*60 + 0.1*40 + 0.1*20 = 40 + 6 + 4 + 2 = 52
        // normalized: 52 / 0.8 = 65.0
        let mut w = HashMap::new();
        w.insert("research".into(), 0.5);
        w.insert("resource".into(), 0.1);
        w.insert("tool".into(), 0.1);
        w.insert("ethic".into(), 0.1);
        let s = compute_from_approved(&four_dim_ratings(), &w);
        assert!((s.composite.unwrap() - 65.0).abs() < 1e-9);
    }

    #[test]
    fn weighted_composite_ignores_dims_with_zero_weight() {
        // If a dim has data but its weight is 0, it should not
        // contribute to the composite (numerator AND denominator).
        let mut w = HashMap::new();
        w.insert("research".into(), 0.5);
        w.insert("resource".into(), 0.0);
        w.insert("tool".into(), 0.0);
        w.insert("ethic".into(), 0.5);
        // weighted = 0.5*80 + 0*60 + 0*40 + 0.5*20 = 40 + 0 + 0 + 10 = 50
        // normalized: 50 / 1.0 = 50.0
        let s = compute_from_approved(&four_dim_ratings(), &w);
        assert!((s.composite.unwrap() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn weighted_composite_handles_missing_data_gracefully() {
        // Only "research" has data; weights for the others shouldn't
        // cause a divide-by-zero or distort the result.
        let mut w = HashMap::new();
        w.insert("research".into(), 0.30);
        for d in &["resource", "fit", "currency", "ethic", "tool"] {
            w.insert((*d).to_string(), 0.14);
        }
        let s = compute_from_approved(&[r("research", 80)], &w);
        // Only research has data: composite = 80.
        assert_eq!(s.composite, Some(80.0));
    }

    #[test]
    fn weighted_composite_falls_back_when_no_weight_for_any_data_dim() {
        // All data-bearing dims have weight 0 → fall back to equal-
        // weight average over data-bearing dims.
        let mut w = HashMap::new();
        for d in &["research", "resource", "fit", "currency", "ethic", "tool"] {
            w.insert((*d).to_string(), 0.0);
        }
        let s = compute_from_approved(&[r("research", 80), r("tool", 40)], &w);
        // (80 + 40) / 2 = 60
        assert_eq!(s.composite, Some(60.0));
    }

    #[test]
    fn weighted_composite_renormalize_pattern_matches_discipline() {
        // Mirrors `discipline::service::renormalize` (H-43):
        //   target = research 0.30
        //   others = (1 - 0.30) / 5 = 0.14 each
        // (Note: only 4 dims have data here, so the renormalize map
        // is the same 0.14 for the *other 4* that have ratings —
        // research/tool/resource/ethic. fit/currency have no data.)
        let mut w = HashMap::new();
        w.insert("research".into(), 0.30);
        w.insert("resource".into(), 0.14);
        w.insert("tool".into(), 0.14);
        w.insert("ethic".into(), 0.14);
        w.insert("fit".into(), 0.14);
        w.insert("currency".into(), 0.14);
        // weighted = 0.30*80 + 0.14*60 + 0.14*40 + 0.14*20
        //         = 24 + 8.4 + 5.6 + 2.8 = 40.8
        // normalized: 40.8 / 0.72 = 56.666...
        let s = compute_from_approved(&four_dim_ratings(), &w);
        let expected = 40.8_f64 / 0.72_f64;
        assert!((s.composite.unwrap() - expected).abs() < 1e-9);
    }
}
