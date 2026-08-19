-- reports: 举报(任何用户可举报,后台审核)
-- 见 G-3 / §7.10.7
CREATE TABLE reports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    reporter_id UUID NOT NULL REFERENCES accounts(id),

    -- 目标
    target_type TEXT NOT NULL
        CHECK (target_type IN ('rating', 'supervisor', 'additional_info')),
    target_id UUID NOT NULL,

    -- 举报原因
    reason TEXT NOT NULL
        CHECK (reason IN ('defamation', 'insult', 'privacy', 'research_leak', 'other')),
    description TEXT,

    -- 状态
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'reviewing', 'resolved', 'dismissed')),
    reviewer_id UUID REFERENCES accounts(id),
    resolution TEXT
        CHECK (resolution IN ('removed', 'warned', 'rejected', 'no_action')),

    -- 时间
    submitted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ,
    sla_deadline TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_reports_target ON reports(target_type, target_id);
CREATE INDEX idx_reports_status ON reports(status);
CREATE INDEX idx_reports_sla ON reports(sla_deadline) WHERE status IN ('pending', 'reviewing');
CREATE INDEX idx_reports_reporter ON reports(reporter_id);

COMMENT ON TABLE reports IS 'G-3: User reports (any user can report, backend review)';
COMMENT ON COLUMN reports.target_type IS 'What is being reported: rating | supervisor | additional_info';
COMMENT ON COLUMN reports.reason IS 'defamation | insult | privacy | research_leak | other';
COMMENT ON COLUMN reports.resolution IS 'removed | warned | rejected | no_action';
