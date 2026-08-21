# DEPLOYMENT.md

> 把 supervisor-arena backend 部署到生产环境的完整步骤。
>
> 假设:Debian/Ubuntu 服务器,Docker 已装,有 HTTPS 终止(proxy / LB / ingress)。

---

## 0. 部署前清单

- [ ] **法律**:M7 公开匿名需要律师意见书(没拿到前别开 M7)
- [ ] **域名 + TLS 证书**(e.g. Caddy / Traefik / nginx 自动)
- [ ] **PostgreSQL 16**(RDS / Cloud SQL / 自管)
- [ ] **密钥管理**:至少能存 3 个 32-byte 密钥(HMAC + AES + JWT)。H-59 的 `KmsKeyStore` 留接缝,但 M6 阶段我们用 `LocalKeyStore` 把密钥以 hex 形式放在环境变量 / secret manager 里
- [ ] **日志聚合**(Loki / Datadog / CloudWatch)
- [ ] **HTTPS 终止**(LB / ingress 强制 HSTS)
- [ ] **CVE 扫描**(Dependabot / Snyk / cargo-audit)

---

## 1. 生成密钥(只在 dev 机执行一次)

```bash
# 加密 key (AES-256-GCM field encryption)
openssl rand -hex 32
# → 64 hex chars → 写到 ENCRYPTION__FIELD_KEY

# HMAC key (P1 fields: email, discipline, institution, IP)
openssl rand -hex 32
# → 64 hex chars → 写到 ENCRYPTION__HMAC_SALT_KEY

# JWT secret (HS256)
openssl rand -hex 64
# → 128 hex chars → 写到 AUTH__JWT_SECRET
```

**不要复用密钥**。每个 key 是独立 32 字节随机。

`ENCRYPTION__KEY_ROTATION_DAYS=90` 启动时 WARN(目前还不自动轮换,只记 — 见 [`RUNBOOK.md`](RUNBOOK.md) 的 "Key rotation" 章节)。

---

## 2. 启动 PostgreSQL

### 选项 A: managed(推荐生产)

- AWS RDS / GCP Cloud SQL / Azure Database / Aliyun RDS
- 16+ minor version
- 至少 20 GB SSD, 100 连接上限
- 备份策略:daily snapshot, 7 天保留

### 选项 B: 自管(便宜但要自己负责)

`backend/docker-compose.yml` 是 dev 配置,生产用 systemd:

```yaml
# /etc/docker-compose.d/supervisor-arena.yml
services:
  postgres:
    image: postgres:16
    restart: unless-stopped
    environment:
      POSTGRES_USER: supervisor
      POSTGRES_PASSWORD: <from-secret-manager>
      POSTGRES_DB: supervisor_arena
    volumes:
      - /var/lib/supervisor-arena/pgdata:/var/lib/postgresql/data
      - ./backend/docker/postgres/init.sql:/docker-entrypoint-initdb.d/00-init.sql
    command: >
      postgres
        -c shared_buffers=256MB
        -c max_connections=200
        -c log_min_duration_statement=500ms
    ports: []  # 不要暴露到 0.0.0.0,让 backend 通过 docker network 访问
```

---

## 3. 部署 backend

### 3.1 编译 release binary

```bash
git clone https://github.com/Darkhe55/supervisor-arena.git
cd supervisor-arena/backend
cargo build --release --bin supervisor-arena
# 产物: target/release/supervisor-arena
```

(交叉编译 / Docker image 看 3.2)

### 3.2 推荐: Docker image

`backend/Dockerfile`(M5+ 还没写,模板如下):

```dockerfile
FROM rust:1.80-slim-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
RUN cargo build --release --bin supervisor-arena

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/supervisor-arena /usr/local/bin/
EXPOSE 8080
USER nobody
ENTRYPOINT ["/usr/local/bin/supervisor-arena"]
```

Build + push:

```bash
docker build -t your-registry.supervisor-arena:v0.0.1 -f backend/Dockerfile backend/
docker push your-registry.supervisor-arena:v0.0.1
```

### 3.3 systemd 部署(裸机)

`/etc/systemd/system/supervisor-arena.service`:

```ini
[Unit]
Description=supervisor-arena backend
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=supervisor-arena
Group=supervisor-arena
WorkingDirectory=/opt/supervisor-arena
ExecStart=/opt/supervisor-arena/supervisor-arena
Restart=on-failure
RestartSec=5s

# Environment — 用 EnvironmentFile= 引用 secret(只 root 可读)
EnvironmentFile=/etc/supervisor-arena/env

# Hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
ReadWritePaths=/var/log/supervisor-arena

# Logging
StandardOutput=append:/var/log/supervisor-arena/app.log
StandardError=append:/var/log/supervisor-arena/app.log

[Install]
WantedBy=multi-user.target
```

`/etc/supervisor-arena/env` (root:root, 0600):

