//! Optional API credentials (GitHub token, CurseForge key), stored in the OS
//! credential store: Windows Credential Manager or the macOS Keychain.
//! They are never written to the state file, logs or the repo.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const SERVICE: &str = "com.noolies.wow-mods";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum SecretKind {
    GithubToken,
    CurseforgeKey,
}

impl SecretKind {
    fn account(self) -> &'static str {
        match self {
            SecretKind::GithubToken => "github-token",
            SecretKind::CurseforgeKey => "curseforge-api-key",
        }
    }
}

pub fn read(kind: SecretKind) -> Option<String> {
    let entry = keyring::Entry::new(SERVICE, kind.account()).ok()?;
    match entry.get_password() {
        Ok(value) if !value.trim().is_empty() => Some(value),
        Ok(_) => None,
        Err(keyring::Error::NoEntry) => None,
        Err(error) => {
            log::warn!("Couldn't read {:?} from the credential store: {error}", kind);
            None
        }
    }
}

/// Saves the secret, or removes it when `value` is empty.
pub fn write(kind: SecretKind, value: Option<&str>) -> Result<()> {
    let entry = keyring::Entry::new(SERVICE, kind.account())
        .context("The system credential store isn't available")?;
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some(value) => entry
            .set_password(value)
            .context("Couldn't save to the system credential store"),
        None => match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error).context("Couldn't remove the saved credential"),
        },
    }
}
