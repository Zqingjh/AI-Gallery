use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(windows)]
use std::{ffi::c_void, os::windows::io::AsRawHandle};

use serde::{Deserialize, Serialize};

use crate::{
    domain::{BackupInfo, BackupKind, DATABASE_SCHEMA_VERSION, WORKSPACE_FORMAT_VERSION},
    repositories::DatabaseRepository,
    services::{DatabaseService, WorkspaceService, WorkspaceServiceError},
};

const DATABASE_RELATIVE_PATH: &str = "data/library.sqlite3";
const WORKSPACE_MANIFEST_RELATIVE_PATH: &str = "data/workspace.json";
const SNAPSHOT_MANIFEST_RELATIVE_PATH: &str = "backup.json";
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const FULL_DIRECTORY_RELATIVE_PATHS: [&str; 4] = ["media", "cache", "exports", "data/backups"];
static STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetFinalPathNameByHandleW(
        file: *mut c_void,
        path: *mut u16,
        path_capacity: u32,
        flags: u32,
    ) -> u32;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackupServiceError {
    Workspace(WorkspaceServiceError),
    InvalidPath,
    DestinationExists,
    SnapshotMissing,
    SnapshotInvalid,
    SnapshotVersionUnsupported,
    SnapshotUnsafe,
    DatabaseInvalid,
    CreateFailed,
    CopyFailed,
    CommitFailed,
    CleanupFailed,
}

