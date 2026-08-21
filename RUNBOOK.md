# RUNBOOK.md

> 运维手册:常见问题排查 + 关键运维操作。
>
> 假设:你已读完 [`DEPLOYMENT.md`](DEPLOYMENT.md),服务在 systemd / docker / k8s 里跑着。

---

## 1. 健康检查 / 端点状态

| 端点 | 含义 | 失败说明 |
|------|------|----------|
| `GET /health` | 进程存活 | 进程 crash / OOM / panic loop |
| `GET /health/db` | DB 连通 | 网络 / 凭据 / PG down |
| `GET /health/crypto` | crypto 子系统初始化 | dev 密钥 / KMS 不可达 |
| `GET /version` | 编译时间 + git sha | (永远 200) |

```bash
# 一行 health check
curl -fsS https://api.supervisor-arena.example.com/health \
  && echo OK || echo FAIL
```

---

## 2. 常见问题

### 2.1 启动时 WARN: "LocalKeyStore is using dev placeholder keys"

**症状**:日志有 `LocalKeyStore is using dev placeholder keys (deadbeef* prefix) — DO NOT deploy to production`

**原因**:`ENCRYPTION__FIELD_KEY` 或 `ENCRYPTION__HMAC_SALT_KEY` 以 `deadbeef` 开头(`.env.example` 的占位符)。

**修法**:
1. 立即停服(systemctl stop)
2. 重新生成密钥(`openssl rand -hex 32`)
3. 更新 `env` 文件
4. 重启

如果已经用 dev key 进了生产:所有加密的 PII 都暴露了(因为 dev key 是公开的)。**必须轮换 key + 重新加密**(见 §5)。

### 2.2 `health/crypto` 失败: `key_id` 显示 "KMS-unavailable"

**症状**:`/health/crypto` 返回 503,日志有 `KMS backend 'kms:...' is not yet wired in (M6 stub)`。

**原因**:配置成了 `KmsKeyStore` 但 M6 stub 还没接真 KMS。开发期正常,生产期不正常。

**修法**:
1. 短期:回退到 `LocalKeyStore`(`.env` 移除 `KEY_STORE_BACKEND=kms` 或类似)
2. 长期:实现真 KMS SDK,走 H-59 迁移路径

### 2.3 评分提交 429

**症状**:用户报告"提交不了评分"。

**原因**:`/supervisors/{a}/ratings` POST 触发了 per-account daily 限流(basic 10/d, member 30/d,H-52)。

**排查**:
```bash
# 查 redis / in-memory 计数(目前是 in-memory,跨进程不共享)
journalctl -u supervisor-arena --since '1 hour ago' | grep "rate limit hit"
```

**修法**:
- 短期:让用户等 24h 或升级到 member
- 中期:加 `?override=true` admin-only 路径(M5+ RBAC 落地时)
- 长期:redis-backed counters(M5+)

### 2.4 登录 429

**症状**:用户报告"登录一直 429"。

**原因**:`/auth/login` POST 触发了 per-IP per-minute 限流(5/min,H-54)。

**排查**:
```bash
# 5 分钟内 > 5 次失败登录 → 该 IP 暂时被限
journalctl -u supervisor-arena --since '5 minutes ago' \
  | grep "rate limit hit" | grep login
```

**修法**:
- 短:等 1 分钟
- 中:看是不是某个 IP 在刷(可能是攻击者)
- 长:换 redis-backed(M5+)

### 2.5 取消账号无法登录

**症状**:用户说"我账号怎么登不上去了"。

**原因**:他们调用了 `/auth/cancel`(H-55) — 这是 irreversible 的(OUTLINE §7.4: "纯删除模式不可选")。

**排查**:
```sql
SELECT id, is_cancelled, cancelled_at
FROM accounts
WHERE email_hash = decode('<HMAC of their email>', 'hex');
```

**修法**:没有,他们必须重新注册。这是产品设计。

### 2.6 邀请码不工作

**症状**:用户说"我的邀请码被拒绝"。

**原因**:code 可能是 (a) 不存在 (b) 过期 (c) 已被用完 (d) 已被 admin 撤销。

**排查**:
```sql
SELECT code, max_uses, use_count, expires_at, revoked_at
FROM account_invitations
WHERE code = 'ABCD1234EF56';
```

**修法**:重新生成一个(`POST /invitations`)。

---

## 3. 关键运维操作

### 3.1 Key rotation (M6 spec 要求)

