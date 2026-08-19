-- ratings: 评分(滑块 0-100 + 可选附加信息)
-- 滑块是核心,附加信息是可选的"软评论"
-- 见 §4.1, §6
CREATE TABLE ratings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    supervisor_id UUID NOT NULL REFERENCES supervisors(id) ON DELETE CASCADE,

    -- 评分维度(6 维,见 §3)
    dim TEXT NOT NULL CHECK (dim IN ('research', 'resource', 'fit', 'currency', 'ethic', 'tool')),

    -- 滑块值(默认 0-100,负分受控开放 C-6)
    value SMALLINT NOT NULL CHECK (value >= -100 AND value <= 100),

    -- 附加信息(可选,P2 加密)
    dim_additional_enc BYTEA,                -- 该维度附加信息
    overall_additional_enc BYTEA,            -- 整体附加信息
    additional_level TEXT CHECK (additional_level IN ('L1', 'L2', 'L3', 'L4')),

    -- 证据(URL 数组)
    evidence TEXT[] NOT NULL DEFAULT '{}',

    -- 学科快照(P1,用于学科相关性加权)
    discipline_hash BYTEA,

    -- 时间 + 覆盖关系
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    superseded_by UUID REFERENCES ratings(id),  -- B-9: 覆盖关系

    -- 审核状态(G-11)
    review_status TEXT NOT NULL DEFAULT 'pending_review'
        CHECK (review_status IN ('pending_review', 'approved', 'rejected')),
    review_started_at TIMESTAMPTZ,
    review_completed_at TIMESTAMPTZ,
    reviewer_id UUID REFERENCES accounts(id),
    review_notes TEXT,

    -- 敏感信息检测结果
    sensitivity_flags TEXT,                  -- P0_严禁|P1_脱敏|P2_警告
    redacted_dim_additional_enc BYTEA,        -- 脱敏后版本
    redacted_overall_additional_enc BYTEA
);

-- 索引
CREATE INDEX idx_ratings_account ON ratings(account_id);
CREATE INDEX idx_ratings_supervisor_dim ON ratings(supervisor_id, dim);
CREATE INDEX idx_ratings_superseded ON ratings(superseded_by) WHERE superseded_by IS NOT NULL;
CREATE INDEX idx_ratings_review_status ON ratings(review_status);
CREATE INDEX idx_ratings_created_at ON ratings(created_at DESC);
-- 防止同账号对同 supervisor+dim 多次评分(覆盖关系除外)
CREATE UNIQUE INDEX uq_ratings_one_current
    ON ratings(account_id, supervisor_id, dim)
    WHERE superseded_by IS NULL;

COMMENT ON TABLE ratings IS 'Ratings (slider 0-100 + optional additional info)';
COMMENT ON COLUMN ratings.value IS 'C-1: 0-100 default, -100 to -1 unlocked via C-6 mechanism';
COMMENT ON COLUMN ratings.dim_additional_enc IS 'P2: optional additional info for this dimension';
COMMENT ON COLUMN ratings.overall_additional_enc IS 'P2: optional overall additional info';
COMMENT ON COLUMN ratings.superseded_by IS 'B-9: When same user re-rates, new rating points to old';
COMMENT ON COLUMN ratings.review_status IS 'G-11: pending_review (default) | approved | rejected';
COMMENT ON COLUMN ratings.sensitivity_flags IS 'G-12: P0_strict | P1_redact | P2_warn (auto-detected)';
