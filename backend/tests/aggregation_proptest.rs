//! Property-based tests for the aggregation algorithm
//!
//! These use `proptest` to generate random inputs and verify invariants:
//! - composite_score is always >= 0 when not None
//! - composite_score is None iff no ratings
//! - per-dim means are within [min_input, max_input] for that dim
//! - composite is the mean of dim-means that are Some
//! - count is the total number of ratings

use proptest::prelude::*;
use supervisor_arena::aggregation::compute_from_approved;
use supervisor_arena::aggregation::equal_weights;
use supervisor_arena::aggregation::ApprovedRating;

const DIMS: &[&str] = &[
    "research",
    "resource",
    "fit",
    "currency",
    "ethic",
    "tool",
];

/// Generate a vec of random approved ratings.
fn ratings_strategy() -> impl Strategy<Value = Vec<ApprovedRating>> {
    // Up to 50 ratings; per-dim and per-value drawn independently.
    proptest::collection::vec(
        (
            proptest::sample::select(DIMS),
            -100i16..=100i16,
        )
            .prop_map(|(dim, value)| ApprovedRating {
                dim: dim.to_string(),
                value,
            }),
        0..50,
    )
}

proptest! {
    #[test]
    fn composite_is_non_negative_or_none(ratings in ratings_strategy()) {
        let s = compute_from_approved(&ratings, &equal_weights());
        if ratings.is_empty() {
            assert!(s.composite.is_none(), "empty → composite None");
        } else {
            let c = s.composite.expect("non-empty ratings → composite Some");
            assert!(c >= 0.0, "composite must be >= 0 (public clamp), got {c}");
        }
    }

    #[test]
    fn count_matches_input_len(ratings in ratings_strategy()) {
        let s = compute_from_approved(&ratings, &equal_weights());
        assert_eq!(
            s.approved_rating_count,
            ratings.len(),
            "count must match input length"
        );
    }

    #[test]
    fn radar_dimensions_within_clamped_input_range(ratings in ratings_strategy()) {
        // Algorithm invariant: per-dim mean is within [clamp(min, 0), clamp(max, 0)].
        // (Mean is clamped to >= 0 per H-33.)
        let s = compute_from_approved(&ratings, &equal_weights());
        for (i, &dim) in DIMS.iter().enumerate() {
            let input_values: Vec<i16> = ratings
                .iter()
                .filter(|r| r.dim == dim)
                .map(|r| r.value)
                .collect();
            let mean_opt = match i {
                0 => s.radar.research,
                1 => s.radar.resource,
                2 => s.radar.fit,
                3 => s.radar.currency,
                4 => s.radar.ethic,
                5 => s.radar.tool,
                _ => unreachable!(),
            };
            if let Some(m) = mean_opt {
                if let (Some(&min), Some(&max)) =
                    (input_values.iter().min(), input_values.iter().max())
                {
                    // Both bounds clamped: a single -100 input gives [0, 0].
                    let minf = (min as f64).max(0.0);
                    let maxf = (max as f64).max(0.0);
                    assert!(
                        m >= minf - 0.01 && m <= maxf + 0.01,
                        "{dim} mean {m} outside clamped input range [{minf}, {maxf}]"
                    );
                }
            }
        }
    }

    #[test]
    fn all_six_dims_present_in_radar(ratings in ratings_strategy()) {
        let s = compute_from_approved(&ratings, &equal_weights());
        let json = serde_json::to_value(&s.radar).unwrap();
        let obj = json.as_object().unwrap();
        // All 6 keys present, even if null.
        for dim in DIMS {
            assert!(obj.contains_key(*dim), "radar missing key {dim}");
        }
    }

    #[test]
    fn composite_is_mean_of_present_dim_means(ratings in ratings_strategy()) {
        let s = compute_from_approved(&ratings, &equal_weights());
        // Skip empty / all-null cases (composite is None, nothing to compare).
        let present: Vec<f64> = [
            s.radar.research,
            s.radar.resource,
            s.radar.fit,
            s.radar.currency,
            s.radar.ethic,
            s.radar.tool,
        ]
        .into_iter()
        .flatten()
        .collect();
        if let Some(actual) = s.composite {
            if !present.is_empty() {
                let expected: f64 = present.iter().sum::<f64>() / present.len() as f64;
                let diff = (actual - expected).abs();
                assert!(
                    diff < 0.01,
                    "composite {actual} != mean of dim-means {expected} (diff {diff})"
                );
            }
        }
    }

    #[test]
    fn single_value_clamps_to_zero_if_negative(
        value in -100i16..=-1i16
    ) {
        let ratings = vec![ApprovedRating {
            dim: "research".to_string(),
            value,
        }];
        let s = compute_from_approved(&ratings, &equal_weights());
        assert_eq!(s.composite, Some(0.0), "negative {value} must clamp to 0");
        assert_eq!(s.radar.research, Some(0.0));
    }

    #[test]
    fn single_positive_value_passes_through(value in 0i16..=100i16) {
        let ratings = vec![ApprovedRating {
            dim: "research".to_string(),
            value,
        }];
        let s = compute_from_approved(&ratings, &equal_weights());
        // Float epsilon — weighted formula does (w * mean) / w, which can
        // accumulate rounding error.
        let expected = value as f64;
        let c = s.composite.unwrap();
        assert!((c - expected).abs() < 1e-9, "composite {c} != {expected}");
        let r = s.radar.research.unwrap();
        assert!((r - expected).abs() < 1e-9, "radar {r} != {expected}");
    }
}
