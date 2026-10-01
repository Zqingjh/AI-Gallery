use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path},
};

use serde::{Deserialize, Serialize};

use crate::domain::{
    PathAvailability, StoredPathKind, StoredPathStatus, WORKSPACE_FORMAT_VERSION, WorkspaceInfo,
};

const MANIFEST_RELATIVE_PATH: &str = "data/workspace.json";
const MANIFEST_TEMP_RELATIVE_PATH: &str = "data/workspace.json.tmp";
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const REQUIRED_DIRECTORIES: [&str; 10] = [
    "data",
    "media",
    "cache",
    "exports",
    "data/backups",
    "media/images",
    "media/videos",
    "media/references",
    "media/covers",
    "cache/thumbnails",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceServiceError {
    InvalidPath,
    AlreadyExists,
    NotFound,
    ManifestMissing,
    ManifestTooLarge,
    ManifestInvalid,
    VersionUnsupported,
    StructureInvalid,
    CreateFailed,
    OpenFailed,
    RollbackFailed,
    WorkspaceOverlap,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkspaceManifest {
    format_version: u32,
}

#[derive(Debug, Default)]
pub(crate) struct WorkspaceService;

impl WorkspaceService {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn create_workspace(
        &self,
        root: &Path,
    ) -> Result<WorkspaceInfo, WorkspaceServiceError> {
        self.create_workspace_with(root, |created_root| {
            self.initialize_workspace(created_root)?;
            self.open_workspace(created_root)
        })
    }

    fn create_workspace_with<T, F>(
        &self,
        root: &Path,
        initializer: F,
    ) -> Result<T, WorkspaceServiceError>
    where
        F: FnOnce(&Path) -> Result<T, WorkspaceServiceError>,
    {
        validate_new_workspace_root(root)?;
        fs::create_dir(root).map_err(|_| WorkspaceServiceError::CreateFailed)?;

        match initializer(root) {
            Ok(value) => Ok(value),
            Err(error) => match fs::remove_dir_all(root) {
                Ok(()) => Err(error),
                Err(_) => Err(WorkspaceServiceError::RollbackFailed),
            },
        }
    }

    fn initialize_workspace(&self, root: &Path) -> Result<(), WorkspaceServiceError> {
        for relative_directory in REQUIRED_DIRECTORIES {
            fs::create_dir_all(root.join(relative_directory))
                .map_err(|_| WorkspaceServiceError::CreateFailed)?;
        }

        let manifest = WorkspaceManifest {
            format_version: WORKSPACE_FORMAT_VERSION,
        };
        let bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|_| WorkspaceServiceError::CreateFailed)?;
        let temporary_path = root.join(MANIFEST_TEMP_RELATIVE_PATH);
        let manifest_path = root.join(MANIFEST_RELATIVE_PATH);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
            .map_err(|_| WorkspaceServiceError::CreateFailed)?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| WorkspaceServiceError::CreateFailed)?;
        drop(file);
        fs::rename(temporary_path, manifest_path)
            .map_err(|_| WorkspaceServiceError::CreateFailed)?;
        Ok(())
    }

    pub(crate) fn open_workspace(
        &self,
        root: &Path,
    ) -> Result<WorkspaceInfo, WorkspaceServiceError> {
        validate_absolute_path(root)?;
        let root_metadata = match fs::symlink_metadata(root) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(WorkspaceServiceError::NotFound);
            }
            Err(_) => return Err(WorkspaceServiceError::OpenFailed),
        };
        if !root_metadata.is_dir() {
            return Err(WorkspaceServiceError::NotFound);
        }
        reject_link_or_reparse(&root_metadata)?;

        // 先检查父目录再检查子目录，避免通过父级重解析点触达库外路径。
        for relative_directory in REQUIRED_DIRECTORIES {
            let directory = root.join(relative_directory);
            let metadata = fs::symlink_metadata(&directory)
                .map_err(|_| WorkspaceServiceError::StructureInvalid)?;
            if !metadata.is_dir() {
                return Err(WorkspaceServiceError::StructureInvalid);
            }
            reject_link_or_reparse(&metadata)?;
        }

        let manifest_path = root.join(MANIFEST_RELATIVE_PATH);
        let manifest_metadata = match fs::symlink_metadata(&manifest_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(WorkspaceServiceError::ManifestMissing);
            }
            Err(_) => return Err(WorkspaceServiceError::OpenFailed),
        };
        reject_link_or_reparse(&manifest_metadata)?;
        if !manifest_metadata.is_file() {
            return Err(WorkspaceServiceError::ManifestInvalid);
        }
        if manifest_metadata.len() > MAX_MANIFEST_BYTES {
            return Err(WorkspaceServiceError::ManifestTooLarge);
        }

        let mut bytes = Vec::with_capacity(
            usize::try_from(manifest_metadata.len().min(MAX_MANIFEST_BYTES))
                .map_err(|_| WorkspaceServiceError::ManifestTooLarge)?,
        );
        File::open(manifest_path)
            .map_err(|_| WorkspaceServiceError::OpenFailed)?
            .take(MAX_MANIFEST_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| WorkspaceServiceError::OpenFailed)?;
        if bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(WorkspaceServiceError::ManifestTooLarge);
        }
        let manifest: WorkspaceManifest =
            serde_json::from_slice(&bytes).map_err(|_| WorkspaceServiceError::ManifestInvalid)?;
        if manifest.format_version != WORKSPACE_FORMAT_VERSION {
            return Err(WorkspaceServiceError::VersionUnsupported);
        }

        let display_name = root
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .ok_or(WorkspaceServiceError::InvalidPath)?
            .to_owned();

        Ok(WorkspaceInfo {
            display_name,
            format_version: manifest.format_version,
            ready: true,
        })
    }

    pub(crate) fn check_stored_path(
        &self,
        workspace_root: &Path,
        kind: StoredPathKind,
        stored_path: &Path,
    ) -> Result<StoredPathStatus, WorkspaceServiceError> {
        validate_absolute_path(workspace_root)?;
        match kind {
            StoredPathKind::Managed => self.check_managed_path(workspace_root, stored_path),
            StoredPathKind::External => self.check_external_path(stored_path),
        }
    }

    /// 正常与隔离工作区不得相同或互相嵌套，避免数据串库及备份边界重叠。
    pub(crate) fn validate_isolated_workspace(
        &self,
        primary_root: &Path,
        isolated_root: &Path,
    ) -> Result<(), WorkspaceServiceError> {
        let primary = comparable_path(primary_root)?;
        let isolated = comparable_path(isolated_root)?;
        if primary == isolated || primary.starts_with(&isolated) || isolated.starts_with(&primary) {
            return Err(WorkspaceServiceError::WorkspaceOverlap);
        }
        Ok(())
    }

    fn check_managed_path(
        &self,
        workspace_root: &Path,
        stored_path: &Path,
    ) -> Result<StoredPathStatus, WorkspaceServiceError> {
        validate_absolute_path(workspace_root)?;
        if stored_path.as_os_str().is_empty() || stored_path.is_absolute() {
            return Err(WorkspaceServiceError::InvalidPath);
        }

        let mut portable_components = Vec::new();
        for component in stored_path.components() {
            match component {
                Component::Normal(value) => portable_components
                    .push(value.to_str().ok_or(WorkspaceServiceError::InvalidPath)?),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(WorkspaceServiceError::InvalidPath);
                }
            }
        }
        if portable_components.is_empty() {
            return Err(WorkspaceServiceError::InvalidPath);
        }

        let root_canonical =
            fs::canonicalize(workspace_root).map_err(|_| WorkspaceServiceError::InvalidPath)?;
        let candidate = workspace_root.join(stored_path);
        let available = candidate
            .try_exists()
            .map_err(|_| WorkspaceServiceError::OpenFailed)?;
        let boundary_target = if available {
            fs::canonicalize(&candidate).map_err(|_| WorkspaceServiceError::OpenFailed)?
        } else {
            canonicalize_existing_ancestor(&candidate)?
        };
        if !boundary_target.starts_with(&root_canonical) {
            return Err(WorkspaceServiceError::InvalidPath);
        }

        Ok(StoredPathStatus {
            kind: StoredPathKind::Managed,
            availability: if available {
                PathAvailability::Available
            } else {
                PathAvailability::Missing
            },
            portable_path: Some(portable_components.join("/")),
        })
    }

    fn check_external_path(
        &self,
        stored_path: &Path,
    ) -> Result<StoredPathStatus, WorkspaceServiceError> {
        validate_absolute_path(stored_path)?;
        let available = stored_path
            .try_exists()
            .map_err(|_| WorkspaceServiceError::OpenFailed)?;

        Ok(StoredPathStatus {
            kind: StoredPathKind::External,
            availability: if available {
                PathAvailability::Available
            } else {
                PathAvailability::Missing
            },
            portable_path: None,
        })
    }
}

