//! Aggregation service: pure functions to compute composite_score + radar
//!
//! These are intentionally decoupled from the DB (they take `&[ApprovedRating]`
//! as input) so we can unit-test them without a live DB.

use serde::Serialize;
use std::collections::BTreeMap;
use uuid::Uuid;

use super::error::AggregationError;
use super::repo::{ApprovedRating, RatingRepo};
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

    /// Compute the score for a supervisor. Pulls all approved ratings
    /// from the DB and applies the algorithm in `compute_from_approved`.
    pub async fn compute(
        &self,
        supervisor_id: Uuid,
    ) -> Result<SupervisorScore, AggregationError> {
        let ratings = self.rating_repo.list_approved(supervisor_id).await?;
        Ok(compute_from_approved(&ratings))
    }
}

/// Pure function: aggregate the (dim, value) pairs into score + radar.
///
/// Public clamping: every per-dim mean is `max(0, mean)` (negative means
/// never surface), as is the composite.
///
/// Returns `composite: None` (and all radar dims `None`) when no approved
/// ratings exist.
pub fn compute_from_approved(ratings: &[ApprovedRating]) -> SupervisorScore {
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

    // Composite: mean of the per-dim means (only dims that have data).
    let dim_means = [
        radar.research,
        radar.resource,
        radar.fit,
        radar.currency,
        radar.ethic,
        radar.tool,
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<f64>>();
    let composite = if dim_means.is_empty() {
        None
    } else {
        let raw = dim_means.iter().sum::<f64>() / dim_means.len() as f64;
        Some(raw.max(0.0))
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

    #[test]
    fn empty_ratings_yield_none() {
        let s = compute_from_approved(&[]);
        assert!(s.composite.is_none());
        assert_eq!(s.approved_rating_count, 0);
        assert!(s.radar.research.is_none());
        assert!(s.radar.tool.is_none());
    }

    #[test]
    fn single_dim_single_value_clamps() {
        let s = compute_from_approved(&[r("research", 85)]);
        assert_eq!(s.composite, Some(85.0));
        assert_eq!(s.radar.research, Some(85.0));
        assert_eq!(s.approved_rating_count, 1);
    }

    #[test]
    fn negative_value_clamps_to_zero() {
        let s = compute_from_approved(&[r("research", -50)]);
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
        let s = compute_from_approved(&ratings);
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
        let s = compute_from_approved(&[r("research", 80), r("resource", 60)]);
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
        let s = compute_from_approved(&ratings);
        // Per-dim means clamped to 0.
        assert_eq!(s.radar.research, Some(0.0));
        assert_eq!(s.radar.resource, Some(0.0));
        // Composite of [0, 0] is 0.
        assert_eq!(s.composite, Some(0.0));
    }

    #[test]
    fn all_six_dims_present_in_radar() {
        let s = compute_from_approved(&[]);
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
        let s = compute_from_approved(&ratings);
        // (50+70+90+100)/4 = 310/4 = 77.5
        assert_eq!(s.radar.research, Some(77.5));
        assert_eq!(s.composite, Some(77.5));
    }
}
