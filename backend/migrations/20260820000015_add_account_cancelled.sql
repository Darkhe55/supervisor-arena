-- M3 §7.4 — Account cancellation.
-- 注销 = 匿名化保留(评分仍计入综合分,身份消失)。
-- 与 `is_banned` 不同:
--   - `is_banned` = admin / 反滥用系统标记,用户理论上能解封
--   - `is_cancelled` = 用户主动申请,不可逆
-- 二者任一为 TRUE 都不允许登录。

ALTER TABLE accounts
    ADD COLUMN is_cancelled BOOLEAN NOT NULL DEFAULT FALSE;

ALTER TABLE accounts
    ADD COLUMN cancelled_at TIMESTAMPTZ;

CREATE INDEX idx_accounts_cancelled ON accounts(is_cancelled) WHERE is_cancelled;

COMMENT ON COLUMN accounts.is_cancelled IS 'M3 §7.4: 主动注销。TRUE 时所有 PII 已匿名化,不可逆';
COMMENT ON COLUMN accounts.cancelled_at IS 'M3 §7.4: 注销时间';

-- 更新 login 拒绝规则:is_cancelled=TRUE 不允许登录
-- (现有的 is_banned 检查保持不变,二者是 OR 关系)
