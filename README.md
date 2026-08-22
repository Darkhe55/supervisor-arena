# Supervisor Arena

> 给导师打分的"众包 + 动态加权"排名系统。
> 核心理念:**没有绝对好坏,所有评分都是相对的、综合的**。

[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)
[![Status](https://img.shields.io/badge/backend-M0%2FM1%2FM2%2FM3%2FM5%20%E5%AE%8C%E6%88%90-brightgreen)]()

## 项目结构

```
supervisor-arena/
├── docs/
│   ├── OUTLINE.md                  # 设计大纲(主参考)
│   └── DECISIONS.md                # 决策清单(H-1..H-53)
├── backend/                        # Rust 后端
│   ├── Cargo.toml
│   ├── docker-compose.yml          # 本地 PG(5433) + Redis
│   ├── .env.example
│   ├── migrations/                 # 17 个 SQL 迁移
│   ├── src/
│   │   ├── main.rs / lib.rs        # 入口
│   │   ├── config.rs               # 12-factor env 配置
│   │   ├── db.rs                   # deadpool-postgres + 启动迁移
│   │   ├── observability.rs        # tracing
│   │   ├── crypto/                 # AES-256-GCM + HMAC + Argon2id + KeyStore trait
│   │   ├── account/                # 注册 / 登录 / JWT / /auth/me / 取消
│   │   ├── supervisor/             # supervisor + alias_generator + 词库
│   │   ├── rating/                 # 评分 + 敏感度检测 + P1 脱敏
│   │   ├── aggregation/            # 综合分 + 学科权重 + composite
│   │   ├── discipline/             # 学科自适应权重投票
│   │   ├── report/                 # 举报 + 审核 SLA
│   │   ├── audit/                  # 加密 audit log
│   │   ├── rate_limit/             # 评分日上限 + 登录 IP 节流
│   │   ├── invitation/             # 邀请码注册
│   │   └── lookup/                 # 学科/学院/维度 i18n 查询
│   └── tests/                      # 200 个测试(lib + proptest + 集成)
├── DEPLOYMENT.md                   # 生产部署手册
├── RUNBOOK.md                      # 运维手册
└── README.md
```

## 技术栈

| 层 | 选型 |
|----|------|
| 后端 | **Rust 2021** + **axum 0.7** + **tokio 1** |
| DB 驱动 | **tokio-postgres 0.7** + **deadpool-postgres 0.14**(Plan B,见 H-11) |
| 数据库 | **PostgreSQL 16** + Postgres FTS |
| 缓存 | Redis 7(预留) |
| 加密 | aes-gcm 0.10 + hmac 0.12 + sha2 0.10 + argon2 0.5 + zeroize 1 |
| Auth | jsonwebtoken 9(HS256) |
| 配置 | dotenvy 0.15 + config 0.14(双下划线 env,见 H-13) |
| 观测 | tracing + tracing-subscriber |
| 前端(M4 规划) | React |

> **不用 sqlx**:Plan B 避开 sqlx 0.8 + Alpine musl PG 的 `ErrorResponse` parser 非 UTF-8 bug。tokio-postgres 是原 PG 协议实现,无此问题。

## 核心特性

- ✅ **多维度评分** — 6 维(科研 / 资源 / 学科适配 / 跟进 / 行为 / 工具)
- ✅ **学科自适应权重** — 用户共同投票决定(M2 完成)
- ✅ **匿名化名 + k-匿名 ≥ 10** — 化名人名白名单校验,跨学科+学院严格 1-to-1
- ✅ **滑块 + 附加信息** — 6 个滑块(必填)+ 每维度附加信息(可选)+ 总附加信息
- ✅ **均值变动曲线**(会员可见,替代"修改时间线")
- ✅ **G-8 加密** — P0/P1/P2/P3 四级,字段级 AES-256-GCM + HMAC-SHA256 + Argon2id
- ✅ **敏感信息禁止** — 科研内容(未发表数据/技术细节)+ 人工审核 + 自动脱敏
- ✅ **举报 → 审核流程**(24h SLA,见 H-50)
- ✅ **评分日上限 + 登录 IP 节流**(M3 §7.6)
- ✅ **邀请码注册**(M5 邀请试用)
- ✅ **加密 audit log**(M6 §7.9.5,8 个调用点全覆盖)
- ❌ **不开放讨论区** / **不做侵权投诉** / **不做导师自证** / **不做导出** / **不做"机构"层**

## 快速开始

```bash
# Clone
git clone git@github.com:Darkhe55/supervisor-arena.git
cd supervisor-arena

# 后端
cd backend
cp .env.example .env
# 编辑 .env —— 必填 AUTH__JWT_SECRET / ENCRYPTION__FIELD_KEY / ENCRYPTION__HMAC_SALT_KEY
docker compose up -d   # PG 在 5433(避 Windows 本地 PG 5432 冲突,见 H-12)
cargo run

# 健康检查
curl http://localhost:8080/health
# {"status":"ok"}
```

> **Windows 注意**: 本地若有 `postgresql-x64-16` 服务会占 `5432`,docker compose 已映射到 `5433`。`.env` 里 `DATABASE__URL` 必须用 `5433` 不是 `5432`。
>
> **PowerShell 注意**: `cargo` / `rustc` 默认不在 PATH,需用 `& "$env:USERPROFILE\.cargo\bin\cargo.exe"`。

## 文档

- **设计大纲** — [`docs/OUTLINE.md`](./docs/OUTLINE.md)
- **决策清单** — [`docs/DECISIONS.md`](./docs/DECISIONS.md)(53 条,H-1..H-53)
- **生产部署** — [`DEPLOYMENT.md`](./DEPLOYMENT.md)
- **运维手册** — [`RUNBOOK.md`](./RUNBOOK.md)
- **后端 README** — [`backend/README.md`](./backend/README.md)

## 里程碑状态

| 里程碑 | 范围 | 状态 |
|---|---|---|
| **M0** | 项目脚手架 + 配置 + 观测 | ✅ 完成 |
| **M1** | 11 表 schema + seed + Crypto/Account/Supervisor/Rating/Aggregation | ✅ 完成 |
| **M2** | 学科自适应权重投票 + composite 加权 | ✅ 完成 |
| **M3** | 软移除过滤 / 举报 / 评分上限 / 登录节流 / 取消 | ✅ 完成(200/200 tests pass) |
| **M4** | React 前端 | ⏸ **DEFERRED**(独立项目,见项目 MEMORY) |
| **M5** | i18n 查询 + 邀请码注册 | ✅ 后端完成;前端 0% |
| **M6** | 加密强化 + audit log + KeyStore trait + KMS stub | ⚠ 后端 5/7;KMS SDK 集成待真实云 |
| **M7** | 极简公开匿名 | ⛔ **BLOCKED** — 需律师法律意见书(G-13) |

测试: 200 个测试全过(lib 171 + proptest 7 + 集成 22,sequential 跑)。

## 安全要点

- ⚠ **所有 secrets 走 env**:`AUTH__JWT_SECRET` / `ENCRYPTION__FIELD_KEY` / `ENCRYPTION__HMAC_SALT_KEY`,`backend/.env` 已在 .gitignore
- ⚠ **M1 dev 用 LocalKeyStore**:M6+ 必须在生产接 KMS(见 H-59 KeyStore trait),KmsKeyStore stub 启动会 fail-closed
- ⚠ **PII 列全部加密**:`email_enc` / `email_hash` / `submitted_name_enc` / `ip_hash` 等(见 migrations)
- ⚠ **k-匿名 ≥ 10**:综合分 / 维度均分都按学科+学院聚合,小组被吸收
- ⚠ **审计可追溯**:`encryption_audit_log` 记录每次字段访问(8 个调用点)

## License

MIT OR Apache-2.0
