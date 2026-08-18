# supervisor-arena backend (Rust)

## Status

**Phase 1: Project Scaffold** ✅

This is the Rust backend for the supervisor-arena project — a crowd-sourced
supervisor rating system with anonymous alias protection (see
[`../docs/OUTLINE.md`](../docs/OUTLINE.md) for full design).

## Tech Stack

| Layer | Choice |
|-------|--------|
| Language | Rust 1.75+ (edition 2021) |
| Web framework | axum 0.7 |
| Async runtime | tokio 1 |
| Database | PostgreSQL 16 |
| ORM | sqlx 0.8 (async + compile-time SQL check) |
| Cache | Redis 7 |
| Crypto | aes-gcm + hmac + sha2 + argon2 (G-8) |
| Logging | tracing + tracing-subscriber |
| Errors | thiserror + anyhow |

## Quick Start

### 1. Install dependencies

```bash
# Rust 1.75+
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 2. Start PostgreSQL + Redis

```bash
cp .env.example .env
docker compose up -d
```

This starts:
- PostgreSQL on `localhost:5432` (user/pwd: `supervisor/supervisor_dev_pwd`)
- Redis on `localhost:6379`

### 3. Generate dev secrets

```bash
# Generate 64-char hex strings for JWT + encryption
export JWT_SECRET=$(openssl rand -hex 64)
export ENCRYPTION_FIELD_KEY=$(openssl rand -hex 32)
export HMAC_SALT_KEY=$(openssl rand -hex 32)
```

Update `.env` with these values.

### 4. Build + run

```bash
# Set DATABASE_URL for sqlx compile-time check
export DATABASE_URL=postgres://supervisor:supervisor_dev_pwd@localhost:5432/supervisor_arena

# Run (will run migrations when Phase 2 lands)
cargo run
```

### 5. Verify

```bash
curl http://localhost:8080/health
# {"status":"ok","version":"0.1.0"}

curl http://localhost:8080/version
# {"status":"ok","version":"0.1.0"}
```

## Project Layout

```
backend/
├── Cargo.toml              # dependencies
├── docker-compose.yml      # local PG + Redis
├── .env.example            # config template
├── docker/
│   └── postgres/
│       └── init.sql        # PG extensions setup
├── src/
│   ├── main.rs             # binary entry point
│   ├── lib.rs              # library entry (run() function)
│   ├── config.rs           # environment-driven config
│   └── observability.rs    # tracing setup
└── README.md
```

## Roadmap

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | Project scaffold | ✅ |
| 2 | Database migrations (sqlx) + connection pool | ⏳ |
| 3 | Crypto module (AES-256-GCM + HMAC + Argon2id) | ⏳ |
| 4 | Account module (register/login/JWT) | ⏳ |
| 5 | Supervisor + AliasGenerator (核心) | ⏳ |
| 6 | Rating module (slider + additional info) | ⏳ |
| 7 | Aggregation + public API | ⏳ |
| 8 | Tests (unit + integration) | ⏳ |

See [`../docs/OUTLINE.md`](../docs/OUTLINE.md) §11 for the full M0-M7 milestone plan.

## Security Notes

- ⚠ **Never commit `.env`** — it contains secrets
- ⚠ **Dev defaults are insecure** — generate real secrets for any non-dev environment
- ⚠ **M1 uses local KeyStore** for encryption keys (G-8); production MUST use KMS
- See [`../docs/OUTLINE.md`](../docs/OUTLINE.md) §7.9 for full G-8 details

## License

MIT OR Apache-2.0
