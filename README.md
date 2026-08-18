# Supervisor Arena

> 给导师打分的"众包 + 动态加权"排名系统。
> 核心理念:**没有绝对好坏,所有评分都是相对的、综合的**。

[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

## 项目结构

```
supervisor-arena/
├── docs/                    # 项目文档
│   ├── OUTLINE.md           # 设计大纲(主参考)
│   └── DECISIONS.md         # 决策清单
├── backend/                 # Rust 后端
│   ├── Cargo.toml
│   ├── docker-compose.yml   # 本地 PG + Redis
│   ├── .env.example
│   ├── src/
│   │   ├── main.rs          # 二进制入口
│   │   ├── lib.rs           # 库入口
│   │   ├── config.rs
│   │   └── observability.rs
│   └── README.md
└── README.md
```

## 技术栈

| 层 | 选型 |
|----|------|
| 后端 | **Rust** + **axum** 0.7 + **sqlx** 0.8 |
| 数据库 | **PostgreSQL** 16 + Postgres FTS |
| 缓存 | Redis 7 |
| 加密 | aes-gcm + hmac + sha2 + argon2 |
| 前端(规划) | React |

## 核心特性

- ✅ **多维度评分** — 科研 / 资源 / 学科适配 / 跟进 / 行为 / 工具(6 维,初版)
- ✅ **学科自适应权重** — 不同学科维度权重不同,用户共同投票决定
- ✅ **无关化名 + k-匿名 ≥ 10** — 化名人名白名单 10000+ 词校验,跨学科+学院严格 1-to-1
- ✅ **滑块 + 附加信息** — 6 个滑块(必填)+ 每维度附加信息(可选)+ 总附加信息
- ✅ **均值变动曲线**(会员可见,替代"修改时间线")
- ✅ **G-8 加密** — P0/P1/P2/P3 四级,字段级 AES-256-GCM + HMAC-SHA256 + Argon2id
- ✅ **敏感信息禁止** — 科研内容(未发表数据/技术细节)+ 人工审核
- ✅ **对比功能**(任何用户可用)
- ✅ **举报 → 审核流程**
- ❌ **不开放讨论区** / **不做侵权投诉** / **不做导师自证** / **不做导出** / **不做"机构"层**

## 快速开始

```bash
# Clone
git clone git@github.com:Darkhe55/supervisor-arena.git
cd supervisor-arena

# 后端
cd backend
cp .env.example .env
docker compose up -d
cargo run

# 健康检查
curl http://localhost:8080/health
# {"status":"ok","version":"0.1.0"}
```

## 文档

- **设计大纲** — [`docs/OUTLINE.md`](./docs/OUTLINE.md)
- **决策清单** — [`docs/DECISIONS.md`](./docs/DECISIONS.md)
- **后端 README** — [`backend/README.md`](./backend/README.md)

## 当前进度

**Phase 1: 项目脚手架** ✅ 完成
- Rust + axum + sqlx 项目结构
- Docker Compose 本地 PG + Redis
- 配置 / 观测 / 健康检查

**Phase 2-8**: 见 `docs/OUTLINE.md` §11 里程碑

## License

MIT OR Apache-2.0