fn validate_absolute_path(path: &Path) -> Result<(), WorkspaceServiceError> {
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return Err(WorkspaceServiceError::InvalidPath);
    }
    Ok(())
}

fn validate_new_workspace_root(root: &Path) -> Result<(), WorkspaceServiceError> {
    validate_absolute_path(root)?;
    if root
        .try_exists()
        .map_err(|_| WorkspaceServiceError::CreateFailed)?
    {
        return Err(WorkspaceServiceError::AlreadyExists);
    }
    let parent = root.parent().ok_or(WorkspaceServiceError::InvalidPath)?;
    if !parent.is_dir() || root.file_name().is_none() {
        return Err(WorkspaceServiceError::InvalidPath);
    }
    Ok(())
}

fn canonicalize_existing_ancestor(
    path: &Path,
) -> Result<std::path::PathBuf, WorkspaceServiceError> {
    let mut current = path.parent().ok_or(WorkspaceServiceError::InvalidPath)?;
    while !current
        .try_exists()
        .map_err(|_| WorkspaceServiceError::OpenFailed)?
    {
        current = current.parent().ok_or(WorkspaceServiceError::InvalidPath)?;
    }
    fs::canonicalize(current).map_err(|_| WorkspaceServiceError::OpenFailed)
}

fn comparable_path(path: &Path) -> Result<std::path::PathBuf, WorkspaceServiceError> {
    validate_absolute_path(path)?;
    let mut current = path.to_path_buf();
    let mut missing = Vec::new();
    while !current
        .try_exists()
        .map_err(|_| WorkspaceServiceError::OpenFailed)?
    {
        let name = current
            .file_name()
            .ok_or(WorkspaceServiceError::InvalidPath)?
            .to_owned();
        missing.push(name);
        current = current
            .parent()
            .ok_or(WorkspaceServiceError::InvalidPath)?
            .to_path_buf();
    }
    let mut resolved = fs::canonicalize(current).map_err(|_| WorkspaceServiceError::OpenFailed)?;
    for component in missing.into_iter().rev() {
        resolved.push(component);
    }
    #[cfg(windows)]
    {
        resolved = std::path::PathBuf::from(resolved.to_string_lossy().to_lowercase());
    }
    Ok(resolved)
}