**目前未实现自动轮换** — 启动 WARN 提示"应该 90 天轮换一次",但还没有 re-encryption 流水线。

**手动轮换步骤**(M6 之后的事):

1. 生成新 KEK 1(新 `ENCRYPTION__FIELD_KEY`)和 KEK 2(新 `ENCRYPTION__HMAC_SALT_KEY`)
2. 用新 KEK re-encrypt 所有 `accounts.email_enc`、`ratings.*_additional_enc`、`supervisor_name_mappings.submitted_name_enc` 等密文
3. 期间双 KEK 共存(老读 + 老写 + 新读 + 新写)— 灰度切换
4. 全量切完后,删除老 KEK
5. 更新 `key_id()` 反映新 key fingerprint,审计日志能看出切换点

**实现见 M6+ 路线图**。

### 3.2 软移除老师(soft-remove)

`POST /auth/admin/soft-remove` 把某个账号的 `soft_removed` 设为 TRUE。**效果**:
- 该账号的评分在聚合中被排除(H-48)
- 该账号仍可登录(按 OUTLINE §7.1)
- 现有 audit trail 保留(不删 rating row)

**撤销**:`POST /auth/admin/soft-remove {target_id, value: false}` 即可。

```bash
TOKEN=$(curl -sX POST .../auth/login -d '...' | jq -r .access_token)
curl -X POST .../auth/admin/soft-remove \
    -H "Authorization: Bearer $TOKEN" \
    -H 'content-type: application/json' \
    -d "{\"target_id\": \"$TEACHER_ID\", \"value\": true}"
```

(M5+ 才有 admin role 校验;现在任何 authed user 都能调,这是 M5 RBAC 的 TODO)

### 3.3 封禁账号

`POST /auth/admin/ban` 设 `is_banned = TRUE`:
- 不能登录(403 AccountUnavailable)
- 评分被聚合排除
- 完全不可逆要走 SQL

**慎用**。先用 soft-remove;实在不行再 ban。

### 3.4 查看 audit log

```sql
-- 某账号的所有访问
SELECT purpose, accessor, field_accessed, success, accessed_at
FROM encryption_audit_log
WHERE account_id = '<uuid>'
ORDER BY accessed_at DESC
LIMIT 100;

-- 某 IP 的所有尝试(hash 比较,不是 plaintext)
SELECT purpose, accessor, field_accessed, account_id, success, accessed_at
FROM encryption_audit_log
WHERE ip_hash = decode('<hex>', 'hex')
ORDER BY accessed_at DESC;

-- audit 写入失败次数(健康度)
SELECT count(*) FROM encryption_audit_log
WHERE success = false AND accessed_at > NOW() - INTERVAL '1 day';
```

### 3.5 DB 备份与恢复

**RDS / Cloud SQL**:用 provider 的 snapshot 机制(daily + 7-day PITR)。

**自管**:
```bash
# Backup
docker exec supervisor-arena-postgres pg_dump -U supervisor supervisor_arena \
    | gzip > /backup/sa-$(date +%F).sql.gz

# Restore (警告:会覆盖当前数据)
gunzip -c /backup/sa-2026-08-22.sql.gz | \
    docker exec -i supervisor-arena-postgres psql -U supervisor supervisor_arena
```

**演练**:每月 1 次,验证 backup 能恢复(在 staging 试)。

### 3.6 DB schema 迁移

Migrations 是顺序号(`20260820000017_add_account_invited_by.sql` 之类)。M3+ 不会改老 migrations(只能加新的)。

新增 migration:
```bash
# backend/migrations/20270101000018_add_my_feature.sql
# -- 用比当前 max(20260820000017) 大的序号
# 内容: 写 SQL,带 IF NOT EXISTS 防御性
```

启动时自动跑,失败会回滚整个 transaction。

### 3.7 季度泄露演练(quarterly leak drill)

OUTLINE §7.9 要求"每季度模拟一次"。

**清单**:
1. 找 5 个真实 PII(email / 评分内容)
2. 用脱敏数据重建 dev 环境
3. 让 security team 尝试还原(他们应该只能得到加密的字节)
4. 记录:
   - 用了多长时间还原(应当是 infeasible)
   - 哪些 PII 被脱敏干净了
   - 哪些"漏网"
5. 修补漏洞

**真实环境**:
- snapshot DB → 拿回来
- AES 密文是 GMAC authenticated,改 1 bit 整段就解密失败
- HMAC hash 是单向,无 salt 暴露
- Argon2 慢 hash,brute force 不行

