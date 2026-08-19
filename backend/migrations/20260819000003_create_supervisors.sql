-- supervisors: 导师条目(公开视图)
-- 公开字段仅:public_code + discipline + college + 综合分 + 雷达图
-- 化名与真人无关,见 G-13 / §7.10
CREATE TABLE supervisors (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- 公开字段
    public_code TEXT NOT NULL UNIQUE,       -- 平台生成的化名
    discipline TEXT NOT NULL,               -- 学科
    college TEXT NOT NULL,                   -- 学院

    -- 缓存的综合分 + 雷达图(可选,由后台聚合任务更新)
    composite_score DOUBLE PRECISION,        -- 0-100(均值不低于 0)
    radar_dimensions JSONB,                  -- 6 维 JSON

    -- 状态
    review_status TEXT NOT NULL DEFAULT 'pending_review'
        CHECK (review_status IN ('pending_review', 'approved', 'rejected', 'hidden')),
    k_anonymity_count INTEGER NOT NULL DEFAULT 0,  -- 同"学科+学院"分类下活跃数
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- 合并:当多个 submitted_name 命中同一真人,合并到主条目
    merged_into UUID REFERENCES supervisors(id) ON DELETE SET NULL
);

-- 索引
CREATE INDEX idx_supervisors_public_code ON supervisors(public_code);
-- k-匿名:仅当 status=approved 且 k_anonymity_count >= 10 才显示
CREATE INDEX idx_supervisors_discipline_college ON supervisors(discipline, college)
    WHERE review_status = 'approved';
CREATE INDEX idx_supervisors_status ON supervisors(review_status);
CREATE INDEX idx_supervisors_merged ON supervisors(merged_into) WHERE merged_into IS NOT NULL;

COMMENT ON TABLE supervisors IS 'Supervisor entries (public view)';
COMMENT ON COLUMN supervisors.public_code IS 'G-13: Platform-generated alias, unrelated to real names';
COMMENT ON COLUMN supervisors.composite_score IS 'C-3: Composite score (0-100, mean NOT below 0 publicly)';
COMMENT ON COLUMN supervisors.k_anonymity_count IS 'G-14: Count of approved supervisors in same (discipline, college); < 10 = hidden';
COMMENT ON COLUMN supervisors.review_status IS 'G-11: pending_review | approved | rejected | hidden (k-anon < 10)';