```ini
DATABASE__URL=postgres://supervisor:...@db.internal:5432/supervisor_arena
SERVER__HOST=0.0.0.0
SERVER__PORT=8080
AUTH__JWT_SECRET=<...>
ENCRYPTION__FIELD_KEY=<...>
ENCRYPTION__HMAC_SALT_KEY=<...>
ENCRYPTION__KEY_ROTATION_DAYS=90
REVIEW__MODE=auto_pass
RATE_LIMIT__RATINGS_PER_DAY_BASIC=10
RATE_LIMIT__RATINGS_PER_DAY_MEMBER=30
RATE_LIMIT__LOGIN_PER_MIN=5
RUST_LOG=info,supervisor_arena=info
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now supervisor-arena
sudo systemctl status supervisor-arena
```

---

## 4. HTTPS / 代理

让 LB / ingress 终止 TLS,backend 收 HTTP。

**关键**:proxy 必须传 `X-Forwarded-For` 头(我们用第一跳算 ip_hash — H-54)。
生产部署时,**覆盖** 客户端发的 XFF(信任 LB 内网 IP 段),否则攻击者能伪造 IP 绕过 per-IP rate limit。

Caddyfile 例子:

```caddyfile
api.supervisor-arena.example.com {
    reverse_proxy 127.0.0.1:8080 {
        header_up X-Forwarded-For {remote_host}
        header_up X-Real-IP {remote_host}
    }
    encode zstd gzip
    header Strict-Transport-Security "max-age=31536000; includeSubDomains"
    header X-Content-Type-Options "nosniff"
    header X-Frame-Options "DENY"
}
```

---

## 5. 启动后验证

```bash
# 1. 存活
curl https://api.supervisor-arena.example.com/health
# → {"status":"ok","version":"..."}

# 2. DB 连通
curl https://api.supervisor-arena.example.com/health/db
# → {"status":"ok"}

# 3. crypto 子系统
curl https://api.supervisor-arena.example.com/health/crypto
# → {"status":"ok","key_id":"local:abcd1234..."}  ← 应该不是 dev-placeholder

# 4. migrations 跑完
SELECT version, description FROM _migrations ORDER BY version;
# → 17 rows, latest 是 20260820000017_add_account_invited_by

# 5. 端到端 smoke
curl -X POST https://api.supervisor-arena.example.com/auth/register \
    -H 'content-type: application/json' \
    -d '{"email":"smoke@example.com","password":"Test1234abcd","discipline":"CS","institution":"Test"}'
# → 201 Created
```

---

## 6. KeyStore 迁移到真 KMS(H-59)

目前我们用 `LocalKeyStore`(`KeyStore` trait 实现),密钥以 hex 形式存在环境变量。

**生产 KMS 迁移路径**:

1. 选一个 cloud KMS(AWS KMS / Aliyun KMS / HashiCorp Vault)
2. 实现 `KmsKeyStore: KeyStore` trait:
   - `field_key()`: 调 KMS Decrypt 解封 wrapped data key (DEK)
   - `hmac_key()`: 同上
   - DEK 用 `GenerateDataKey` 一次生成,wrap 后存到密钥存储后端
3. 改 `lib.rs::run` 一行:`Arc::new(local)` → `Arc::new(KmsKeyStore::new(...))`
4. 部署一个 ENVELOPE_ENCRYPTION 迁移:用新 DEK re-encrypt 旧 ciphertext row

**M6 之后的事**,M1-M5 backend 不需要这一步。

---

## 7. 监控 / 告警

最小集:

| 指标 | 告警阈值 |
|------|----------|
| HTTP 5xx rate | > 1% in 5min |
| p99 latency | > 1s |
| DB connection pool saturation | > 80% |
| Audit log write failures | > 0 in 1h |
| 取消账号数 / day | 异常 spike |
| Soft-removed 数 | 异常 spike |
| 评分提交数 | 异常 spike (rate limit 触发) |

接 Prometheus / Datadog / CloudWatch 都行 — backend 已经是 structured JSON 日志。

异常告警(M6 spec 要求)目前**未实现**。M5+ 加上"IP-账号异常关联" 检测逻辑(behavior fingerprinting)。

---

## 8. 部署后必做

- [ ] **监控 + 告警**接好(上面的表)
- [ ] **DB 备份**验证过恢复流程(每月一次 dry-run)
- [ ] **JWT secret** 入 secret manager(不写在 env 文件里)
- [ ] **ENCRYPTION keys** 入 KMS(Stage 1 可以先 env,Stage 2 必须 KMS)
- [ ] **HTTPS 强制 HSTS**(看 4 Caddyfile)
- [ ] **rate limit** 验证生效(cURL 6 次 login 看 6th 是 429)
- [ ] **audit log** 接 SIEM(看 [`RUNBOOK.md`](RUNBOOK.md) 的 "查询 audit log" 章节)
- [ ] **依赖 CVE 扫描**:`cargo audit` CI step 装上
- [ ] **季度泄露演练**:process,见 RUNBOOK
- [ ] **rollback plan**:`git revert` + 旧 binary 还留着,5min 回滚
