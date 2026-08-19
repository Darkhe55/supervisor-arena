-- behavior_fingerprints: 行为指纹(用于反刷分 + 同 IP 限制)
CREATE TABLE behavior_fingerprints (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    ip_hash BYTEA NOT NULL,                 -- P1: HMAC-SHA256 of IP
    user_agent_hash BYTEA,                  -- P1
    device_fingerprint_hash BYTEA,          -- P1
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_bf_account ON behavior_fingerprints(account_id);
CREATE INDEX idx_bf_ip_hash ON behavior_fingerprints(ip_hash);
CREATE INDEX idx_bf_recorded_at ON behavior_fingerprints(recorded_at DESC);
CREATE INDEX idx_bf_account_ip ON behavior_fingerprints(account_id, ip_hash, recorded_at DESC);

COMMENT ON TABLE behavior_fingerprints IS 'Behavior fingerprint (P1, for abuse detection)';
COMMENT ON COLUMN behavior_fingerprints.ip_hash IS 'E-4: HMAC-SHA256 of IP (for same-IP limit)';
