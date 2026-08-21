//! Shared test infrastructure: ephemeral Postgres via testcontainers.
//!
//! One Postgres container is started for the whole integration test binary
//! (via `OnceCell`), reused across all tests in this binary. Migrations
//! are run once at setup. Between tests, `truncate_all` resets the schema
//! (faster than spinning up a new container per test).
//!
//! Docker is required on the test machine. If Docker is unavailable, the
//! tests will fail with a clear "could not start container" message.

use deadpool_postgres::Pool;
use std::sync::OnceLock;
use testcontainers::ContainerAsync;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;

/// Holds the running Postgres container + its connection pool. The
/// container is dropped (and removed) when the test binary exits.
pub struct TestDb {
    #[allow(dead_code)]
    pub container: ContainerAsync<Postgres>,
    pub pool: Pool,
}

static TEST_DB: OnceLock<tokio::sync::Mutex<Option<TestDb>>> =
    OnceLock::new();

fn test_db_lock() -> &'static tokio::sync::Mutex<Option<TestDb>> {
    TEST_DB.get_or_init(|| tokio::sync::Mutex::new(None))
}

/// Start the test Postgres container (if not already started) and return
/// the connection pool. After the first call, subsequent calls return
/// the cached pool.
pub async fn get_test_pool() -> Pool {
    let mut guard = test_db_lock().lock().await;
    if let Some(td) = guard.as_ref() {
        return td.pool.clone();
    }

    let container = Postgres::default()
        .start()
        .await
        .expect("failed to start testcontainers postgres — is Docker running?");
    let host = container
        .get_host()
        .await
        .expect("container host");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("container port 5432");
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");

    let pool = supervisor_arena::db::build_pool_from_url(&url)
        .await
        .expect("build pool from testcontainers url");

    // testcontainers-modules 0.11 ships a Postgres image older than 13,
    // where `gen_random_uuid()` lives in the `pgcrypto` extension
    // instead of being a built-in. Enable it before running migrations
    // so the schema (which uses gen_random_uuid() for PKs) applies
    // cleanly. PG 13+ ignores this as a no-op (extension already
    // present in core).
    {
        let c = pool.get().await.expect("get conn for pgcrypto enable");
        c.batch_execute("CREATE EXTENSION IF NOT EXISTS pgcrypto;")
            .await
            .expect("enable pgcrypto");
    }

    supervisor_arena::db::run_migrations(&pool)
        .await
        .expect("run migrations on test DB");

    *guard = Some(TestDb { container, pool: pool.clone() });
    pool
}

/// Truncate all app tables. Use this at the start of each test for
/// isolation, since `serial_test` keeps the container alive across tests.
///
/// # Strategy
///
/// **We cannot use `TRUNCATE ... CASCADE`** because CASCADE follows FK
/// references regardless of the ON DELETE rule, so truncating `accounts`
/// (for example) would cascade through the FK chain
/// `accounts` → `discipline_weight_votes` → `discipline_weights` and
/// wipe the M2 bootstrap rows we want to keep across tests.
///
/// Instead we issue explicit `DELETE FROM ...` statements in a safe
/// order (children before parents), with `RESTART IDENTITY` for any
/// serial PKs. This preserves the bootstrap state of
/// `discipline_weights` / `discipline_weight_history` between tests.
///
/// # What survives
///
/// - `disciplines`, `colleges`, `rating_dimensions` (lookup tables)
/// - `discipline_weights` (M2 bootstrap)
/// - `discipline_weight_history` (M2 bootstrap)
pub async fn truncate_all(pool: &Pool) {
    let c = pool
        .get()
        .await
        .expect("truncate_all: pool get");
    c.batch_execute(
        "DELETE FROM account_invitations;
         DELETE FROM discipline_weight_voters;
         DELETE FROM discipline_weight_votes;
         DELETE FROM ratings;
         DELETE FROM supervisor_name_mappings;
         DELETE FROM supervisor_creation_requests;
         DELETE FROM supervisors;
         DELETE FROM behavior_fingerprints;
         DELETE FROM reports;
         DELETE FROM evidence;
         DELETE FROM supervisor_aggregate_snapshots;
         DELETE FROM encryption_audit_log;
         DELETE FROM accounts;",
    )
    .await
    .expect("truncate_all: app tables (DELETEs in FK-safe order)");
}
