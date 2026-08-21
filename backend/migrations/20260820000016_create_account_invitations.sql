-- M5 邀请试用 (Invitation trial)
-- See OUTLINE §11 / M5 邀请试用.
--
-- Schema: any user can generate an invitation code. The code can
-- then be redeemed by a new registrant (during /auth/register).
-- Codes are single-use by default (max_uses=1) and can be
-- time-limited (expires_at).
--
-- We don't gate registration behind an invite (open
-- registration per OUTLINE §7.6) — invitation is an *optional*
-- track that lets the backend tag new users as "invited" so the
-- frontend can show a different UX (e.g. "you're an early user,
-- thanks!").
--
-- The schema is intentionally simple — no tier, no role, no
-- permission. Those can be added in M5+ if the product wants.

CREATE TABLE account_invitations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- The human-typeable code (e.g. "7K9X-3RT1-A82B"). We store
    -- the canonical form (uppercase, no dashes) and the lookup
    -- is case-insensitive (we lowercase on insert and on read).
    code TEXT NOT NULL UNIQUE,

    -- Audit
    created_by UUID REFERENCES accounts(id),       -- nullable for system-generated
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Redemption
    used_by UUID REFERENCES accounts(id),         -- the account that redeemed this
    used_at TIMESTAMPTZ,                          -- when it was redeemed

    -- Lifecycle
    max_uses INTEGER NOT NULL DEFAULT 1 CHECK (max_uses >= 1 AND max_uses <= 1000),
    use_count INTEGER NOT NULL DEFAULT 0 CHECK (use_count >= 0),
    expires_at TIMESTAMPTZ,                       -- NULL = no expiry
    revoked_at TIMESTAMPTZ,                       -- manual kill switch
    note TEXT                                      -- free-form: "for Alice (CS @ MIT)"
);

CREATE INDEX idx_ai_code ON account_invitations(code);
CREATE INDEX idx_ai_created_by ON account_invitations(created_by);
CREATE INDEX idx_ai_used_by ON account_invitations(used_by);
CREATE INDEX idx_ai_active ON account_invitations(expires_at)
    WHERE revoked_at IS NULL;

COMMENT ON TABLE account_invitations IS 'M5: Invitation codes. Optional path; registration is open per OUTLINE §7.6.';
COMMENT ON COLUMN account_invitations.code IS 'Case-insensitive on read; stored as uppercase';
COMMENT ON COLUMN account_invitations.max_uses IS 'How many times the code can be redeemed (1..=1000)';
COMMENT ON COLUMN account_invitations.use_count IS 'Number of successful redemptions so far';
COMMENT ON COLUMN account_invitations.note IS 'Free-form context (who is the code for, why, etc.)';
