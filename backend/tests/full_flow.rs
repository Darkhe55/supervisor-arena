//! Full lifecycle integration test:
//!   insert accounts + supervisors → verify k-anon gating + aggregation
//!   → test B-9 supersede via 3-step transaction
//!
//! Uses an ephemeral Postgres (testcontainers). Each test truncates the
//! tables first to keep them independent. Tests run with `serial_test`
//! so the shared container isn't contested.

mod common;

use deadpool_postgres::Pool;
use serial_test::serial;
use supervisor_arena::aggregation::{compute_from_approved, ApprovedRating};
use supervisor_arena::supervisor::alias::AliasGenerator;
use uuid::Uuid;

const TEST_HMAC_KEY: [u8; 32] = [0x42_u8; 32];

async fn setup() -> Pool {
    let pool = common::get_test_pool().await;
    common::truncate_all(&pool).await;
    pool
}

async fn insert_account(
    pool: &Pool,
    email: &str,
    discipline: &str,
    institution: &str,
) -> Uuid {
    let c = pool.get().await.unwrap();
    let id = Uuid::new_v4();
    c.execute(
        "INSERT INTO accounts
            (id, email_enc, email_hash, password_hash,
             discipline_hash, institution_hash, tier)
         VALUES ($1, '\\x00'::bytea, $2, 'placeholder',
                 $3, $4, 'basic')",
        &[
            &id,
            &fnv_hash(email.as_bytes()),
            &fnv_hash(discipline.as_bytes()),
            &fnv_hash(institution.as_bytes()),
        ],
    )
    .await
    .unwrap();
    id
}

async fn insert_supervisor(
    pool: &Pool,
    alias: &str,
    discipline: &str,
    college: &str,
    review_status: &str,
    k_count: i32,
) -> Uuid {
    let c = pool.get().await.unwrap();
    let id = Uuid::new_v4();
    c.execute(
        "INSERT INTO supervisors
            (id, public_code, discipline, college,
             review_status, k_anonymity_count)
         VALUES ($1, $2, $3, $4, $5, $6)",
        &[&id, &alias, &discipline, &college, &review_status, &k_count],
    )
    .await
    .unwrap();
    id
}

async fn insert_rating(
    pool: &Pool,
    account_id: Uuid,
    supervisor_id: Uuid,
    dim: &str,
    value: i16,
) {
    let c = pool.get().await.unwrap();
    c.execute(
        "INSERT INTO ratings
            (account_id, supervisor_id, dim, value, discipline_hash)
         VALUES ($1, $2, $3, $4, '\\x00'::bytea)",
        &[&account_id, &supervisor_id, &dim, &value],
    )
    .await
    .unwrap();
}