impl From<WorkspaceServiceError> for BackupServiceError {
    fn from(error: WorkspaceServiceError) -> Self {
        Self::Workspace(error)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SnapshotManifest {
    format_version: u32,
    kind: SnapshotKind,
    schema_version: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum SnapshotKind {
    Full,
    Lightweight,
}

impl From<BackupKind> for SnapshotKind {
    fn from(kind: BackupKind) -> Self {
        match kind {
            BackupKind::Full => Self::Full,
            BackupKind::Lightweight => Self::Lightweight,
        }
    }
}

impl From<SnapshotKind> for BackupKind {
    fn from(kind: SnapshotKind) -> Self {
        match kind {
            SnapshotKind::Full => Self::Full,
            SnapshotKind::Lightweight => Self::Lightweight,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkspaceManifest {
    format_version: u32,
}

pub(crate) struct BackupService;

impl BackupService {
    pub(crate) fn new() -> Self {
        Self
    }

    /// 创建不可覆盖的目录快照；密钥位于系统安全存储，不参与任何文件复制。
    pub(crate) fn create_backup(
        &self,
        workspace_root: &Path,
        destination: &Path,
        kind: BackupKind,
    ) -> Result<BackupInfo, BackupServiceError> {
        WorkspaceService::new().open_workspace(workspace_root)?;
        validate_new_destination(destination)?;
        reject_destination_inside_source(workspace_root, destination)?;
        let trusted_source_root = workspace_root
            .canonicalize()
            .map_err(|_| BackupServiceError::SnapshotUnsafe)?;
        let source_database = workspace_root.join(DATABASE_RELATIVE_PATH);
        validate_regular_file(&source_database, BackupServiceError::DatabaseInvalid)?;
        let source_connection = DatabaseRepository::open(&source_database)
            .map_err(|_| BackupServiceError::DatabaseInvalid)?;
        validate_schema(&source_connection, DATABASE_SCHEMA_VERSION)?;
        let (managed_asset_count, external_asset_count) =
            asset_path_kind_counts(&source_connection)?;

        let staging = create_staging_directory(
            destination
                .parent()
                .ok_or(BackupServiceError::InvalidPath)?,
        )?;
        let result = (|| {
            fs::create_dir(staging.join("data")).map_err(|_| BackupServiceError::CreateFailed)?;
            copy_file_safe(
                &workspace_root.join(WORKSPACE_MANIFEST_RELATIVE_PATH),
                &staging.join(WORKSPACE_MANIFEST_RELATIVE_PATH),
                &trusted_source_root,
            )?;
            create_database_snapshot(&source_connection, &staging.join(DATABASE_RELATIVE_PATH))?;
            if kind == BackupKind::Full {
                for relative in FULL_DIRECTORY_RELATIVE_PATHS {
                    copy_tree_safe(
                        &workspace_root.join(relative),
                        &staging.join(relative),
                        &trusted_source_root,
                    )?;
                }
            }
            write_snapshot_manifest(&staging, kind)?;
            validate_snapshot(&staging)
        })();

        if let Err(error) = result {
            cleanup_staging(&staging)?;
            return Err(error);
        }
        fs::rename(&staging, destination).map_err(|_| {
            let _ = fs::remove_dir_all(&staging);
            BackupServiceError::CommitFailed
        })?;

        Ok(BackupInfo {
            kind,
            schema_version: DATABASE_SCHEMA_VERSION,
            managed_asset_count,
            external_asset_count,
        })
    }

    /// 仅允许恢复到不存在的新目标，以 staging 校验完成后再同卷提交。
    pub(crate) fn restore_backup(
        &self,
        snapshot_root: &Path,
        destination: &Path,
    ) -> Result<BackupInfo, BackupServiceError> {
        let info = validate_snapshot(snapshot_root)?;
        validate_new_destination(destination)?;
        reject_destination_inside_source(snapshot_root, destination)?;
        let trusted_snapshot_root = snapshot_root
            .canonicalize()
            .map_err(|_| BackupServiceError::SnapshotUnsafe)?;
        let parent = destination
            .parent()
            .ok_or(BackupServiceError::InvalidPath)?;
        let staging_container = create_staging_directory(parent)?;
        let staging = staging_container.join("workspace");
        let result = (|| {
            WorkspaceService::new()
                .create_workspace(&staging)
                .map_err(BackupServiceError::Workspace)?;
            copy_file_safe(
                &snapshot_root.join(DATABASE_RELATIVE_PATH),
                &staging.join(DATABASE_RELATIVE_PATH),
                &trusted_snapshot_root,
            )?;
            if info.kind == BackupKind::Full {
                for relative in FULL_DIRECTORY_RELATIVE_PATHS {
                    copy_tree_contents_safe(
                        &snapshot_root.join(relative),
                        &staging.join(relative),
                        &trusted_snapshot_root,
                    )?;
                }
            }
            WorkspaceService::new().open_workspace(&staging)?;
            let connection = DatabaseRepository::open(&staging.join(DATABASE_RELATIVE_PATH))
                .map_err(|_| BackupServiceError::DatabaseInvalid)?;
            validate_schema(&connection, info.schema_version)?;
            drop(connection);
            if info.schema_version < DATABASE_SCHEMA_VERSION {
                // 只迁移 staging 副本；源快照保持不变，失败时整个 staging 会被清理。
                DatabaseService::new()
                    .prepare_workspace_database(&staging)
                    .map_err(|_| BackupServiceError::DatabaseInvalid)?;
            }
            let connection = DatabaseRepository::open(&staging.join(DATABASE_RELATIVE_PATH))
                .map_err(|_| BackupServiceError::DatabaseInvalid)?;
            validate_schema(&connection, DATABASE_SCHEMA_VERSION)?;
            Ok(())
        })();
        if let Err(error) = result {
            cleanup_staging(&staging_container)?;
            return Err(error);
        }
        fs::rename(&staging, destination).map_err(|_| {
            let _ = fs::remove_dir_all(&staging_container);
            BackupServiceError::CommitFailed
        })?;
        fs::remove_dir(&staging_container).map_err(|_| BackupServiceError::CleanupFailed)?;
        Ok(BackupInfo {
            schema_version: DATABASE_SCHEMA_VERSION,
            ..info
        })
    }
}

fn validate_snapshot(root: &Path) -> Result<BackupInfo, BackupServiceError> {
    validate_existing_directory(root, BackupServiceError::SnapshotMissing)?;
    let manifest: SnapshotManifest = read_json_file(&root.join(SNAPSHOT_MANIFEST_RELATIVE_PATH))?;
    if manifest.format_version != 1 {
        return Err(BackupServiceError::SnapshotVersionUnsupported);
    }
    if manifest.schema_version == 0 || manifest.schema_version > DATABASE_SCHEMA_VERSION {
        return Err(BackupServiceError::SnapshotVersionUnsupported);
    }
    let workspace_manifest: WorkspaceManifest =
        read_json_file(&root.join(WORKSPACE_MANIFEST_RELATIVE_PATH))?;
    if workspace_manifest.format_version != WORKSPACE_FORMAT_VERSION {
        return Err(BackupServiceError::SnapshotVersionUnsupported);
    }

    let database_path = root.join(DATABASE_RELATIVE_PATH);
    validate_regular_file(&database_path, BackupServiceError::SnapshotUnsafe)?;
    let connection = DatabaseRepository::open(&database_path)
        .map_err(|_| BackupServiceError::DatabaseInvalid)?;
    validate_schema(&connection, manifest.schema_version)?;

    match manifest.kind {
        SnapshotKind::Full => {
            validate_directory_entries(
                root,
                &["backup.json", "data", "media", "cache", "exports"],
            )?;
            validate_directory_entries(
                &root.join("data"),
                &["workspace.json", "library.sqlite3", "backups"],
            )?;
            for relative in FULL_DIRECTORY_RELATIVE_PATHS {
                validate_tree_safe(&root.join(relative))?;
            }
        }
        SnapshotKind::Lightweight => {
            validate_directory_entries(root, &["backup.json", "data"])?;
            validate_directory_entries(&root.join("data"), &["workspace.json", "library.sqlite3"])?;
            for relative in FULL_DIRECTORY_RELATIVE_PATHS {
                if root
                    .join(relative)
                    .try_exists()
                    .map_err(|_| BackupServiceError::SnapshotUnsafe)?
                {
                    return Err(BackupServiceError::SnapshotInvalid);
                }
            }
        }
    }
    Ok(BackupInfo {
        kind: manifest.kind.into(),
        schema_version: manifest.schema_version,
        managed_asset_count: 0,
        external_asset_count: 0,
    })
}

fn asset_path_kind_counts(
    connection: &rusqlite::Connection,
) -> Result<(u64, u64), BackupServiceError> {
    let mut statement = connection
        .prepare("SELECT path_kind, COUNT(*) FROM assets GROUP BY path_kind")
        .map_err(|_| BackupServiceError::DatabaseInvalid)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|_| BackupServiceError::DatabaseInvalid)?;
    let mut managed = 0_u64;
    let mut external = 0_u64;
    for row in rows {
        match row.map_err(|_| BackupServiceError::DatabaseInvalid)? {
            (kind, count) if kind == "managed" => {
                managed = u64::try_from(count).map_err(|_| BackupServiceError::DatabaseInvalid)?
            }
            (kind, count) if kind == "external" => {
                external = u64::try_from(count).map_err(|_| BackupServiceError::DatabaseInvalid)?
            }
            _ => return Err(BackupServiceError::DatabaseInvalid),
        }
    }
    Ok((managed, external))
}

fn validate_schema(
    connection: &rusqlite::Connection,
    expected_version: u32,
) -> Result<(), BackupServiceError> {
    let version = DatabaseRepository::schema_version(connection)
        .map_err(|_| BackupServiceError::DatabaseInvalid)?;
    if version != expected_version
        || expected_version == 0
        || expected_version > DATABASE_SCHEMA_VERSION
    {
        return Err(BackupServiceError::SnapshotVersionUnsupported);
    }
    let result = match expected_version {
        1 => DatabaseRepository::validate_v1(connection),
        2 => DatabaseRepository::validate_v2(connection),
        3 => DatabaseRepository::validate_v3(connection),
        4 => DatabaseRepository::validate_v4(connection),
        5 => DatabaseRepository::validate_v5(connection),
        6 => DatabaseRepository::validate_v6(connection),
        7 => DatabaseRepository::validate_v7(connection),
        8 => DatabaseRepository::validate_v8(connection),
        9 => DatabaseRepository::validate_v9(connection),
        10 => DatabaseRepository::validate_v10(connection),
        _ => return Err(BackupServiceError::SnapshotVersionUnsupported),
    };
    result.map_err(|_| BackupServiceError::DatabaseInvalid)
}

fn create_database_snapshot(
    source: &rusqlite::Connection,
    destination: &Path,
) -> Result<(), BackupServiceError> {
    create_new_file(destination)?;
    let mut output = DatabaseRepository::open_backup_destination(destination)
        .map_err(|_| BackupServiceError::CreateFailed)?;
    DatabaseRepository::backup(source, &mut output).map_err(|_| BackupServiceError::CopyFailed)?;
    drop(output);
    validate_regular_file(destination, BackupServiceError::CopyFailed)
}

fn write_snapshot_manifest(root: &Path, kind: BackupKind) -> Result<(), BackupServiceError> {
    let manifest = SnapshotManifest {
        format_version: 1,
        kind: kind.into(),
        schema_version: DATABASE_SCHEMA_VERSION,
    };
    let bytes =
        serde_json::to_vec_pretty(&manifest).map_err(|_| BackupServiceError::CreateFailed)?;
    let path = root.join(SNAPSHOT_MANIFEST_RELATIVE_PATH);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| BackupServiceError::CreateFailed)?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| BackupServiceError::CreateFailed)
}

fn read_json_file<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, BackupServiceError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| BackupServiceError::SnapshotMissing)?;
    reject_link_or_reparse(&metadata, BackupServiceError::SnapshotUnsafe)?;
    if !metadata.is_file() || metadata.len() > MAX_MANIFEST_BYTES {
        return Err(BackupServiceError::SnapshotInvalid);
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)
        .map_err(|_| BackupServiceError::SnapshotMissing)?
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| BackupServiceError::SnapshotInvalid)?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(BackupServiceError::SnapshotInvalid);
    }
    serde_json::from_slice(&bytes).map_err(|_| BackupServiceError::SnapshotInvalid)
}

