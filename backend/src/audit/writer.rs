//! Audit log writer (best-effort, async).

use deadpool_postgres::Pool;
use uuid::Uuid;

/// The reason for the access. Stored as a string in the DB (matches
/// the existing `purpose TEXT NOT NULL` schema).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditPurpose {
    /// Account registration, login, or /auth/me.
    Login,
    /// Submitting a rating or supervisor.
    Submit,
    /// Reviewer action (approve / reject / cancel).
    Review,
    /// User-initiated account cancellation.
    Cancellation,
    /// Admin action (set_soft_removed, ban, etc).
    AdminAction,
    /// Any other access.
    Other,
}

impl AuditPurpose {
    pub fn as_db_str(self) -> &'static str {
        match self {
            AuditPurpose::Login => "login",
            AuditPurpose::Submit => "submit",
            AuditPurpose::Review => "review",
            AuditPurpose::Cancellation => "cancellation",
            AuditPurpose::AdminAction => "admin_action",
            AuditPurpose::Other => "other",
        }
    }
}

/// A single access event.
#[derive(Debug, Clone)]
pub struct EncryptionAccess {
    /// Fully qualified field name (e.g. "accounts.email_enc").
    pub field: &'static str,
    /// The account whose data was accessed (None for system access).
    pub account_id: Option<Uuid>,
    /// The service / handler / user_id that performed the access.
    pub accessor: &'static str,
    /// Why the access happened.
    pub purpose: AuditPurpose,
    /// Optional: hash of the requester IP (P1). None if not known.
    pub ip_hash: Option<Vec<u8>>,
    /// Did the access succeed (true) or fail (false)?
    pub success: bool,
}

/// Owns the audit log writer. Held on AppState so handlers can
/// `state.audit.log(...)` without re-allocating the pool reference.
#[derive(Clone)]
pub struct AuditLog {
    pool: Pool,
}

impl AuditLog {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Best-effort log. Returns `Ok(())` on success, or `Ok(())` even
    /// on a DB error (the error is logged via `tracing::warn!` so it
    /// shows up in ops dashboards but does not break the request).
    pub async fn log(&self, access: EncryptionAccess) {
        if let Err(e) = self.try_log(&access).await {
            tracing::warn!(
                field = access.field,
                account_id = ?access.account_id,
                purpose = access.purpose.as_db_str(),
                accessor = access.accessor,
                error = %e,
                "encryption_audit_log write failed (request still succeeded)"
            );
        }
    }

    /// Convenience: derive the `ip_hash` from the request and log.
    /// Use this from handlers that already have `HeaderMap` and
    /// `ConnectInfo<SocketAddr>` extractors — those that don't
    /// just call `log()` directly with `ip_hash: None`.
    pub async fn log_with_ip(
        &self,
        mut access: EncryptionAccess,
        xff: Option<&str>,
        peer: Option<std::net::SocketAddr>,
        hmac_key: &[u8; 32],
    ) {
        access.ip_hash = crate::audit::context::ip_hash_from(xff, peer, hmac_key);
        self.log(access).await;
    }

    async fn try_log(&self, access: &EncryptionAccess) -> Result<(), anyhow::Error> {
        let c = self.pool.get().await?;
        c.execute(
            "INSERT INTO encryption_audit_log
                (field_accessed, account_id, accessor, purpose, ip_hash, success)
             VALUES ($1::text, $2, $3::text, $4::text, $5, $6::bool)",
            &[
                &access.field,
                &access.account_id,
                &access.accessor,
                &access.purpose.as_db_str(),
                &access.ip_hash,
                &access.success,
            ],
        )
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purpose_as_db_str_is_stable() {
        // The DB stores the string verbatim; changing any of these
        // would break historical query compatibility.
        assert_eq!(AuditPurpose::Login.as_db_str(), "login");
        assert_eq!(AuditPurpose::Submit.as_db_str(), "submit");
        assert_eq!(AuditPurpose::Review.as_db_str(), "review");
        assert_eq!(AuditPurpose::Cancellation.as_db_str(), "cancellation");
        assert_eq!(AuditPurpose::AdminAction.as_db_str(), "admin_action");
        assert_eq!(AuditPurpose::Other.as_db_str(), "other");
    }

    #[test]
    fn access_construction_smoke() {
        let a = EncryptionAccess {
            field: "accounts.email_enc",
            account_id: Some(Uuid::new_v4()),
            accessor: "account::service::login",
            purpose: AuditPurpose::Login,
            ip_hash: Some(vec![0xAB; 32]),
            success: true,
        };
        assert_eq!(a.field, "accounts.email_enc");
        assert_eq!(a.purpose, AuditPurpose::Login);
    }

    #[test]
    fn access_handles_optional_fields() {
        let a = EncryptionAccess {
            field: "ratings.overall_additional_enc",
            account_id: None,
            accessor: "system::crawler",
            purpose: AuditPurpose::Other,
            ip_hash: None,
            success: false,
        };
        assert!(a.account_id.is_none());
        assert!(a.ip_hash.is_none());
    }

    // Suppress unused-import warnings in test builds (no chrono use here).
}
