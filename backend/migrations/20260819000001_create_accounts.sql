-- accounts: 系统账号(P0/P1/P2 字段加密,见 G-8)
CREATE TABLE accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- P0 极度敏感
    email_enc BYTEA NOT NULL,                -- AES-256-GCM(可还原,用于登录)
    email_hash BYTEA NOT NULL UNIQUE,        -- HMAC-SHA256(用于按邮箱查找)

    password_hash TEXT NOT NULL,             -- Argon2id(不可还原)

    -- P1 高度敏感(用于学科相关性加权 + 老师软移除检测)
    discipline_hash BYTEA NOT NULL,
    institution_hash BYTEA NOT NULL,

    -- P2 准敏感(选填年级)
    grade_enc BYTEA,

    -- 业务字段
    tier TEXT NOT NULL DEFAULT 'basic'
        CHECK (tier IN ('basic', 'member')),
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_active_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    soft_removed BOOLEAN NOT NULL DEFAULT FALSE,  -- 老师软移除标记
    is_banned BOOLEAN NOT NULL DEFAULT FALSE,

    -- 负分机制触发状态(简化版,完整版在 Phase 5)
    negative_score_unlocked_supervisors UUID[] NOT NULL DEFAULT '{}'
);

-- 索引
CREATE INDEX idx_accounts_email_hash ON accounts(email_hash) WHERE NOT soft_removed;
CREATE INDEX idx_accounts_discipline_hash ON accounts(discipline_hash);
CREATE INDEX idx_accounts_institution_hash ON accounts(institution_hash);
CREATE INDEX idx_accounts_tier ON accounts(tier) WHERE tier = 'member';
CREATE INDEX idx_accounts_soft_removed ON accounts(soft_removed) WHERE soft_removed;

COMMENT ON TABLE accounts IS 'User accounts (P0/P1/P2 encrypted fields per G-8)';
COMMENT ON COLUMN accounts.email_enc IS 'P0: AES-256-GCM encrypted email (reversible)';
COMMENT ON COLUMN accounts.email_hash IS 'HMAC-SHA256 of email (for lookup)';
COMMENT ON COLUMN accounts.password_hash IS 'Argon2id hashed password';
COMMENT ON COLUMN accounts.discipline_hash IS 'P1: HMAC-SHA256 of declared discipline';
COMMENT ON COLUMN accounts.institution_hash IS 'P1: HMAC-SHA256 of declared institution';
COMMENT ON COLUMN accounts.soft_removed IS 'E-1: Teacher soft-removed (silently drops ratings)';
COMMENT ON COLUMN accounts.negative_score_unlocked_supervisors IS 'C-6: Supervisors where negative scores are unlocked for this account';