fn validate_new_destination(destination: &Path) -> Result<(), BackupServiceError> {
    if destination.as_os_str().is_empty() || !destination.is_absolute() {
        return Err(BackupServiceError::InvalidPath);
    }
    if destination
        .try_exists()
        .map_err(|_| BackupServiceError::InvalidPath)?
    {
        return Err(BackupServiceError::DestinationExists);
    }
    let parent = destination
        .parent()
        .ok_or(BackupServiceError::InvalidPath)?;
    validate_existing_directory(parent, BackupServiceError::InvalidPath)?;
    if destination.file_name().is_none() {
        return Err(BackupServiceError::InvalidPath);
    }
    Ok(())
}

fn reject_destination_inside_source(
    source_root: &Path,
    destination: &Path,
) -> Result<(), BackupServiceError> {
    let source = fs::canonicalize(source_root).map_err(|_| BackupServiceError::InvalidPath)?;
    let parent = destination
        .parent()
        .ok_or(BackupServiceError::InvalidPath)?;
    let candidate = fs::canonicalize(parent)
        .map_err(|_| BackupServiceError::InvalidPath)?
        .join(
            destination
                .file_name()
                .ok_or(BackupServiceError::InvalidPath)?,
        );
    if candidate.starts_with(source) {
        return Err(BackupServiceError::InvalidPath);
    }
    Ok(())
}

