-- M5 邀请试用 — link accounts to their inviter
--
-- When a new account registers with a valid invite_code, we set
-- `accounts.invited_by_account_id` to the creator of the code.
-- NULL means the account was not invited (open registration
-- per OUTLINE §7.6).
--
-- The FK is ON DELETE SET NULL so deleting an account doesn't
-- cascade-delete the invitee.

ALTER TABLE accounts
    ADD COLUMN invited_by_account_id UUID REFERENCES accounts(id) ON DELETE SET NULL;

CREATE INDEX idx_accounts_invited_by ON accounts(invited_by_account_id)
    WHERE invited_by_account_id IS NOT NULL;

COMMENT ON COLUMN accounts.invited_by_account_id IS 'M5: the account whose invitation code this user redeemed (NULL = open registration)';
