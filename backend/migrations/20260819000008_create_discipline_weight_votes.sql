-- discipline_weight_votes: 学科自适应权重投票
-- 见 C-2 / §4.4
CREATE TABLE discipline_weight_votes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    discipline_hash BYTEA NOT NULL,          -- P1
    dim TEXT NOT NULL CHECK (dim IN ('research', 'resource', 'fit', 'currency', 'ethic', 'tool')),
    proposed_weight DOUBLE PRECISION NOT NULL CHECK (proposed_weight >= 0 AND proposed_weight <= 1),
    proposer_id UUID NOT NULL REFERENCES accounts(id),

    -- 投票计数
    agree_count INTEGER NOT NULL DEFAULT 0,
    disagree_count INTEGER NOT NULL DEFAULT 0,

    -- 状态
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'applied', 'rejected')),
    applied_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- 备注
    reason TEXT                                -- 投票理由
);

CREATE INDEX idx_dwv_discipline ON discipline_weight_votes(discipline_hash);
CREATE INDEX idx_dwv_status ON discipline_weight_votes(status);
CREATE INDEX idx_dwv_proposer ON discipline_weight_votes(proposer_id);
CREATE INDEX idx_dwv_created_at ON discipline_weight_votes(created_at DESC);

COMMENT ON TABLE discipline_weight_votes IS 'C-2: Subject-adaptive weight voting';
COMMENT ON COLUMN discipline_weight_votes.proposed_weight IS '0-1, new weight for this dim in this discipline';
COMMENT ON COLUMN discipline_weight_votes.status IS 'pending | applied | rejected';