fn create_staging_directory(parent: &Path) -> Result<PathBuf, BackupServiceError> {
    for _ in 0..100 {
        let sequence = STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".ai-gallery-staging-{}-{sequence}",
            std::process::id()
        ));
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(BackupServiceError::CreateFailed),
        }
    }
    Err(BackupServiceError::CreateFailed)
}

fn cleanup_staging(path: &Path) -> Result<(), BackupServiceError> {
    fs::remove_dir_all(path).map_err(|_| BackupServiceError::CleanupFailed)
}

fn create_new_file(path: &Path) -> Result<(), BackupServiceError> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(|_| BackupServiceError::CreateFailed)
}

fn copy_file_safe(
    source: &Path,
    destination: &Path,
    trusted_root: &Path,
) -> Result<(), BackupServiceError> {
    validate_regular_file(source, BackupServiceError::SnapshotUnsafe)?;
    let canonical_source = source
        .canonicalize()
        .map_err(|_| BackupServiceError::SnapshotUnsafe)?;
    if !canonical_source.starts_with(trusted_root) {
        return Err(BackupServiceError::SnapshotUnsafe);
    }
    let mut input = File::open(source).map_err(|_| BackupServiceError::CopyFailed)?;
    // 只信任已打开句柄的最终路径，防止路径在预检和打开之间被 junction/link 替换。
    ensure_open_file_inside_root(&input, trusted_root)?;
    create_new_file(destination)?;
    let mut output = OpenOptions::new()
        .write(true)
        .open(destination)
        .map_err(|_| BackupServiceError::CopyFailed)?;
    std::io::copy(&mut input, &mut output).map_err(|_| BackupServiceError::CopyFailed)?;
    output
        .sync_all()
        .map_err(|_| BackupServiceError::CopyFailed)?;
    validate_regular_file(destination, BackupServiceError::CopyFailed)
}

