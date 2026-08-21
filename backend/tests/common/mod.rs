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
    supervisor_arena::db::run_migrations(&pool)
        .await
        .expect("run migrations on test DB");

    *guard = Some(TestDb { container, pool: pool.clone() });
    pool
}

/// Truncate all app tables. Use this at the start of each test for
/// isolation, since `serial_test` keeps the container alive across tests.
///
/// `CASCADE` handles FK constraints. `RESTART IDENTITY` resets serial PKs.
pub async fn truncate_all(pool: &Pool) {
    let c = pool
        .get()
        .await
        .expect("truncate_all: pool get");
    c.batch_execute(
        "TRUNCATE TABLE
             ratings,
             supervisor_name_mappings,
             supervisor_creation_requests,
             supervisors,
             disciplines,
             colleges,
             accounts
         RESTART IDENTITY CASCADE",
    )
    .await
    .expect("truncate_all: batch_execute");
}