如果 security team 5 分钟还原了 PII — 漏了。追查。

---

## 4. 性能 / 容量 数字 (经验)

实测 (2026-08 dev, PG 16, 4 vCPU):

| 操作 | p50 | p99 |
|------|-----|-----|
| `POST /auth/register` | 350ms | 800ms (Argon2 dominates) |
| `POST /auth/login` | 280ms | 600ms (Argon2 verify) |
| `POST /supervisors/{a}/ratings` | 60ms | 200ms (3 encrypt + 1 hash) |
| `GET /supervisors/by-alias/{a}` | 25ms | 80ms (k-anon + aggregate) |
| `GET /lookup/disciplines` | 5ms | 15ms (cached query) |
| `GET /disciplines/CS/weights` | 8ms | 30ms |

**容量**(粗估):
- 1 台 4 vCPU:可支撑 100 RPS 评分,1000 RPS 查询
- DB IOPS:聚合查询 ~100 IOPS,写入 ~50 IOPS(tps:total)
- 内存:300-500 MB (in-memory 状态 + 连接池)

---

## 5. 错误码速查

| Status | 含义 | 常见原因 |
|--------|------|----------|
| 400 | 请求格式错 | JSON 解析 / 字段验证失败 |
| 401 | 鉴权失败 | token 缺 / 过期 / 错 |
| 403 | 已鉴权但无权限 | banned / soft-removed / cancelled |
| 404 | 资源不存在 | 错误的 alias / code / id |
| 409 | 状态冲突 | 唯一约束 / 已存在 / 已解决 |
| 410 | Gone | 资源已过期(邀请码 / 撤销的 code) |
| 429 | 限流 | daily 满 / per-IP 满 |
| 500 | 服务器错 | DB error / 内部 panic |
| 503 | 子系统不可用 | crypto KMS 不可达 |

---

## 6. 紧急操作

### 6.1 全部流量回滚(严重 bug)

```bash
# 假设前一个版本是 v0.0.1
sudo systemctl stop supervisor-arena
/opt/supervisor-arena/v0.0.1/supervisor-arena &
# 或
docker run -d your-registry.supervisor-arena:v0.0.1
```

5min 内完成。git 历史 + 旧 binary 都在 /opt 留 3 个版本。

### 6.2 紧急封禁整片 IP(被攻击)

目前**没有 in-band 路径**。临时方案:

```bash
sudo iptables -I INPUT -s <attacker_ip> -j DROP
# 或在 LB 防火墙规则加
```

长期:M5+ 加 `/admin/ip-block` 端点。

### 6.3 DB 锁死 / connection 耗尽

```sql
-- 查 active connections
SELECT pid, query, state, wait_event_type, wait_event
FROM pg_stat_activity
WHERE state != 'idle'
ORDER BY query_start;

-- 杀长时间运行的 query
SELECT pg_terminate_backend(<pid>);
```

如果是 migration 卡住,看 `_migrations` 表里哪个没完成。

### 6.4 数据导出 / 用户请求(GDPR-like)

M3 §7.4 的"数据脱敏导出"**未实现**。临时方案:
- 用户要导出 → 后台直接 query DB dump 给他(manual,not user-facing)
- 用户要删除 → 已经用 `/auth/cancel` 匿名化

M5+ 加 user-facing endpoint。

---

## 7. 联系

- **架构 / 代码**:仓库 owner(merged PR 的人)
- **P0 / 紧急**:生产事故群 / on-call 轮值(M5+ RBAC 落地时配)
- **法律 / M7**:见 OUTLINE §11 — 律师意见书后开 M7

---

## 8. 常见配置坑

1. **DB pool size**:default 20,production 单实例足够;多实例 × pool > max_connections
2. **`SERVER__HOST`**:0.0.0.0 暴露所有接口(需要 proxy / firewall);127.0.0.1 仅本机
3. **`REVIEW__MODE=manual`**:会让所有评分 pending,等审核。M5 之前别开
4. **`RATE_LIMIT__LOGIN_PER_MIN=0`**:禁用限流 = 暴露给 brute force,**不要**
5. **`AUTH__JWT_SECRET` < 32 字节**:启动拒绝,但 `< 64 字节`也能过(只是弱)
6. **PostgreSQL `statement_timeout`**:默认无限。production 应设 `5s` 之类
