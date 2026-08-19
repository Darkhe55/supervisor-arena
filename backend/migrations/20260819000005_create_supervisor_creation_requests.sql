-- supervisor_creation_requests: 导师创建请求(审核员可见 submitted_name 明文)
-- 审核员无映射表查询权,只看用户提交内容
CREATE TABLE supervisor_creation_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    submitter_id UUID NOT NULL REFERENCES accounts(id),
    submitted_name TEXT NOT NULL,            -- 明文,审核员可见
    discipline TEXT NOT NULL,
    college TEXT NOT NULL,
    review_status TEXT NOT NULL DEFAULT 'pending_review'
        CHECK (review_status IN ('pending_review', 'approved', 'rejected')),
    reviewer_id UUID REFERENCES accounts(id),
    review_notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ,
    resolved_supervisor_id UUID REFERENCES supervisors(id),

    -- SLA tracking
    sla_deadline TIMESTAMPTZ NOT NULL        -- created_at + SLA
);

CREATE INDEX idx_scr_submitter ON supervisor_creation_requests(submitter_id);
CREATE INDEX idx_scr_status ON supervisor_creation_requests(review_status);
CREATE INDEX idx_scr_sla ON supervisor_creation_requests(sla_deadline) WHERE review_status = 'pending_review';
CREATE INDEX idx_scr_resolved_supervisor ON supervisor_creation_requests(resolved_supervisor_id);

COMMENT ON TABLE supervisor_creation_requests IS 'G-16: Supervisor creation requests (any user can submit)';
COMMENT ON COLUMN supervisor_creation_requests.submitted_name IS 'Plain text, only visible to reviewer (审核员无映射表查询权)';
COMMENT ON COLUMN supervisor_creation_requests.review_status IS 'G-11: pending | approved | rejected';
COMMENT ON COLUMN supervisor_creation_requests.sla_deadline IS 'Review SLA deadline (24h workday / 72h off-hours)';
