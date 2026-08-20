//! Input validation for account registration
//!
//! Validation is intentionally **explicit** (not via the `validator` derive
//! crate) so that error messages can be tied directly to `AccountError`
//! variants. Length caps are documented here for the API contract.

use super::error::AccountError;

/// Maximum length for free-form fields. Anything longer is almost certainly
/// a client bug or an attack.
pub const MAX_EMAIL_LEN: usize = 254; // RFC 5321
pub const MAX_PASSWORD_LEN: usize = 128; // Argon2id PHC inputs are bounded
pub const MAX_DISCIPLINE_LEN: usize = 64;
pub const MAX_INSTITUTION_LEN: usize = 200;
pub const MAX_GRADE_LEN: usize = 32;

/// Minimum password length. See DECISIONS.md H-19 for the rationale.
pub const MIN_PASSWORD_LEN: usize = 12;

/// Email format check.
///
/// We do a deliberately loose check here — the canonical source of truth
/// is "does the user get a confirmation email" (M5+). For M4, the check
/// is just: non-empty, length-capped, has exactly one '@', local part
/// contains no whitespace, domain contains at least one '.' and no
/// whitespace.
pub fn validate_email(email: &str) -> Result<(), AccountError> {
    if email.is_empty() {
        return Err(AccountError::InvalidEmail("empty".into()));
    }
    if email.len() > MAX_EMAIL_LEN {
        return Err(AccountError::InvalidEmail(format!(
            "longer than {MAX_EMAIL_LEN} chars"
        )));
    }
    if email.chars().any(|c| c.is_whitespace()) {
        return Err(AccountError::InvalidEmail("contains whitespace".into()));
    }
    let mut parts = email.split('@');
    let local = parts.next().unwrap_or("");
    let domain = parts.next().unwrap_or("");
    if local.is_empty() || domain.is_empty() {
        return Err(AccountError::InvalidEmail("missing local or domain".into()));
    }
    if parts.next().is_some() {
        return Err(AccountError::InvalidEmail("more than one '@'".into()));
    }
    if !domain.contains('.') {
        return Err(AccountError::InvalidEmail("domain has no dot".into()));
    }
    Ok(())
}

/// Password strength check.
///
/// M4 policy: minimum 12 characters, must contain at least one ASCII letter
/// and at least one ASCII digit. This is the absolute minimum; users with
/// real accounts are expected to use a password manager and a unique
/// password. No upper-case / symbol requirements (NIST SP 800-63B §5.1.1.2
/// recommends against composition rules — they encourage predictable
/// patterns like `Password1!`).
pub fn validate_password(password: &str) -> Result<(), AccountError> {
    if password.len() < MIN_PASSWORD_LEN {
        return Err(AccountError::WeakPassword(format!(
            "shorter than {MIN_PASSWORD_LEN} characters"
        )));
    }
    if password.len() > MAX_PASSWORD_LEN {
        return Err(AccountError::WeakPassword(format!(
            "longer than {MAX_PASSWORD_LEN} characters"
        )));
    }
    let has_letter = password.chars().any(|c| c.is_ascii_alphabetic());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    if !has_letter || !has_digit {
        return Err(AccountError::WeakPassword(
            "must contain at least one letter and one digit".into(),
        ));
    }
    Ok(())
}

pub fn validate_discipline(s: &str) -> Result<(), AccountError> {
    if s.is_empty() {
        return Err(AccountError::InvalidField {
            field: "discipline",
            message: "empty".into(),
        });
    }
    if s.len() > MAX_DISCIPLINE_LEN {
        return Err(AccountError::InvalidField {
            field: "discipline",
            message: format!("longer than {MAX_DISCIPLINE_LEN} chars"),
        });
    }
    Ok(())
}

pub fn validate_institution(s: &str) -> Result<(), AccountError> {
    if s.is_empty() {
        return Err(AccountError::InvalidField {
            field: "institution",
            message: "empty".into(),
        });
    }
    if s.len() > MAX_INSTITUTION_LEN {
        return Err(AccountError::InvalidField {
            field: "institution",
            message: format!("longer than {MAX_INSTITUTION_LEN} chars"),
        });
    }
    Ok(())
}

pub fn validate_grade(s: &str) -> Result<(), AccountError> {
    if s.len() > MAX_GRADE_LEN {
        return Err(AccountError::InvalidField {
            field: "grade",
            message: format!("longer than {MAX_GRADE_LEN} chars"),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_happy_paths() {
        assert!(validate_email("user@example.com").is_ok());
        assert!(validate_email("first.last@sub.example.co.uk").is_ok());
        assert!(validate_email("a+tag@b.c").is_ok());
    }

    #[test]
    fn email_rejects_garbage() {
        assert!(validate_email("").is_err());
        assert!(validate_email("no-at-sign").is_err());
        assert!(validate_email("@no-local.com").is_err());
        assert!(validate_email("no-domain@").is_err());
        assert!(validate_email("two@@signs.com").is_err());
        assert!(validate_email("white space@example.com").is_err());
        assert!(validate_email("no-dot@example").is_err());
    }

    #[test]
    fn password_rejects_short() {
        assert!(validate_password("Aa1").is_err());
        assert!(validate_password("12345678ab").is_err()); // 10 chars
    }

    #[test]
    fn password_rejects_all_letters_or_all_digits() {
        assert!(validate_password("abcdefghijkl").is_err());
        assert!(validate_password("123456789012").is_err());
    }

    #[test]
    fn password_happy_path() {
        assert!(validate_password("hunter22hunter").is_ok()); // 14 chars
        assert!(validate_password("correct horse 99").is_ok()); // 17 chars incl. space
    }

    #[test]
    fn password_enforces_max() {
        let too_long = "a".repeat(MAX_PASSWORD_LEN + 1);
        assert!(validate_password(&too_long).is_err());
    }
}
