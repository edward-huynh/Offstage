//! API keys. A provider config only holds a reference (`keychain:<name>` or `env:<VAR>`);
//! the key itself lives in the OS keychain or in an environment variable and is read only
//! when a request is made. Nothing here ever puts a key in an error message or a log.

use crate::LlmError;

/// Service name the keychain entries are filed under.
#[cfg(any(target_os = "macos", windows))]
const SERVICE: &str = "dev.offstage.app";

/// Where a key is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyRef {
    Keychain(String),
    Env(String),
}

impl KeyRef {
    /// Parses `keychain:<name>` or `env:<VAR>`.
    pub fn parse(key_ref: &str) -> Result<Self, LlmError> {
        let bad = || LlmError::Keychain(format!("unknown key reference {key_ref:?}"));
        let (scheme, name) = key_ref.split_once(':').ok_or_else(bad)?;
        let name = name.trim();
        if name.is_empty() {
            return Err(bad());
        }
        match scheme {
            "keychain" => Ok(Self::Keychain(name.to_owned())),
            "env" => Ok(Self::Env(name.to_owned())),
            _ => Err(bad()),
        }
    }
}

/// The key a reference points to, or `None` when nothing is stored there yet.
pub fn resolve(key_ref: &str) -> Result<Option<String>, LlmError> {
    match KeyRef::parse(key_ref)? {
        KeyRef::Env(var) => Ok(std::env::var(&var).ok().filter(|v| !v.trim().is_empty())),
        KeyRef::Keychain(name) => keychain::get(&name),
    }
}

/// Saves a key under `keychain:<name>`. Environment references are read-only.
pub fn store(key_ref: &str, key: &str) -> Result<(), LlmError> {
    match KeyRef::parse(key_ref)? {
        KeyRef::Env(_) => Err(LlmError::Keychain(
            "a key set by an environment variable cannot be changed from the app".into(),
        )),
        KeyRef::Keychain(name) => keychain::set(&name, key.trim()),
    }
}

/// True when a key is available for the reference.
pub fn exists(key_ref: &str) -> Result<bool, LlmError> {
    resolve(key_ref).map(|key| key.is_some())
}

/// Removes a key saved under `keychain:<name>`. A key that is not there is not an error.
pub fn remove(key_ref: &str) -> Result<(), LlmError> {
    match KeyRef::parse(key_ref)? {
        KeyRef::Env(_) => Ok(()),
        KeyRef::Keychain(name) => keychain::delete(&name),
    }
}

#[cfg(any(target_os = "macos", windows))]
mod keychain {
    use keyring::{Entry, Error};

    use super::{LlmError, SERVICE};

    fn entry(name: &str) -> Result<Entry, LlmError> {
        Entry::new(SERVICE, name).map_err(|e| LlmError::Keychain(e.to_string()))
    }

    pub fn get(name: &str) -> Result<Option<String>, LlmError> {
        match entry(name)?.get_password() {
            Ok(key) if key.trim().is_empty() => Ok(None),
            Ok(key) => Ok(Some(key)),
            Err(Error::NoEntry) => Ok(None),
            Err(e) => Err(LlmError::Keychain(e.to_string())),
        }
    }

    pub fn set(name: &str, key: &str) -> Result<(), LlmError> {
        if key.is_empty() {
            return delete(name);
        }
        entry(name)?
            .set_password(key)
            .map_err(|e| LlmError::Keychain(e.to_string()))
    }

    pub fn delete(name: &str) -> Result<(), LlmError> {
        match entry(name)?.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(e) => Err(LlmError::Keychain(e.to_string())),
        }
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod keychain {
    use super::LlmError;

    fn unsupported() -> LlmError {
        LlmError::Keychain(
            "the system keychain is not supported on this OS yet; use env:<VAR>".into(),
        )
    }

    pub fn get(_name: &str) -> Result<Option<String>, LlmError> {
        Err(unsupported())
    }

    pub fn set(_name: &str, _key: &str) -> Result<(), LlmError> {
        Err(unsupported())
    }

    pub fn delete(_name: &str) -> Result<(), LlmError> {
        Err(unsupported())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_parse_into_where_the_key_lives() {
        assert_eq!(
            KeyRef::parse("keychain:openai").unwrap(),
            KeyRef::Keychain("openai".into())
        );
        assert_eq!(
            KeyRef::parse("env:OPENAI_API_KEY").unwrap(),
            KeyRef::Env("OPENAI_API_KEY".into())
        );
    }

    #[test]
    fn malformed_references_are_rejected_without_echoing_a_secret_shaped_value() {
        for bad in ["", "openai", "keychain:", "env:  ", "file:/etc/passwd"] {
            assert!(KeyRef::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn an_env_reference_reads_the_variable() {
        // A name no other test or machine uses, so tests can run in parallel.
        let var = "OFFSTAGE_TEST_KEY_REF_READS_VARIABLE";
        std::env::set_var(var, "  sk-test-value  ");
        let found = resolve(&format!("env:{var}")).unwrap();
        std::env::remove_var(var);
        assert_eq!(found.as_deref(), Some("  sk-test-value  "));
    }

    #[test]
    fn a_missing_or_blank_variable_means_no_key() {
        assert_eq!(
            resolve("env:OFFSTAGE_TEST_KEY_REF_NEVER_SET").unwrap(),
            None
        );
        let var = "OFFSTAGE_TEST_KEY_REF_BLANK";
        std::env::set_var(var, "   ");
        let found = resolve(&format!("env:{var}")).unwrap();
        std::env::remove_var(var);
        assert_eq!(found, None);
    }

    #[test]
    fn an_env_key_cannot_be_overwritten_from_the_app() {
        let err = store("env:ANYTHING", "sk-secret").unwrap_err().to_string();
        assert!(!err.contains("sk-secret"));
    }

    #[test]
    fn removing_an_env_reference_is_a_no_op() {
        assert!(remove("env:ANYTHING").is_ok());
    }
}