#[cfg(windows)]
fn ensure_open_file_inside_root(
    file: &File,
    trusted_root: &Path,
) -> Result<(), BackupServiceError> {
    let mut buffer = vec![0_u16; 32_768];
    let length = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            0,
        )
    };
    if length == 0 || length as usize >= buffer.len() {
        return Err(BackupServiceError::SnapshotUnsafe);
    }
    let opened = String::from_utf16(&buffer[..length as usize])
        .map_err(|_| BackupServiceError::SnapshotUnsafe)?;
    let opened = normalize_windows_final_path(&opened);
    let root = normalize_windows_final_path(&trusted_root.to_string_lossy());
    let boundary = format!("{root}\\");
    if opened == root || opened.starts_with(&boundary) {
        Ok(())
    } else {
        Err(BackupServiceError::SnapshotUnsafe)
    }
}

#[cfg(windows)]
fn normalize_windows_final_path(value: &str) -> String {
    let normalized = value
        .strip_prefix(r"\\?\UNC\")
        .map(|path| format!(r"\\{path}"))
        .or_else(|| value.strip_prefix(r"\\?\").map(str::to_owned))
        .unwrap_or_else(|| value.to_owned());
    normalized.trim_end_matches(['\\', '/']).to_lowercase()
}

#[cfg(not(windows))]
fn ensure_open_file_inside_root(
    _file: &File,
    trusted_root: &Path,
) -> Result<(), BackupServiceError> {
    trusted_root
        .is_absolute()
        .then_some(())
        .ok_or(BackupServiceError::SnapshotUnsafe)
}

fn copy_tree_safe(
    source: &Path,
    destination: &Path,
    trusted_root: &Path,
) -> Result<(), BackupServiceError> {
    validate_existing_directory(source, BackupServiceError::SnapshotUnsafe)?;
    fs::create_dir_all(destination).map_err(|_| BackupServiceError::CreateFailed)?;
    copy_tree_contents_safe(source, destination, trusted_root)
}

fn copy_tree_contents_safe(
    source: &Path,
    destination: &Path,
    trusted_root: &Path,
) -> Result<(), BackupServiceError> {
    validate_existing_directory(source, BackupServiceError::SnapshotUnsafe)?;
    validate_existing_directory(destination, BackupServiceError::CopyFailed)?;
    for entry in fs::read_dir(source).map_err(|_| BackupServiceError::CopyFailed)? {
        let entry = entry.map_err(|_| BackupServiceError::CopyFailed)?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let metadata =
            fs::symlink_metadata(&source_path).map_err(|_| BackupServiceError::SnapshotUnsafe)?;
        reject_link_or_reparse(&metadata, BackupServiceError::SnapshotUnsafe)?;
        if metadata.is_dir() {
            if destination_path
                .try_exists()
                .map_err(|_| BackupServiceError::CopyFailed)?
            {
                validate_existing_directory(&destination_path, BackupServiceError::CopyFailed)?;
            } else {
                fs::create_dir(&destination_path).map_err(|_| BackupServiceError::CreateFailed)?;
            }
            copy_tree_contents_safe(&source_path, &destination_path, trusted_root)?;
        } else if metadata.is_file() {
            copy_file_safe(&source_path, &destination_path, trusted_root)?;
        } else {
            return Err(BackupServiceError::SnapshotUnsafe);
        }
    }
    Ok(())
}

fn validate_tree_safe(root: &Path) -> Result<(), BackupServiceError> {
    validate_existing_directory(root, BackupServiceError::SnapshotUnsafe)?;
    for entry in fs::read_dir(root).map_err(|_| BackupServiceError::SnapshotUnsafe)? {
        let entry = entry.map_err(|_| BackupServiceError::SnapshotUnsafe)?;
        let metadata =
            fs::symlink_metadata(entry.path()).map_err(|_| BackupServiceError::SnapshotUnsafe)?;
        reject_link_or_reparse(&metadata, BackupServiceError::SnapshotUnsafe)?;
        if metadata.is_dir() {
            validate_tree_safe(&entry.path())?;
        } else if !metadata.is_file() {
            return Err(BackupServiceError::SnapshotUnsafe);
        }
    }
    Ok(())
}

fn validate_directory_entries(root: &Path, allowed: &[&str]) -> Result<(), BackupServiceError> {
    validate_existing_directory(root, BackupServiceError::SnapshotUnsafe)?;
    for entry in fs::read_dir(root).map_err(|_| BackupServiceError::SnapshotUnsafe)? {
        let entry = entry.map_err(|_| BackupServiceError::SnapshotUnsafe)?;
        let name = entry.file_name();
        let name = name.to_str().ok_or(BackupServiceError::SnapshotUnsafe)?;
        if !allowed.contains(&name) {
            return Err(BackupServiceError::SnapshotInvalid);
        }
        let metadata =
            fs::symlink_metadata(entry.path()).map_err(|_| BackupServiceError::SnapshotUnsafe)?;
        reject_link_or_reparse(&metadata, BackupServiceError::SnapshotUnsafe)?;
    }
    Ok(())
}

fn validate_existing_directory(
    path: &Path,
    error: BackupServiceError,
) -> Result<(), BackupServiceError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| error)?;
    reject_link_or_reparse(&metadata, error)?;
    if metadata.is_dir() {
        Ok(())
    } else {
        Err(error)
    }
}

fn validate_regular_file(path: &Path, error: BackupServiceError) -> Result<(), BackupServiceError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| error)?;
    reject_link_or_reparse(&metadata, error)?;
    if metadata.is_file() {
        Ok(())
    } else {
        Err(error)
    }
}

