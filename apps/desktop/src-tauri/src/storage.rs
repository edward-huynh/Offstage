//! Where the app keeps its data, and the one database opened there.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use offstage_core::db::Db;
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
