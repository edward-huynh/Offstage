//! Where the app keeps its data, and the one database opened there.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use offstage_core::db::{Db, DbError};
use serde::{de::DeserializeOwned, Serialize};
use tauri::{AppHandle, Manager};

/// Environment variable that moves the database and the stored documents to another folder,
/// so a development run can have its own data instead of sharing the installed app's.
pub const DATA_DIR_VAR: &str = "OFFSTAGE_DATA_DIR";

/// The override when it is set to something, otherwise the system's app data folder.
fn resolve_data_dir(over: Option<OsString>, default: &Path) -> PathBuf {
    match over {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => default.to_path_buf(),
    }
}

/// The folder for the database and the documents. The same one in development and in release
/// builds unless `OFFSTAGE_DATA_DIR` says otherwise.
pub fn data_dir(app: &AppHandle) -> tauri::Result<PathBuf> {
    let default = app.path().app_data_dir()?;
    Ok(resolve_data_dir(std::env::var_os(DATA_DIR_VAR), &default))
}

/// Opens (creating it on first use) the database inside the data folder.
pub fn open_db(data_dir: &Path) -> Result<Db, String> {
    Db::open(data_dir).map_err(|e| format!("cannot open the local database: {e}"))
}

/// Key that remembers the settings file of the old store plugin was copied into the database.
const LEGACY_SETTINGS_KEY: &str = "legacy_settings_imported";

/// Copies the values the old `settings.json` held (overlay config, window position, app
/// settings...) into the database, once. Returns how many were copied.
pub fn import_legacy_settings(db: &Db, app: &AppHandle) -> Result<usize, String> {
    let file = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("settings.json");
    db.import_json_object(&file, LEGACY_SETTINGS_KEY)
        .map_err(|e| e.to_string())
}

fn open_handle(app: &AppHandle) -> Result<Db, String> {
    app.try_state::<Db>()
        .map(|db| db.inner().clone())
        .ok_or_else(|| "the local database is not open".to_owned())
}

/// A stored value, or `None` when there is none or it is not of the expected shape (a value
/// from an older version, say), so callers fall back to their defaults.
pub fn get<T: DeserializeOwned>(app: &AppHandle, key: &str) -> Result<Option<T>, String> {
    match open_handle(app)?.kv_get(key) {
        Ok(value) => Ok(value),
        Err(DbError::Json(_)) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

pub fn set<T: Serialize>(app: &AppHandle, key: &str, value: &T) -> Result<(), String> {
    open_handle(app)?
        .kv_set(key, value)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_folder_is_used_unless_the_variable_is_set() {
        let default = Path::new("/data/dev.offstage.app");
        assert_eq!(resolve_data_dir(None, default), default);
        assert_eq!(resolve_data_dir(Some(OsString::new()), default), default);
        assert_eq!(
            resolve_data_dir(Some("/tmp/offstage-dev".into()), default),
            Path::new("/tmp/offstage-dev")
        );
    }
}
