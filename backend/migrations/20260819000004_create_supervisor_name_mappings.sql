-- supervisor_name_mappings: 后台原始名映射(⚠ P0 加密,物理隔离)
-- 平台零映射通道,审核员无查询权,见 G-15 / §7.10.1
CREATE TABLE supervisor_name_mappings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    supervisor_id UUID NOT NULL REFERENCES supervisors(id) ON DELETE CASCADE,

    -- P0: 用户提交的任意名称(可能是真名/化名/乱码)
    submitted_name_enc BYTEA NOT NULL,       -- AES-256-GCM(可还原,仅内部审计用)
    submitted_name_hash BYTEA NOT NULL,      -- HMAC-SHA256(用于去重)
    discipline_hash BYTEA NOT NULL,          -- P1(去重用)
    college_hash BYTEA NOT NULL,             -- P1(去重用)

    -- 平台生成的化名
    generated_alias TEXT NOT NULL,
    alias_generation_seed TEXT,               -- 生成种子(可复现)

    created_by UUID NOT NULL REFERENCES accounts(id),  -- 创建者(审计)
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- 唯一约束
    -- G-19: 同一"原始名 + 学科 + 学院" = 同一条目
    CONSTRAINT uq_snm_dedup UNIQUE (submitted_name_hash, discipline_hash, college_hash),
    -- G-20: 化名严格 1-to-1,跨学科+学院不可重用
    CONSTRAINT uq_snm_alias UNIQUE (generated_alias)
);

CREATE INDEX idx_snm_supervisor ON supervisor_name_mappings(supervisor_id);
CREATE INDEX idx_snm_dedup ON supervisor_name_mappings(submitted_name_hash, discipline_hash, college_hash);
CREATE INDEX idx_snm_created_by ON supervisor_name_mappings(created_by);

COMMENT ON TABLE supervisor_name_mappings IS 'G-15: Back-end submitted-name mapping (P0 encrypted, physical isolation)';
COMMENT ON COLUMN supervisor_name_mappings.submitted_name_enc IS 'P0: AES-256-GCM encrypted user-submitted name';
COMMENT ON COLUMN supervisor_name_mappings.submitted_name_hash IS 'HMAC-SHA256 of submitted name (for dedup, G-19)';
COMMENT ON COLUMN supervisor_name_mappings.generated_alias IS 'G-17: Platform-generated alias (unrelated to real names)';
