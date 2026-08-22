# supervisor-arena backend (Rust)

## Status

M3 + M5 + M6(部分)后端 100% 完成,200/200 测试通过。

- M0 scaffold ✅
- M1 (11 表 schema + seed + Crypto/Account/Supervisor/Rating/Aggregation) ✅
- M2 (学科自适应权重投票 + composite) ✅
- M3 (软移除过滤 / 举报 / 评分上限 / 登录节流 / 取消) ✅
- M5 (i18n 查询 + 邀请码注册) ✅
- M6 (加密强化 + audit log + KeyStore trait + KMS stub) ⚠ 5/7 — KMS SDK 集成待真实云

## Tech Stack

| Layer | Choice |
|-------|--------|
| Language | Rust 1.75+ (edition 2021) |
| Web framework | axum 0.7 |
| Async runtime | tokio 1 |
| DB driver | **tokio-postgres 0.7** + **deadpool-postgres 0.14**(Plan B,H-11) |
| Database | PostgreSQL 16 |
| Cache | Redis 7(reserved) |
| Crypto | aes-gcm + hmac + sha2 + argon2 + zeroize (G-8) |
| Auth | jsonwebtoken 9 (HS256) |
| Config | dotenvy 0.15 + config 0.14(env 双下划线嵌套,H-13) |
| Logging | tracing + tracing-subscriber |
| Errors | thiserror + anyhow |

> **不**用 sqlx(Plan B): sqlx 0.8 + Alpine musl PG 触发 `ErrorResponse` parser 非 UTF-8 bug,绕路用 tokio-postgres 原协议实现。H-11 有完整理由记录。

## Quick Start

### 1. 装依赖

```bash
# Rust 1.75+
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# Windows PowerShell:cargo 不在 PATH,用
#   & "$env:USERPROFILE\.cargo\bin\cargo.exe"
```

### 2. 起 PG + Redis

```bash
cp .env.example .env
docker compose up -d
```

- PostgreSQL → `localhost:5433`(不是 5432 — 避 Windows 本地 PG 16 冲突,见 H-12)
- Redis → `localhost:6379`
- 默认用户: `supervisor / supervisor_dev_pwd`(dev only)

### 3. 生成 dev secrets

```bash
# PowerShell:
$env:AUTH__JWT_SECRET = -join ((1..64) | ForEach-Object { '{0:x2}' -f (Get-Random -Max 256) })
$env:ENCRYPTION__FIELD_KEY = -join ((1..32) | ForEach-Object { '{0:x2}' -f (Get-Random -Max 256) })
$env:ENCRYPTION__HMAC_SALT_KEY = -join ((1..32) | ForEach-Object { '{0:x2}' -f (Get-Random -Max 256) })
# bash:
# export AUTH__JWT_SECRET=$(openssl rand -hex 32)
# export ENCRYPTION__FIELD_KEY=$(openssl rand -hex 32)
# export ENCRYPTION__HMAC_SALT_KEY=$(openssl rand -hex 32)
```

写入 `.env`。**env 变量名用 `__` 双下划线**(`AUTH__JWT_SECRET` 不是 `AUTH_JWT_SECRET`),H-13 锁定。

> **⚠ H-53 待办**:`config.rs::set_default("database.url", ...)` 当前给 dev 凭据做了 fallback,生产忘设 env 不会报错。修法是去掉 `set_default`,启动时强制要求 env 提供。dev 体验用 `.env.example` 兜住。

### 4. Build + run

```bash
cargo run
# 启动自动跑 migrations(17 个),无需手动 `sqlx migrate run`
```

### 5. 验证

```bash
curl http://localhost:8080/health
# {"status":"ok"}
```

## Project Layout

