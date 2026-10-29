//! Commands for the Knowledge documents. Files are picked and read here in Rust, the
//! webview never touches them.

use std::{
    fs::{self, File},
    io::Read,
    path::Path,
    sync::Mutex,
};

use offstage_core::knowledge::{
    KnowledgeDoc, KnowledgeError, KnowledgeKind, KnowledgeStore, MAX_FILE_BYTES,
};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

/// The document storage. The lock keeps two commands from interleaving their
/// read-modify-write of the index.
pub struct Knowledge(pub Mutex<KnowledgeStore>);

/// A file that was not imported, and why, in words the user can read.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SkippedFile {
    name: String,
    reason: String,
}

/// Outcome of importing a batch: one bad file does not spoil the rest.
#[derive(Debug, Default, Serialize)]
pub struct ImportReport {
    imported: Vec<KnowledgeDoc>,
    skipped: Vec<SkippedFile>,
}

fn with_store<T>(
    knowledge: &Knowledge,
    op: impl FnOnce(&KnowledgeStore) -> Result<T, KnowledgeError>,
) -> Result<T, String> {
    let store = knowledge
        .0
        .lock()
        .map_err(|_| "document storage is unavailable".to_owned())?;
    op(&store).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_knowledge(knowledge: State<Knowledge>) -> Result<Vec<KnowledgeDoc>, String> {
    with_store(&knowledge, |s| s.list())
}

#[tauri::command]
pub fn delete_knowledge(knowledge: State<Knowledge>, id: String) -> Result<(), String> {
    with_store(&knowledge, |s| s.delete(&id))
}

/// Lets the user pick any number of documents and copies them into the store.
/// Cancelling the dialog gives an empty report.
/// Async so the blocking dialog does not run on the main thread, which would deadlock.
#[tauri::command]
pub async fn import_knowledge(
    app: AppHandle,
    knowledge: State<'_, Knowledge>,
) -> Result<ImportReport, String> {
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter("Tài liệu", &KnowledgeKind::EXTENSIONS)
        .blocking_pick_files()
    else {
        return Ok(ImportReport::default());
    };

    let mut report = ImportReport::default();
    for file in picked {
        let path = match file.into_path() {
            Ok(path) => path,
            Err(e) => {
                report.skipped.push(SkippedFile {
                    name: "(không rõ tên)".to_owned(),
                    reason: format!("Không mở được tệp: {e}"),
                });
                continue;
            }
        };
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let result = read_document(&path, &name)
            .and_then(|bytes| with_store(&knowledge, |s| s.import(&name, &bytes)));
        match result {
            Ok(doc) => report.imported.push(doc),
            Err(reason) => report.skipped.push(SkippedFile { name, reason }),
        }
    }
    Ok(report)
}

/// Reads a picked file after checking, from its name and its metadata, that the store
/// would accept it, so a huge or wrong file is never loaded into memory.
/// The filter in the dialog is only a hint on some systems, hence the extension check.
fn read_document(path: &Path, name: &str) -> Result<Vec<u8>, String> {
    if KnowledgeKind::from_file_name(name).is_none() {
        return Err(KnowledgeError::UnsupportedType.to_string());
    }
    let metadata = fs::metadata(path).map_err(|e| format!("Không đọc được tệp: {e}"))?;
    if !metadata.is_file() {
        return Err("Không phải tệp".to_owned());
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(KnowledgeError::TooLarge.to_string());
    }
    // The file may have grown since the metadata was read, so cap the read as well.
    // The store rejects anything over the limit.
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    File::open(path)
        .and_then(|f| f.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|e| format!("Không đọc được tệp: {e}"))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// A scratch folder removed on drop. The desktop crate has no `tempfile` dependency,
    /// and a new one would only add churn to the manifest for four small tests.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(test: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("offstage-knowledge-{}-{test}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_wrong_extension_is_skipped_without_reading() {
        // The path does not exist: reaching the metadata would give a different error.
        let err = read_document(Path::new("/nowhere/virus.exe"), "virus.exe").unwrap_err();
        assert_eq!(err, "Chỉ nhận .pdf, .docx, .md, .txt");
    }

    #[test]
    fn a_missing_file_and_a_folder_are_skipped_with_a_reason() {
        let dir = Scratch::new("missing");
        let missing = dir.0.join("gone.txt");
        assert!(read_document(&missing, "gone.txt").is_err());
        let folder = dir.0.join("folder.txt");
        fs::create_dir(&folder).unwrap();
        assert_eq!(
            read_document(&folder, "folder.txt").unwrap_err(),
            "Không phải tệp"
        );
    }

    #[test]
    fn a_file_over_the_limit_is_skipped_by_its_size() {
        let dir = Scratch::new("big");
        let big = dir.0.join("big.pdf");
        let file = File::create(&big).unwrap();
        file.set_len(MAX_FILE_BYTES + 1).unwrap();
        assert_eq!(
            read_document(&big, "big.pdf").unwrap_err(),
            "Tệp quá lớn (tối đa 20 MB)"
        );
    }

    #[test]
    fn a_normal_file_is_read_whole() {
        let dir = Scratch::new("normal");
        let path = dir.0.join("cv.md");
        fs::write(&path, "# CV").unwrap();
        assert_eq!(read_document(&path, "cv.md").unwrap(), b"# CV");
    }
}
