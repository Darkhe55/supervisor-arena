-- supervisor_aggregate_snapshots: 均值变动曲线数据源
-- 每小时聚合一次,不存储任何个人级数据
-- 见 C-11 / §6.4
CREATE TABLE supervisor_aggregate_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    supervisor_id UUID NOT NULL REFERENCES supervisors(id) ON DELETE CASCADE,
    snapshot_time TIMESTAMPTZ NOT NULL,

    -- 聚合数据
    composite_mean DOUBLE PRECISION,         -- 综合分均值(0-100)
    dim_means JSONB NOT NULL,                -- 6 维均值 JSON {"research": 75.3, ...}
    consensus DOUBLE PRECISION,              -- 共识度 0-1
    confidence JSONB,                        -- 6 维置信区间 JSON

    -- 元数据
    rating_count INTEGER NOT NULL DEFAULT 0, -- 当时评分条数
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- 唯一:每个 supervisor 每个时间点一个快照
    CONSTRAINT uq_sas_supervisor_time UNIQUE (supervisor_id, snapshot_time)
);

-- 索引(向后查询:最近 90 天)
CREATE INDEX idx_sas_supervisor_time ON supervisor_aggregate_snapshots(supervisor_id, snapshot_time DESC);
CREATE INDEX idx_sas_snapshot_time ON supervisor_aggregate_snapshots(snapshot_time DESC);

COMMENT ON TABLE supervisor_aggregate_snapshots IS 'C-11: Hourly aggregate snapshots for mean trajectory curve';
COMMENT ON COLUMN supervisor_aggregate_snapshots.dim_means IS 'JSON: {"research": 75.3, "resource": 62.0, ...}';
COMMENT ON COLUMN supervisor_aggregate_snapshots.consensus IS '0-1, agreement among raters';
COMMENT ON COLUMN supervisor_aggregate_snapshots.confidence IS 'JSON: 6-dim confidence intervals';
COMMENT ON COLUMN supervisor_aggregate_snapshots.rating_count IS 'How many ratings contributed to this snapshot';
