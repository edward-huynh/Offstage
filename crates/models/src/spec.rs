use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Everything needed to fetch and verify one model file. Pure data: new models are new values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSpec {
    pub id: String,
    /// File name inside the models dir. A plain name, no directories.
    pub file_name: String,
    pub url: String,
    /// Lowercase hex SHA-256 of the final file.
    pub sha256: String,
    /// Size of the final file in bytes; used as the progress total.
    pub size: u64,
    /// License name as published by the model's source, shown to the user before download.
    pub license: String,
    pub license_url: Option<String>,
}

impl ModelSpec {
    /// Where the finished model lives inside `dir`.
    pub fn path_in(&self, dir: &Path) -> PathBuf {
        dir.join(&self.file_name)
    }

    /// In-progress download; resumed by the next `ensure` call.
    pub fn part_path_in(&self, dir: &Path) -> PathBuf {
        dir.join(format!("{}.part", self.file_name))
    }
}
