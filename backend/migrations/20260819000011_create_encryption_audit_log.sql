-- encryption_audit_log: 加密审计日志
-- 任何 P0/P1/P2 字段的读/写都记录
-- 见 G-8 / §7.9.5
CREATE TABLE encryption_audit_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    field_accessed TEXT NOT NULL,            -- 如 "Account.email_enc"
    account_id UUID REFERENCES accounts(id),  -- 涉及账号(可能 null,如系统访问)
    accessor TEXT NOT NULL,                   -- 访问者(service name 或 user_id)
    purpose TEXT NOT NULL,                    -- login | export | review | other
    accessed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ip_hash BYTEA,                            -- 访问者 IP(P1)
    success BOOLEAN NOT NULL DEFAULT TRUE     -- 是否成功
);

CREATE INDEX idx_eal_field ON encryption_audit_log(field_accessed);
CREATE INDEX idx_eal_account ON encryption_audit_log(account_id) WHERE account_id IS NOT NULL;
CREATE INDEX idx_eal_accessed_at ON encryption_audit_log(accessed_at DESC);
CREATE INDEX idx_eal_success ON encryption_audit_log(success) WHERE NOT success;

COMMENT ON TABLE encryption_audit_log IS 'G-8: Audit log for P0/P1/P2 field access';
COMMENT ON COLUMN encryption_audit_log.field_accessed IS 'Fully qualified field name (Table.column)';
COMMENT ON COLUMN encryption_audit_log.accessor IS 'Service name or user_id performing the access';
COMMENT ON COLUMN encryption_audit_log.purpose IS 'Reason for access: login | export | review | other';
