-- M2: Discipline-Adaptive Weights (学科自适应权重)
-- Implements C-2 / OUTLINE §4.4
--
-- Adds 3 tables:
--   discipline_weight_voters   — individual agree/disagree votes per (vote, voter)
--                                (the existing discipline_weight_votes row only
--                                 stores the COUNTs; we need the actual voters to
--                                 enforce one-vote-per-user and audit/rollback)
--   discipline_weights         — current applied weights per (discipline, dim)
--                                (the live snapshot aggregation reads)
--   discipline_weight_history  — full history of every applied/withdrawn weight
--                                (for rollback + audit, never deleted)
--
-- Discipline codes are stored as TEXT (the public code from `disciplines.code`),
-- NOT hashes — votes are about a public discipline identity, and the lookup
-- table is the source of truth (G-12).

-- =========================================================================
-- Migration 13a: extend discipline_weight_votes (M1) with a public
-- discipline_code column. The original M1 schema used `discipline_hash
-- BYTEA` because the table was speculatively designed for a different
-- purpose. For the M2 voting flow we need to filter / list by the
-- public discipline code directly (so we can join to `supervisors.
-- discipline` and to the lookup table without re-hashing). Discipline
-- codes are NOT PII (G-12), so storing them in plaintext is correct.
--
-- The table is empty in M1, so the DEFAULT '' is safe and the new
-- column can be NOT NULL after a backfill (which is a no-op).
-- =========================================================================
ALTER TABLE discipline_weight_votes
    ADD COLUMN discipline_code TEXT NOT NULL DEFAULT '';

-- An index on discipline_code supports the vote-listing path
-- (list_pending_votes, list_recent_votes).
CREATE INDEX idx_dwv_discipline_code ON discipline_weight_votes(discipline_code);


-- =========================================================================
-- Table 1: discipline_weight_voters
-- One row per (vote, voter) — records individual agree/disagree.
-- =========================================================================
CREATE TABLE discipline_weight_voters (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    vote_id UUID NOT NULL REFERENCES discipline_weight_votes(id) ON DELETE CASCADE,
    voter_id UUID NOT NULL REFERENCES accounts(id),
    choice TEXT NOT NULL CHECK (choice IN ('agree', 'disagree')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- A given voter can only vote once on a given proposal.
    CONSTRAINT uq_dwvoter_per_vote UNIQUE (vote_id, voter_id)
);

CREATE INDEX idx_dwvoter_vote ON discipline_weight_voters(vote_id);
CREATE INDEX idx_dwvoter_voter ON discipline_weight_voters(voter_id);
CREATE INDEX idx_dwvoter_choice ON discipline_weight_voters(vote_id, choice);

COMMENT ON TABLE discipline_weight_voters IS 'C-2: Individual agree/disagree ballots on a weight proposal';
COMMENT ON COLUMN discipline_weight_voters.choice IS 'agree | disagree (one per voter per vote)';


-- =========================================================================
-- Table 2: discipline_weights
-- The live "current applied" weight for a (discipline, dim) pair.
-- Upserted on weight application; read by the aggregation path.
--
-- The table is intentionally denormalized — one row per (discipline, dim).
-- Renormalization happens at application time (service layer).
-- =========================================================================
CREATE TABLE discipline_weights (
    discipline TEXT NOT NULL,                  -- FK to disciplines.code (logical)
    dim TEXT NOT NULL CHECK (dim IN ('research', 'resource', 'fit', 'currency', 'ethic', 'tool')),
    weight DOUBLE PRECISION NOT NULL CHECK (weight >= 0 AND weight <= 1),
    -- The vote that produced this weight (audit trail).
    source_vote_id UUID REFERENCES discipline_weight_votes(id) ON DELETE SET NULL,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (discipline, dim)
);

CREATE INDEX idx_dw_discipline ON discipline_weights(discipline);
CREATE INDEX idx_dw_applied_at ON discipline_weights(applied_at DESC);

COMMENT ON TABLE discipline_weights IS 'C-2: Live applied weights per (discipline, dim). Read by aggregation.';
COMMENT ON COLUMN discipline_weights.weight IS '0..=1; sum across 6 dims for a discipline = 1.0 (renormalized at apply)';
COMMENT ON COLUMN discipline_weights.source_vote_id IS 'The proposal that produced this weight (audit only)';


-- =========================================================================
-- Table 3: discipline_weight_history
-- Append-only log of every applied/withdrawn weight. Used for:
--   - Rollback (manually restore a prior version)
--   - Audit / forensics (who changed what when)
--   - Visualization of the weight-over-time chart (M5+ frontend)
-- =========================================================================
CREATE TABLE discipline_weight_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    discipline TEXT NOT NULL,
    dim TEXT NOT NULL,
    old_weight DOUBLE PRECISION,                -- NULL on first application for the (disc,dim)
    new_weight DOUBLE PRECISION NOT NULL,
    source_vote_id UUID REFERENCES discipline_weight_votes(id) ON DELETE SET NULL,
    action TEXT NOT NULL CHECK (action IN ('applied', 'rolled_back', 'rejected')),
    -- Lightweight actor reference; NULL for system actions.
    actor_id UUID REFERENCES accounts(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_dwhistory_discipline_dim ON discipline_weight_history(discipline, dim, created_at DESC);
CREATE INDEX idx_dwhistory_action ON discipline_weight_history(action);
CREATE INDEX idx_dwhistory_created ON discipline_weight_history(created_at DESC);

COMMENT ON TABLE discipline_weight_history IS 'C-2: Immutable weight change log; append-only';
COMMENT ON COLUMN discipline_weight_history.action IS 'applied | rolled_back | rejected';
COMMENT ON COLUMN discipline_weight_history.old_weight IS 'Previous weight for (disc,dim), NULL if first application';


-- =========================================================================
-- Backfill: equal weights (1/6 ≈ 0.16667) for every active discipline × 6 dims.
-- This makes the first launch "all disciplines have equal weights" without
-- requiring a vote to bootstrap the table.
-- =========================================================================
INSERT INTO discipline_weights (discipline, dim, weight)
SELECT d.code, dim.code, 1.0 / 6.0
FROM disciplines d
CROSS JOIN rating_dimensions dim
WHERE d.is_active AND dim.is_active
ON CONFLICT (discipline, dim) DO NOTHING;

-- Log the bootstrap as a history event per (disc, dim) so the chart starts
-- with a visible "applied" point.
INSERT INTO discipline_weight_history (discipline, dim, old_weight, new_weight, action, actor_id)
SELECT d.code, dim.code, NULL, 1.0 / 6.0, 'applied', NULL
FROM disciplines d
CROSS JOIN rating_dimensions dim
WHERE d.is_active AND dim.is_active
ON CONFLICT DO NOTHING;