```
backend/
├── Cargo.toml                # dependencies
├── docker-compose.yml        # PG(5433) + Redis
├── .env.example              # 配置模板
├── migrations/               # 17 个 SQL 迁移(20260819*..20260820*)
├── src/
│   ├── main.rs               # binary entry
│   ├── lib.rs                # library entry (run() + router + AppState)
│   ├── config.rs             # 12-factor env + dev defaults
│   ├── db.rs                 # deadpool-postgres pool + 启动迁移
│   ├── observability.rs      # tracing setup
│   ├── crypto/               # AES + HMAC + Argon2id + KeyStore trait + Local/Kms
│   ├── account/              # 注册 / 登录 / JWT / /auth/me / 取消
│   ├── supervisor/           # supervisor 实体 + alias_generator + 词库 + 白名单
│   ├── rating/               # 评分 + 敏感度 + P1 脱敏
│   ├── aggregation/          # 综合分 + 学科权重 + composite
│   ├── discipline/           # 学科自适应权重投票
│   ├── report/               # 举报 + 审核 SLA
│   ├── audit/                # 加密 audit log writer
│   ├── rate_limit/           # 评分日上限 + 登录 IP 节流
│   ├── invitation/           # 邀请码 service + 路由
│   └── lookup/               # 学科/学院/维度 i18n 查询
└── tests/                    # 200 个测试(单元 + proptest + 集成)
```

## Module Quick Map

| Module | Routes / 功能 |
|---|---|
| `account` | `POST /auth/register` `/auth/login` `/auth/refresh` `GET /auth/me` `DELETE /auth/me` |
| `supervisor` | `POST /supervisor/requests` `GET /supervisor/:id` `/supervisor/search` |
| `rating` | `POST /supervisor/:id/ratings` `GET /supervisor/:id/ratings` |
| `aggregation` | `GET /supervisor/:id/aggregate` 内部 composite 计算 |
| `discipline` | `POST /discipline/weights/propose` `/discipline/weights/:id/ballot` |
| `report` | `POST /reports` `PATCH /reports/:id/claim` `/reports/:id/resolve` |
| `invitation` | `POST /invitations` `GET /invitations/:code` 公开邀请码注册 |
| `lookup` | `GET /lookup/disciplines` `/lookup/colleges` `/lookup/rating-dimensions` |
| `rate_limit` | 中间件 + 内存 counter(Redis 预留) |
| `audit` | 加密字段访问 audit log writer |
| `crypto` | AES-256-GCM + HMAC-SHA256 + Argon2id + KeyStore trait + Local/Kms impl |

完整路由列表见 `lib.rs::build_router`。

## Security Notes

- ⚠ **Never commit `.env`** — gitignore line 72 已加
- ⚠ **Dev defaults fail open** — `config.rs::set_default` 当前给 dev 凭据做 fallback,生产忘设 env 不会报错(**H-53 待修**)
- ⚠ **M1-M3 用 LocalKeyStore** — 真实 key bytes 驻在进程内存;生产 **必须** 改用 KmsKeyStore(H-59)
- ⚠ **PII 列全部加密** — `email_enc` / `email_hash` / `submitted_name_enc` / `ip_hash` / `user_agent_hash`(见 migrations)
- ⚠ **审计可追溯** — `encryption_audit_log` 覆盖 8 个调用点(注册/登录/me/取消/举报/投票/supervisor 创建/邀请)
- ⚠ **P1 字段强制脱敏** — `rating::redaction` 自动检测 email / phone / 微信号 / QQ / 链接,写库前过滤

## Testing

```bash
cargo test --lib -- --test-threads=1         # 171 单元 + 7 proptest
cargo test --test full_flow -- --test-threads=1  # 22 集成(testcontainers PG)
```

> ⚠ 集成测试串行跑(testcontainers race flake)。M4 后考虑改 per-test pool 或 mutex 串行。

## Roadmap

| 阶段 | 内容 | 状态 |
|---|---|---|
| M0 | scaffold + config + observability | ✅ |
| M1 | 11 表 schema + Crypto + Account + Supervisor + Rating + Aggregation | ✅ |
| M2 | 学科自适应权重 + composite | ✅ |
| M3 | 软移除 / 举报 / 评分上限 / 登录节流 / 取消 | ✅ |
| M4 | React 前端 | ⏸ DEFERRED |
| M5 | i18n + 邀请码 | ✅ 后端 |
| M6 | 加密强化 + audit + KeyStore + KMS | ⚠ 5/7 |
| M7 | 极简公开匿名 | ⛔ BLOCKED(G-13 律师) |

## License

MIT OR Apache-2.0