fn reject_link_or_reparse(
    metadata: &fs::Metadata,
    error: BackupServiceError,
) -> Result<(), BackupServiceError> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x0400 != 0 {
            return Err(error);
        }
    }
    #[cfg(not(windows))]
    if metadata.file_type().is_symlink() {
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use rusqlite::Connection;

    use super::{
        BackupService, BackupServiceError, DATABASE_RELATIVE_PATH, SNAPSHOT_MANIFEST_RELATIVE_PATH,
    };
    use crate::{
        domain::BackupKind,
        repositories::DatabaseRepository,
        services::{DatabaseService, WorkspaceService},
    };

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "ai-gallery-backup-{label}-{}-{}",
                std::process::id(),
                TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir(&path).expect("应能创建隔离目录");
            Self(path)
        }
        fn child(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn workspace(parent: &TestDirectory) -> PathBuf {
        let root = parent.child("workspace");
        WorkspaceService::new()
            .create_workspace(&root)
            .expect("应能创建工作区");
        DatabaseService::new()
            .prepare_workspace_database(&root)
            .expect("应能创建数据库");
        Connection::open(root.join("data/library.sqlite3"))
            .expect("应能打开数据库")
            .execute(
                "INSERT INTO projects (title, created_at, updated_at) VALUES ('快照项目', 1, 1)",
                [],
            )
            .expect("应能写入快照数据");
        root
    }

    #[test]
    fn full_snapshot_restores_database_and_portable_workspace_content() {
        let parent = TestDirectory::new("full");
        let root = workspace(&parent);
        fs::write(root.join("media/images/cover.txt"), "媒体").expect("应能写入媒体");
        fs::write(root.join("cache/thumbnails/thumb.txt"), "缓存").expect("应能写入缓存");
        fs::write(root.join("exports/site.txt"), "导出").expect("应能写入导出");
        fs::write(root.join("data/backups/history.sqlite3"), "历史").expect("应能写入历史备份");
        let snapshot = parent.child("full-snapshot");

        let info = BackupService::new()
            .create_backup(&root, &snapshot, BackupKind::Full)
            .expect("应能创建完整快照");
        assert_eq!(info.kind, BackupKind::Full);
        assert!(snapshot.join("backup.json").is_file());
        assert!(snapshot.join("media/images/cover.txt").is_file());
        assert!(snapshot.join("data/backups/history.sqlite3").is_file());

        let restored = parent.child("restored");
        BackupService::new()
            .restore_backup(&snapshot, &restored)
            .expect("应能恢复完整快照");
        assert_eq!(
            fs::read_to_string(restored.join("media/images/cover.txt")).expect("应能读取恢复媒体"),
            "媒体"
        );
        let count: i64 = Connection::open(restored.join("data/library.sqlite3"))
            .expect("应能打开恢复数据库")
            .query_row(
                "SELECT count(*) FROM projects WHERE title = '快照项目'",
                [],
                |row| row.get(0),
            )
            .expect("应能查询恢复数据");
        assert_eq!(count, 1);
    }

    #[test]
    fn v3_snapshot_is_validated_migrated_in_staging_and_restored_as_v5() {
        let parent = TestDirectory::new("restore-v3");
        let root = workspace(&parent);
        let snapshot = parent.child("v3-snapshot");
        BackupService::new()
            .create_backup(&root, &snapshot, BackupKind::Lightweight)
            .expect("应能创建当前版本快照");
        let database_path = snapshot.join(DATABASE_RELATIVE_PATH);
        fs::remove_file(&database_path).expect("应能移除临时当前版本快照库");
        let connection = DatabaseRepository::open(&database_path).expect("应能重建历史 v3 快照库");
        for migration in [
            include_str!("../migrations/0001_initial.sql"),
            include_str!("../migrations/0002_search.sql"),
            include_str!("../migrations/0003_ai.sql"),
        ] {
            connection
                .execute_batch(migration)
                .expect("应能应用历史 migration 构造 v3 快照");
        }
        connection
            .execute(
                "INSERT INTO projects(title,description,notes,created_at,updated_at)
                 VALUES('快照项目','保留说明','保留备注',1,1)",
                [],
            )
            .expect("应能保留历史快照测试数据");
        connection
            .pragma_update(None, "user_version", 3_u32)
            .expect("应能设置 v3 快照版本");
        DatabaseRepository::validate_v3(&connection).expect("历史快照必须是合法 v3");
        drop(connection);
        let manifest_path = snapshot.join(SNAPSHOT_MANIFEST_RELATIVE_PATH);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).expect("应能读取快照清单"))
                .expect("清单应为 JSON");
        manifest["schemaVersion"] = serde_json::json!(3);
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).expect("应能编码历史清单"),
        )
        .expect("应能写入历史清单 fixture");

        let restored = parent.child("restored-v3");
        let info = BackupService::new()
            .restore_backup(&snapshot, &restored)
            .expect("v3 快照应能在 staging 中升级并恢复");
        assert_eq!(info.schema_version, 10);
        let connection = DatabaseRepository::open(&restored.join(DATABASE_RELATIVE_PATH))
            .expect("应能打开恢复数据库");
        DatabaseRepository::validate_v10(&connection).expect("恢复目标必须是合法 v10");
        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM projects WHERE title='快照项目'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("应能读取恢复数据"),
            1
        );
    }

    #[test]
    fn lightweight_snapshot_contains_only_manifest_and_consistent_database() {
        let parent = TestDirectory::new("lightweight");
        let root = workspace(&parent);
        fs::write(root.join("media/images/only-source.txt"), "不应复制").expect("应能写入媒体");
        let snapshot = parent.child("lightweight-snapshot");

        let info = BackupService::new()
            .create_backup(&root, &snapshot, BackupKind::Lightweight)
            .expect("应能创建轻量快照");
        assert_eq!(info.kind, BackupKind::Lightweight);
        assert!(snapshot.join("data/workspace.json").is_file());
        assert!(snapshot.join("data/library.sqlite3").is_file());
        assert!(!snapshot.join("media").exists());
        assert!(!snapshot.join("cache").exists());
        assert!(!snapshot.join("exports").exists());
        assert!(!snapshot.join("data/backups").exists());

        let restored = parent.child("restored-lightweight");
        BackupService::new()
            .restore_backup(&snapshot, &restored)
            .expect("应能恢复轻量快照");
        assert!(restored.join("media/images").is_dir());
        assert!(!restored.join("media/images/only-source.txt").exists());
    }

    #[test]
    fn refuses_existing_destination_and_corrupted_snapshot_before_restore() {
        let parent = TestDirectory::new("reject");
        let root = workspace(&parent);
        let snapshot = parent.child("snapshot");
        BackupService::new()
            .create_backup(&root, &snapshot, BackupKind::Lightweight)
            .expect("应能创建快照");
        let existing = parent.child("existing");
        fs::create_dir(&existing).expect("应能创建既有目标");
        fs::write(existing.join("keep.txt"), "保留").expect("应能创建保护文件");
        assert_eq!(
            BackupService::new().restore_backup(&snapshot, &existing),
            Err(BackupServiceError::DestinationExists)
        );
        assert!(existing.join("keep.txt").is_file());

        fs::write(snapshot.join("backup.json"), "{}").expect("应能破坏清单");
        let target = parent.child("must-not-exist");
        assert_eq!(
            BackupService::new().restore_backup(&snapshot, &target),
            Err(BackupServiceError::SnapshotInvalid)
        );
        assert!(!target.exists());
    }

    #[test]
    fn refuses_snapshot_destination_inside_the_source_workspace() {
        let parent = TestDirectory::new("inside-source");
        let root = workspace(&parent);
        let destination = root.join("exports/unsafe-snapshot");

        assert_eq!(
            BackupService::new().create_backup(&root, &destination, BackupKind::Full),
            Err(BackupServiceError::InvalidPath)
        );
        assert!(!destination.exists());
    }

    #[test]
    fn rejects_source_links_when_the_platform_can_create_them() {
        let parent = TestDirectory::new("link");
        let root = workspace(&parent);
        let link = root.join("media/images/linked");
        let target = parent.child("outside");
        fs::create_dir(&target).expect("应能创建链接目标");
        #[cfg(windows)]
        let created = std::os::windows::fs::symlink_dir(&target, &link).is_ok();
        #[cfg(unix)]
        let created = {
            std::os::unix::fs::symlink(&target, &link).expect("应能创建链接");
            true
        };
        if !created {
            return;
        }
        assert_eq!(
            BackupService::new().create_backup(&root, &parent.child("snapshot"), BackupKind::Full),
            Err(BackupServiceError::SnapshotUnsafe)
        );
    }
}
