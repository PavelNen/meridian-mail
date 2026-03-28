use keyring::Entry;

use crate::error::{AppError, AppResult};

const SERVICE: &str = "meridian-mail";

/// Save a password to the system credential store.
/// Returns the key (email) used to retrieve it later.
pub fn save_password(email: &str, password: &str) -> AppResult<String> {
    let entry = Entry::new(SERVICE, email)
        .map_err(|e| AppError::Keychain(format!("failed to create entry for {email}: {e}")))?;
    entry
        .set_password(password)
        .map_err(|e| AppError::Keychain(format!("failed to save password for {email}: {e}")))?;
    Ok(email.to_string())
}

/// Retrieve a password from the system credential store by email.
pub fn get_password(email: &str) -> AppResult<String> {
    let entry = Entry::new(SERVICE, email)
        .map_err(|e| AppError::Keychain(format!("failed to create entry for {email}: {e}")))?;
    entry
        .get_password()
        .map_err(|e| AppError::Keychain(format!("failed to get password for {email}: {e}")))
}

/// Delete a password from the system credential store.
/// Returns Ok(()) even if the entry does not exist (best-effort).
pub fn delete_password(email: &str) -> AppResult<()> {
    let entry = Entry::new(SERVICE, email)
        .map_err(|e| AppError::Keychain(format!("failed to create entry for {email}: {e}")))?;
    match entry.delete_credential() {
        Ok(_) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::Keychain(format!(
            "failed to delete password for {email}: {e}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires system credential store access"]
    fn test_save_get_delete() {
        let email = "test_meridian@example.com";
        let password = "super_secret_123";

        let key = save_password(email, password).expect("save_password failed");
        assert_eq!(key, email);

        let retrieved = get_password(email).expect("get_password failed");
        assert_eq!(retrieved, password);

        delete_password(email).expect("delete_password failed");

        let result = get_password(email);
        assert!(result.is_err(), "password should be deleted");
    }

    #[test]
    #[ignore = "requires system credential store access"]
    fn test_delete_nonexistent_is_ok() {
        let result = delete_password("nonexistent_meridian@example.com");
        assert!(result.is_ok());
    }
}