fn reject_link_or_reparse(metadata: &fs::Metadata) -> Result<(), WorkspaceServiceError> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(WorkspaceServiceError::StructureInvalid);
        }
    }

    #[cfg(not(windows))]
    if metadata.file_type().is_symlink() {
        return Err(WorkspaceServiceError::StructureInvalid);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::{
        MANIFEST_RELATIVE_PATH, MAX_MANIFEST_BYTES, REQUIRED_DIRECTORIES, WorkspaceService,
        WorkspaceServiceError,
    };
    use crate::domain::{PathAvailability, StoredPathKind, WORKSPACE_FORMAT_VERSION};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let sequence = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "ai-gallery-{label}-{}-{sequence}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir(&path).expect("应能创建隔离测试目录");
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

    fn create_workspace_fixture(label: &str) -> (TestDirectory, PathBuf) {
        let parent = TestDirectory::new(label);
        let root = parent.child("workspace");
        WorkspaceService::new()
            .create_workspace(&root)
            .expect("应能创建测试工作区");
        (parent, root)
    }

    fn create_directory_link(target: &Path, link: &Path) -> bool {
        #[cfg(windows)]
        {
            if let Err(error) = std::os::windows::fs::symlink_dir(target, link) {
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::Unsupported
                ) {
                    // 普通 Windows 环境可能未启用开发者模式；此时无法安全创建测试重解析点。
                    return false;
                }
                panic!("创建测试目录链接失败：{error}");
            }
        }

        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).expect("应能创建测试目录链接");

        true
    }

    fn create_file_link(target: &Path, link: &Path) -> bool {
        #[cfg(windows)]
        {
            if let Err(error) = std::os::windows::fs::symlink_file(target, link) {
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::Unsupported
                ) {
                    return false;
                }
                panic!("创建测试文件链接失败：{error}");
            }
        }

        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).expect("应能创建测试文件链接");

        true
    }

    #[test]
    fn creates_prd_workspace_structure_and_manifest() {
        let parent = TestDirectory::new("create");
        let root = parent.child("作品库");

        let info = WorkspaceService::new()
            .create_workspace(&root)
            .expect("应能创建工作区");

        assert_eq!(info.display_name, "作品库");
        assert_eq!(info.format_version, WORKSPACE_FORMAT_VERSION);
        assert!(info.ready);
        for relative_directory in REQUIRED_DIRECTORIES {
            assert!(root.join(relative_directory).is_dir());
        }
        let manifest =
            fs::read_to_string(root.join(MANIFEST_RELATIVE_PATH)).expect("应能读取 manifest");
        assert_eq!(manifest.trim(), "{\n  \"formatVersion\": 1\n}");
    }

    #[test]
    fn never_overwrites_an_existing_target() {
        let parent = TestDirectory::new("existing");
        let root = parent.child("workspace");
        fs::create_dir(&root).expect("应能创建既有目录");
        fs::write(root.join("keep.txt"), "保留").expect("应能创建保护文件");

        let error = WorkspaceService::new()
            .create_workspace(&root)
            .expect_err("既有目标必须被拒绝");

        assert_eq!(error, WorkspaceServiceError::AlreadyExists);
        assert!(root.join("keep.txt").is_file());
    }

    #[test]
    fn isolated_workspace_rejects_same_or_nested_roots_but_allows_sibling() {
        let (parent, primary) = create_workspace_fixture("isolated-root");
        let service = WorkspaceService::new();

        assert_eq!(
            service.validate_isolated_workspace(&primary, &primary),
            Err(WorkspaceServiceError::WorkspaceOverlap)
        );
        assert_eq!(
            service.validate_isolated_workspace(&primary, &primary.join("private")),
            Err(WorkspaceServiceError::WorkspaceOverlap)
        );
        assert_eq!(
            service.validate_isolated_workspace(&primary, &parent.0),
            Err(WorkspaceServiceError::WorkspaceOverlap)
        );
        assert!(
            service
                .validate_isolated_workspace(&primary, &parent.child("private-workspace"))
                .is_ok()
        );
    }

    #[test]
    fn rolls_back_only_the_root_created_by_this_attempt() {
        let parent = TestDirectory::new("rollback");
        let root = parent.child("workspace");
        let sentinel = parent.child("keep.txt");
        fs::write(&sentinel, "保留").expect("应能创建父目录文件");

        let error = WorkspaceService::new()
            .create_workspace_with(&root, |created_root| {
                fs::write(created_root.join("partial.txt"), "半成品").expect("应能创建半成品");
                Err::<(), _>(WorkspaceServiceError::CreateFailed)
            })
            .expect_err("初始化失败应触发回滚");

        assert_eq!(error, WorkspaceServiceError::CreateFailed);
        assert!(!root.exists());
        assert!(sentinel.is_file());
    }

    #[test]
    fn opens_valid_workspace_without_enumerating_media() {
        let (_parent, root) = create_workspace_fixture("open");
        fs::write(root.join("media/images/unrelated.txt"), "无需读取")
            .expect("应能创建无关媒体占位文件");

        let info = WorkspaceService::new()
            .open_workspace(&root)
            .expect("固定结构检查应成功");

        assert_eq!(info.display_name, "workspace");
        assert!(info.ready);
    }

    #[test]
    fn reports_manifest_and_structure_failures() {
        let service = WorkspaceService::new();
        let (_missing_parent, missing_root) = create_workspace_fixture("manifest-missing");
        fs::remove_file(missing_root.join(MANIFEST_RELATIVE_PATH)).expect("应能移除 manifest");
        assert_eq!(
            service.open_workspace(&missing_root),
            Err(WorkspaceServiceError::ManifestMissing)
        );

        let (_large_parent, large_root) = create_workspace_fixture("manifest-large");
        fs::write(
            large_root.join(MANIFEST_RELATIVE_PATH),
            vec![b'x'; MAX_MANIFEST_BYTES as usize + 1],
        )
        .expect("应能写入超限 manifest");
        assert_eq!(
            service.open_workspace(&large_root),
            Err(WorkspaceServiceError::ManifestTooLarge)
        );

        let (_invalid_parent, invalid_root) = create_workspace_fixture("manifest-invalid");
        fs::write(invalid_root.join(MANIFEST_RELATIVE_PATH), "not-json")
            .expect("应能写入损坏 manifest");
        assert_eq!(
            service.open_workspace(&invalid_root),
            Err(WorkspaceServiceError::ManifestInvalid)
        );

        let (_version_parent, version_root) = create_workspace_fixture("manifest-version");
        fs::write(
            version_root.join(MANIFEST_RELATIVE_PATH),
            "{\"formatVersion\":2}",
        )
        .expect("应能写入高版本 manifest");
        assert_eq!(
            service.open_workspace(&version_root),
            Err(WorkspaceServiceError::VersionUnsupported)
        );

        let (_structure_parent, structure_root) = create_workspace_fixture("structure");
        fs::remove_dir(structure_root.join("media/covers")).expect("应能移除固定目录");
        assert_eq!(
            service.open_workspace(&structure_root),
            Err(WorkspaceServiceError::StructureInvalid)
        );
    }

    #[test]
    fn rejects_manifest_with_missing_or_unknown_fields() {
        let service = WorkspaceService::new();
        let (_missing_field_parent, missing_field_root) =
            create_workspace_fixture("manifest-missing-field");
        fs::write(missing_field_root.join(MANIFEST_RELATIVE_PATH), "{}")
            .expect("应能写入缺字段 manifest");
        assert_eq!(
            service.open_workspace(&missing_field_root),
            Err(WorkspaceServiceError::ManifestInvalid)
        );

        let (_unknown_field_parent, unknown_field_root) =
            create_workspace_fixture("manifest-unknown-field");
        fs::write(
            unknown_field_root.join(MANIFEST_RELATIVE_PATH),
            "{\"formatVersion\":1,\"privatePath\":\"D:\\\\private\\\\secret.db\"}",
        )
        .expect("应能写入含未知字段 manifest");
        assert_eq!(
            service.open_workspace(&unknown_field_root),
            Err(WorkspaceServiceError::ManifestInvalid)
        );
    }

    #[test]
    fn rejects_non_absolute_workspace_and_external_roots() {
        let service = WorkspaceService::new();
        let relative_root = Path::new("relative-workspace");

        assert_eq!(
            service.create_workspace(relative_root),
            Err(WorkspaceServiceError::InvalidPath)
        );
        assert_eq!(
            service.open_workspace(relative_root),
            Err(WorkspaceServiceError::InvalidPath)
        );
        assert_eq!(
            service.check_stored_path(
                relative_root,
                StoredPathKind::Managed,
                Path::new("media/images/example.png"),
            ),
            Err(WorkspaceServiceError::InvalidPath)
        );
        assert_eq!(
            service.check_stored_path(
                relative_root,
                StoredPathKind::External,
                &std::env::temp_dir().join("outside-example.png"),
            ),
            Err(WorkspaceServiceError::InvalidPath)
        );
    }

    #[test]
    fn normalizes_managed_paths_and_rejects_escape_attempts() {
        let (_parent, root) = create_workspace_fixture("managed-path");
        let image = root.join("media/images/example.png");
        fs::write(&image, "image").expect("应能创建媒体占位文件");
        let service = WorkspaceService::new();

        let status = service
            .check_stored_path(
                &root,
                StoredPathKind::Managed,
                Path::new("media/images/example.png"),
            )
            .expect("安全相对路径应可解析");
        assert_eq!(status.kind, StoredPathKind::Managed);
        assert_eq!(status.availability, PathAvailability::Available);
        assert_eq!(
            status.portable_path.as_deref(),
            Some("media/images/example.png")
        );

        assert_eq!(
            service.check_stored_path(&root, StoredPathKind::Managed, Path::new("../outside.png")),
            Err(WorkspaceServiceError::InvalidPath)
        );
        assert_eq!(
            service.check_stored_path(&root, StoredPathKind::Managed, &root.join("absolute.png")),
            Err(WorkspaceServiceError::InvalidPath)
        );
    }

    #[test]
    fn preserves_portable_path_for_missing_managed_media() {
        let (_parent, root) = create_workspace_fixture("managed-missing");

        let status = WorkspaceService::new()
            .check_stored_path(
                &root,
                StoredPathKind::Managed,
                Path::new("media/images/nested/missing.png"),
            )
            .expect("库内缺失路径仍应保留可迁移表示");

        assert_eq!(status.kind, StoredPathKind::Managed);
        assert_eq!(status.availability, PathAvailability::Missing);
        assert_eq!(
            status.portable_path.as_deref(),
            Some("media/images/nested/missing.png")
        );
    }

    #[test]
    fn canonical_boundary_rejects_existing_link_that_escapes_workspace() {
        let (_workspace_parent, root) = create_workspace_fixture("managed-link-escape");
        let outside = TestDirectory::new("outside-link-target");
        fs::write(outside.child("secret.png"), "不应成为库内文件").expect("应能创建库外目标文件");
        let link = root.join("media/images/outside-link");

        if !create_directory_link(&outside.0, &link) {
            return;
        }

        assert_eq!(
            WorkspaceService::new().check_stored_path(
                &root,
                StoredPathKind::Managed,
                Path::new("media/images/outside-link/secret.png"),
            ),
            Err(WorkspaceServiceError::InvalidPath)
        );
    }

    #[test]
    fn open_rejects_manifest_link_that_escapes_workspace() {
        let (_workspace_parent, root) = create_workspace_fixture("manifest-link-escape");
        let outside = TestDirectory::new("outside-manifest-target");
        let outside_manifest = outside.child("workspace.json");
        fs::write(&outside_manifest, "{\"formatVersion\":1}").expect("应能创建库外 manifest");
        let manifest = root.join(MANIFEST_RELATIVE_PATH);
        fs::remove_file(&manifest).expect("应能移除库内 manifest");
        if !create_file_link(&outside_manifest, &manifest) {
            return;
        }

        assert_eq!(
            WorkspaceService::new().open_workspace(&root),
            Err(WorkspaceServiceError::StructureInvalid)
        );
    }

    #[test]
    fn open_rejects_fixed_directory_link_that_escapes_workspace() {
        let (_workspace_parent, root) = create_workspace_fixture("media-link-escape");
        let outside = TestDirectory::new("outside-media-target");
        for directory in ["images", "videos", "references", "covers"] {
            fs::create_dir(outside.child(directory)).expect("应能创建库外媒体目录");
        }
        let media = root.join("media");
        fs::remove_dir_all(&media).expect("应能移除库内媒体目录");
        if !create_directory_link(&outside.0, &media) {
            return;
        }

        assert_eq!(
            WorkspaceService::new().open_workspace(&root),
            Err(WorkspaceServiceError::StructureInvalid)
        );
    }

    #[test]
    fn missing_external_paths_remain_valid_references() {
        let parent = TestDirectory::new("external");
        let missing = parent.child("missing.mp4");

        let status = WorkspaceService::new()
            .check_stored_path(&parent.0, StoredPathKind::External, &missing)
            .expect("缺失外部文件不应破坏记录");

        assert_eq!(status.kind, StoredPathKind::External);
        assert_eq!(status.availability, PathAvailability::Missing);
        assert_eq!(status.portable_path, None);
    }

    #[test]
    #[ignore = "手动性能基准：会创建 10,000 个临时占位文件"]
    fn benchmark_workspace_open_is_independent_of_media_count() {
        use std::{hint::black_box, time::Instant};

        let (_parent, root) = create_workspace_fixture("open-benchmark");
        let images = root.join("media/images");
        let service = WorkspaceService::new();
        let mut created = 0usize;

        for target_count in [100usize, 1_000, 10_000] {
            for index in created..target_count {
                fs::write(images.join(format!("asset-{index}.jpg")), [])
                    .expect("应能创建性能占位文件");
            }
            created = target_count;

            for _ in 0..10 {
                black_box(service.open_workspace(&root).expect("基准工作区应能打开"));
            }

            let iterations = 500u32;
            let started = Instant::now();
            for _ in 0..iterations {
                black_box(service.open_workspace(&root).expect("基准工作区应能打开"));
            }
            let elapsed = started.elapsed();
            println!(
                "workspace_open_benchmark media_count={target_count} iterations={iterations} total_us={} average_us={:.3}",
                elapsed.as_micros(),
                elapsed.as_secs_f64() * 1_000_000.0 / f64::from(iterations)
            );
        }
    }
}
