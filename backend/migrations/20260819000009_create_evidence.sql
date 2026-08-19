-- evidence: 证据(用于相对修正 §5,可能延后到 M+)
CREATE TABLE evidence (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    subject_supervisor_id UUID NOT NULL REFERENCES supervisors(id) ON DELETE CASCADE,
    comparator_supervisor_id UUID REFERENCES supervisors(id) ON DELETE CASCADE,
    dim TEXT NOT NULL,
    payload TEXT NOT NULL,                    -- URL 或文本
    submitter_id UUID NOT NULL REFERENCES accounts(id),
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'applied', 'contested', 'rejected')),
    applied_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_evidence_subject ON evidence(subject_supervisor_id);
CREATE INDEX idx_evidence_comparator ON evidence(comparator_supervisor_id) WHERE comparator_supervisor_id IS NOT NULL;
CREATE INDEX idx_evidence_status ON evidence(status);
CREATE INDEX idx_evidence_submitter ON evidence(submitter_id);

COMMENT ON TABLE evidence IS 'D-?: Evidence for relative adjustment (M+ feature)';
COMMENT ON COLUMN evidence.payload IS 'URL or text content supporting the comparison';
