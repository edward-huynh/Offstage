//! Commands for the Knowledge documents. Files are picked and read here in Rust, the
//! webview never touches them.

use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use offstage_core::knowledge::{
    KnowledgeDoc, KnowledgeError, KnowledgeKind, KnowledgeStore, MAX_FILE_BYTES,
};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

/// The document storage. The lock keeps two commands from interleaving their
/// read-modify-write of the index. The `Arc` lets a command hand the store to the blocking
/// pool, so file reads and writes never run on the window's event loop.
pub struct Knowledge(pub Arc<Mutex<KnowledgeStore>>);

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
    store: &Mutex<KnowledgeStore>,
    op: impl FnOnce(&KnowledgeStore) -> Result<T, KnowledgeError>,
) -> Result<T, String> {
    let store = store
        .lock()
        .map_err(|_| "document storage is unavailable".to_owned())?;
    op(&store).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_knowledge(
    app: AppHandle,
    knowledge: State<'_, Knowledge>,
) -> Result<Vec<KnowledgeDoc>, String> {
    let workspace = crate::workspaces::active(&app)?.id;
    let store = knowledge.0.clone();
    crate::off_main(move || with_store(&store, |s| s.list_in(&workspace))).await
}

#[tauri::command]
pub async fn delete_knowledge(knowledge: State<'_, Knowledge>, id: String) -> Result<(), String> {
    let store = knowledge.0.clone();
    crate::off_main(move || with_store(&store, |s| s.delete(&id))).await
}

/// Stores text typed or pasted in the app, as a `.md` or `.txt` document.
#[tauri::command]
pub async fn create_knowledge(
    app: AppHandle,
    knowledge: State<'_, Knowledge>,
    title: String,
    kind: KnowledgeKind,
    text: String,
) -> Result<KnowledgeDoc, String> {
    let workspace = crate::workspaces::active(&app)?.id;
    let store = knowledge.0.clone();
    let doc = crate::off_main(move || {
        with_store(&store, |s| s.create_in(&workspace, &title, kind, &text))
    })
    .await?;
    crate::rag::index_in_background(&app, vec![doc.id.clone()]);
    Ok(doc)
}

/// Lets the user pick any number of documents and copies them into the store.
/// Cancelling the dialog gives an empty report.
/// Async so the blocking dialog does not run on the main thread, which would deadlock.
#[tauri::command]
pub async fn import_knowledge(
    app: AppHandle,
    knowledge: State<'_, Knowledge>,
) -> Result<ImportReport, String> {
    let picked = app
        .dialog()
        .file()
        .add_filter("Tài liệu", &KnowledgeKind::EXTENSIONS)
        .blocking_pick_files();
    let Some(picked) = picked else {
        return Ok(ImportReport::default());
    };
    let mut paths = Vec::new();
    let mut report = ImportReport::default();
    for file in picked {
        match file.into_path() {
            Ok(path) => paths.push(path),
            Err(e) => report.skipped.push(SkippedFile {
                name: "(không rõ tên)".to_owned(),
                reason: format!("Không mở được tệp: {e}"),
            }),
        }
    }
    let rest = import_into_workspace(&app, &knowledge, paths).await?;
    report.imported.extend(rest.imported);
    report.skipped.extend(rest.skipped);
    Ok(report)
}

/// Copies files dropped on the window into the store. The paths come from the operating system's
/// drag and drop, which is the user's own action; each one is checked like a picked file (type,
/// size) before it is read.
#[tauri::command]
pub async fn import_knowledge_paths(
    app: AppHandle,
    knowledge: State<'_, Knowledge>,
    paths: Vec<String>,
) -> Result<ImportReport, String> {
    import_into_workspace(
        &app,
        &knowledge,
        paths.into_iter().map(PathBuf::from).collect(),
    )
    .await
}

/// Imports the files into the workspace in use, and starts reading them for the AI. The reading
/// happens off the window's event loop, and the screen is not kept waiting for the indexing.
async fn import_into_workspace(
    app: &AppHandle,
    knowledge: &State<'_, Knowledge>,
    paths: Vec<PathBuf>,
) -> Result<ImportReport, String> {
    let workspace = crate::workspaces::active(app)?.id;
    let store = knowledge.0.clone();
    let report = crate::off_main(move || Ok(import_paths(&store, &workspace, &paths))).await?;
    crate::rag::index_in_background(app, report.imported.iter().map(|d| d.id.clone()).collect());
    Ok(report)
}

/// Imports each file, one bad file not spoiling the rest. A name that appears twice is imported
/// twice: they are two documents.
fn import_paths(store: &Mutex<KnowledgeStore>, workspace: &str, paths: &[PathBuf]) -> ImportReport {
    let mut report = ImportReport::default();
    for path in paths {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let result = read_document(path, &name)
            .and_then(|bytes| with_store(store, |s| s.import_into(workspace, &name, &bytes)));
        match result {
            Ok(doc) => report.imported.push(doc),
            Err(reason) => report.skipped.push(SkippedFile { name, reason }),
        }
    }
    report
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

    fn store_in(dir: &Scratch) -> Mutex<KnowledgeStore> {
        Mutex::new(KnowledgeStore::new(
            offstage_core::db::Db::open_in_memory().unwrap(),
            dir.0.join("store"),
        ))
    }

    #[test]
    fn dropped_files_are_imported_into_the_workspace_and_a_bad_one_does_not_spoil_the_rest() {
        let dir = Scratch::new("dropped");
        let good = dir.0.join("cv.md");
        fs::write(&good, "# CV").unwrap();
        let wrong = dir.0.join("virus.exe");
        fs::write(&wrong, "x").unwrap();
        let gone = dir.0.join("gone.txt");
        let also_good = dir.0.join("jd.txt");
        fs::write(&also_good, "Cần Rust").unwrap();

        let store = store_in(&dir);
        let report = import_paths(&store, "w1", &[good, wrong, gone, also_good]);

        let names: Vec<_> = report.imported.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["cv.md", "jd.txt"]);
        let skipped: Vec<_> = report.skipped.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(skipped, ["virus.exe", "gone.txt"]);
        // They went into the workspace asked for, and into no other.
        let guard = store.lock().unwrap();
        assert_eq!(guard.list_in("w1").unwrap().len(), 2);
        assert!(guard.list_in("default").unwrap().is_empty());
    }

    #[test]
    fn nothing_dropped_is_an_empty_report() {
        let dir = Scratch::new("none");
        let report = import_paths(&store_in(&dir), "default", &[]);
        assert!(report.imported.is_empty() && report.skipped.is_empty());
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