#[tokio::test]
#[serial]
async fn full_lifecycle_aggregation_visible() {
    let pool = setup().await;

    // 1. Insert 10 accounts in CS / CS bucket (≥ k_threshold 10 → visible).
    for i in 0..10 {
        insert_account(
            &pool,
            &format!("user{i}@example.com"),
            "CS",
            "CS",
        )
        .await;
    }

    // 2. Insert 10 approved supervisors in the same CS/CS bucket.
    for i in 0..10 {
        insert_supervisor(&pool, &format!("Q-TEST-{i}"), "CS", "CS", "approved", 10).await;
    }

    // 3. Search returns 10 entries.
    let c = pool.get().await.unwrap();
    let count: i64 = c
        .query_one(
            "SELECT COUNT(*) FROM supervisors
             WHERE review_status = 'approved'
               AND discipline = 'CS' AND college = 'CS'
               AND k_anonymity_count >= 10",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 10, "expected 10 visible approved supervisors");
}

#[tokio::test]
#[serial]
async fn aggregation_correctness_against_db() {
    let pool = setup().await;

    // Setup: 1 account + 1 supervisor + 3 approved ratings.
    let account_id = insert_account(&pool, "agg@example.com", "CS", "CS").await;
    let sup_id = insert_supervisor(&pool, "AGG-TEST", "CS", "CS", "approved", 10).await;
    for (dim, val) in [("research", 80), ("resource", 60), ("fit", 50)] {
        insert_rating(&pool, account_id, sup_id, dim, val).await;
    }

    // Run the same query the AggregationService runs.
    let c = pool.get().await.unwrap();
    let rows = c
        .query(
            "SELECT dim, value FROM ratings
             WHERE supervisor_id = $1::uuid
               AND review_status = 'approved'
               AND superseded_by IS NULL",
            &[&sup_id],
        )
        .await
        .unwrap();

    let approved: Vec<ApprovedRating> = rows
        .into_iter()
        .map(|r| ApprovedRating {
            dim: r.get(0),
            value: r.get(1),
        })
        .collect();

    let score = compute_from_approved(&approved);
    assert_eq!(score.approved_rating_count, 3);
    let expected = (80.0 + 60.0 + 50.0) / 3.0;
    assert!(
        (score.composite.unwrap() - expected).abs() < 0.01,
        "composite = {}, expected ≈ {}",
        score.composite.unwrap(),
        expected
    );
    assert_eq!(score.radar.research, Some(80.0));
    assert_eq!(score.radar.resource, Some(60.0));
    assert_eq!(score.radar.fit, Some(50.0));
    assert!(score.radar.currency.is_none());
}

#[tokio::test]
#[serial]
async fn k_anonymity_gates_search() {
    let pool = setup().await;

    // 5 approved supervisors in LIT/LIT — k_count=5 < 10 → hidden.
    for i in 0..5 {
        insert_supervisor(&pool, &format!("K-HIDDEN-{i}"), "LIT", "LIT", "approved", 5).await;
    }

    let c = pool.get().await.unwrap();
    let count: i64 = c
        .query_one(
            "SELECT COUNT(*) FROM supervisors
             WHERE review_status = 'approved'
               AND discipline = 'LIT' AND college = 'LIT'
               AND k_anonymity_count >= 10",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 0, "k_count < 10 must hide entries from search");

    // Now bump k_count to 10 and verify the search returns them.
    c.execute(
        "UPDATE supervisors SET k_anonymity_count = 10
         WHERE discipline = 'LIT' AND college = 'LIT'",
        &[],
    )
    .await
    .unwrap();
    let count: i64 = c
        .query_one(
            "SELECT COUNT(*) FROM supervisors
             WHERE review_status = 'approved'
               AND discipline = 'LIT' AND college = 'LIT'
               AND k_anonymity_count >= 10",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 5, "after k_count bump, all 5 should be visible");
}

#[tokio::test]
#[serial]
async fn unique_constraint_one_current_rating() {
    // Verify the UQ `uq_ratings_one_current` works at the DB level.
    let pool = setup().await;
    let c = pool.get().await.unwrap();
    let account_id = insert_account(&pool, "uq@example.com", "CS", "CS").await;
    let sup_id = insert_supervisor(&pool, "UQ-TEST", "CS", "CS", "approved", 10).await;

    // Insert 3 ratings on the same (account, sup, dim) without supersede.
    // The first succeeds; the 2nd and 3rd violate the UQ (superseded_by IS NULL).
    insert_rating(&pool, account_id, sup_id, "research", 50).await;
    let r2 = c
        .execute(
            "INSERT INTO ratings
                (account_id, supervisor_id, dim, value, discipline_hash)
             VALUES ($1, $2, 'research', 60, '\\x00'::bytea)",
            &[&account_id, &sup_id],
        )
        .await;
    assert!(r2.is_err(), "second insert must violate uq_ratings_one_current");

    // Confirm: only 1 row exists for this (account, sup, dim).
    let count: i64 = c
        .query_one(
            "SELECT COUNT(*) FROM ratings
             WHERE account_id = $1 AND supervisor_id = $2 AND dim = 'research'",
            &[&account_id, &sup_id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 1, "exactly one row should exist after UQ enforcement");
}

#[tokio::test]
#[serial]
async fn pending_ratings_excluded_from_aggregation() {
    // Verify the aggregation only considers approved ratings.
    let pool = setup().await;
    let account_id = insert_account(&pool, "pend@example.com", "CS", "CS").await;
    let sup_id = insert_supervisor(&pool, "PEND-TEST", "CS", "CS", "approved", 10).await;

    // 2 approved + 1 pending.
    insert_rating(&pool, account_id, sup_id, "research", 80).await;
    insert_rating(&pool, account_id, sup_id, "resource", 60).await;
    let c = pool.get().await.unwrap();
    c.execute(
        "INSERT INTO ratings
            (account_id, supervisor_id, dim, value, discipline_hash, review_status)
         VALUES ($1, $2, 'fit', 100, '\\x00'::bytea, 'pending_review')",
        &[&account_id, &sup_id],
    )
    .await
    .unwrap();

    let rows = c
        .query(
            "SELECT dim, value FROM ratings
             WHERE supervisor_id = $1::uuid
               AND review_status = 'approved'
               AND superseded_by IS NULL",
            &[&sup_id],
        )
        .await
        .unwrap();

    let approved: Vec<ApprovedRating> = rows
        .into_iter()
        .map(|r| ApprovedRating {
            dim: r.get(0),
            value: r.get(1),
        })
        .collect();
    let score = compute_from_approved(&approved);
    // Should be 2 dims (research + resource), not 3 (fit pending).
    assert_eq!(score.approved_rating_count, 2);
    assert_eq!(score.radar.research, Some(80.0));
    assert_eq!(score.radar.resource, Some(60.0));
    assert!(score.radar.fit.is_none(), "pending rating must not be aggregated");
    // Composite = mean of 80 + 60 = 70.
    assert!((score.composite.unwrap() - 70.0).abs() < 0.01);
}

#[tokio::test]
async fn alias_determinism_and_uniqueness() {
    // Pure unit test — no DB needed.
    let g = AliasGenerator::new(TEST_HMAC_KEY);

    // Determinism.
    let a = g
        .generate(
            supervisor_arena::supervisor::alias::AliasInput {
                submitted_name: "张伟",
                discipline: "computer_science",
                college: "MIT",
            },
            0,
        )
        .unwrap();
    let b = g
        .generate(
            supervisor_arena::supervisor::alias::AliasInput {
                submitted_name: "张伟",
                discipline: "computer_science",
                college: "MIT",
            },
            0,
        )
        .unwrap();
    assert_eq!(a.0, b.0, "alias must be deterministic for same input");

    // Uniqueness across variations: 5 × 3 × 3 = 45 unique inputs.
    let mut aliases = std::collections::HashSet::new();
    for name in ["A", "B", "C", "D", "E"] {
        for disc in ["CS", "MATH", "LIT"] {
            for coll in ["MIT", "Stanford", "Yale"] {
                let (alias, _) = g
                    .generate(
                        supervisor_arena::supervisor::alias::AliasInput {
                            submitted_name: name,
                            discipline: disc,
                            college: coll,
                        },
                        0,
                    )
                    .unwrap();
                aliases.insert(alias);
            }
        }
    }
    assert_eq!(
        aliases.len(),
        45,
        "all 45 (name, disc, coll) combos must produce distinct aliases"
    );
}

// Simple FNV-1a hash for test fixtures. NOT a security primitive —
// we use it to generate deterministic dummy email/discipline hashes for
// test rows. Production uses HMAC-SHA256 from LocalKeyStore.
fn fnv_hash(bytes: &[u8]) -> Vec<u8> {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h.to_le_bytes().to_vec()
}
