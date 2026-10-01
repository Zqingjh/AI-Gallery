use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    domain::{DATABASE_SCHEMA_VERSION, DatabaseStatus},
    repositories::{DatabaseRepository, DatabaseRepositoryError},
    services::{WorkspaceService, WorkspaceServiceError},
};

const DATABASE_RELATIVE_PATH: &str = "data/library.sqlite3";
const BACKUP_RELATIVE_DIRECTORY: &str = "data/backups";
const INITIAL_MIGRATION: &str = include_str!("../migrations/0001_initial.sql");
const SEARCH_MIGRATION: &str = include_str!("../migrations/0002_search.sql");
const AI_MIGRATION: &str = include_str!("../migrations/0003_ai.sql");
const P1_LIBRARY_MIGRATION: &str = include_str!("../migrations/0004_p1_library.sql");
const MEDIA_INTEGRITY_MIGRATION: &str = include_str!("../migrations/0005_media_integrity.sql");
const ASSET_ORDER_MIGRATION: &str = include_str!("../migrations/0006_asset_order_and_taxonomy.sql");
const COMMON_TAXONOMY_MIGRATION: &str = include_str!("../migrations/0007_common_taxonomy.sql");
const METADATA_PRESETS_MIGRATION: &str =
    include_str!("../migrations/0008_metadata_presets_and_import_order.sql");
const MEDIA_ORDER_REPAIR_MIGRATION: &str =
    include_str!("../migrations/0009_media_metadata_and_order_repair.sql");
const CANVAS_PROJECTS_MIGRATION: &str = include_str!("../migrations/0010_canvas_projects.sql");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DatabaseServiceError {
    Workspace(WorkspaceServiceError),
    UnsafePath,
    OpenFailed,
    BackupFailed,
    MigrationFailed,
    VersionUnsupported,
    SchemaInvalid,
}

impl From<WorkspaceServiceError> for DatabaseServiceError {
    fn from(error: WorkspaceServiceError) -> Self {
        Self::Workspace(error)
    }
}

pub(crate) struct DatabaseService;

impl DatabaseService {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn prepare_workspace_database(
        &self,
        workspace_root: &Path,
    ) -> Result<DatabaseStatus, DatabaseServiceError> {
        self.prepare_with_migrations(
            workspace_root,
            INITIAL_MIGRATION,
            SEARCH_MIGRATION,
            AI_MIGRATION,
            P1_LIBRARY_MIGRATION,
            MEDIA_INTEGRITY_MIGRATION,
            ASSET_ORDER_MIGRATION,
            COMMON_TAXONOMY_MIGRATION,
            METADATA_PRESETS_MIGRATION,
            MEDIA_ORDER_REPAIR_MIGRATION,
            CANVAS_PROJECTS_MIGRATION,
        )
    }

    /// 只打开并校验已经升级到当前版本的数据库，不创建文件、不执行迁移。
    /// 普通查询和领域服务必须走此入口，避免读路径绕过访问模式触发结构写入。
    pub(crate) fn open_current_workspace_database(
        &self,
        workspace_root: &Path,
    ) -> Result<rusqlite::Connection, DatabaseServiceError> {
        let connection = self.open_existing_workspace_database(workspace_root)?;
        DatabaseRepository::validate_v10(&connection).map_err(map_repository_error)?;
        Ok(connection)
    }

    /// 访问模式存在于最早版本的 app_settings 中，因此迁移前只允许访问模式服务
    /// 使用这个入口读取旧版本；调用方仍必须自行读取并校验具体设置。
    pub(crate) fn open_access_mode_database(
        &self,
        workspace_root: &Path,
    ) -> Result<rusqlite::Connection, DatabaseServiceError> {
        let connection = self.open_existing_workspace_database(workspace_root)?;
        let version =
            DatabaseRepository::schema_version(&connection).map_err(map_repository_error)?;
        if version > DATABASE_SCHEMA_VERSION {
            return Err(DatabaseServiceError::VersionUnsupported);
        }
        Ok(connection)
    }

    fn open_existing_workspace_database(
        &self,
        workspace_root: &Path,
    ) -> Result<rusqlite::Connection, DatabaseServiceError> {
        WorkspaceService::new().open_workspace(workspace_root)?;
        let database_path = workspace_root.join(DATABASE_RELATIVE_PATH);
        if !database_path
            .try_exists()
            .map_err(|_| DatabaseServiceError::OpenFailed)?
        {
            return Err(DatabaseServiceError::OpenFailed);
        }
        validate_regular_non_link_file(&database_path)?;
        let connection = DatabaseRepository::open(&database_path).map_err(map_repository_error)?;
        validate_regular_non_link_file(&database_path)?;
        Ok(connection)
    }

    #[cfg(test)]
    fn prepare_with_migration(
        &self,
        workspace_root: &Path,
        migration_sql: &str,
    ) -> Result<DatabaseStatus, DatabaseServiceError> {
        self.prepare_with_migrations(
            workspace_root,
            migration_sql,
            SEARCH_MIGRATION,
            AI_MIGRATION,
            P1_LIBRARY_MIGRATION,
            MEDIA_INTEGRITY_MIGRATION,
            ASSET_ORDER_MIGRATION,
            COMMON_TAXONOMY_MIGRATION,
            METADATA_PRESETS_MIGRATION,
            MEDIA_ORDER_REPAIR_MIGRATION,
            CANVAS_PROJECTS_MIGRATION,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "迁移脚本逐个传入，便于测试单个版本升级失败时的备份与回滚"
    )]
    fn prepare_with_migrations(
        &self,
        workspace_root: &Path,
        initial_migration_sql: &str,
        search_migration_sql: &str,
        ai_migration_sql: &str,
        p1_library_migration_sql: &str,
        media_integrity_migration_sql: &str,
        asset_order_migration_sql: &str,
        common_taxonomy_migration_sql: &str,
        metadata_presets_migration_sql: &str,
        media_order_repair_migration_sql: &str,
        canvas_projects_migration_sql: &str,
    ) -> Result<DatabaseStatus, DatabaseServiceError> {
        WorkspaceService::new().open_workspace(workspace_root)?;

        let database_path = workspace_root.join(DATABASE_RELATIVE_PATH);
        let database_existed = database_path
            .try_exists()
            .map_err(|_| DatabaseServiceError::OpenFailed)?;
        if database_existed {
            validate_regular_non_link_file(&database_path)?;
        }

        let mut connection =
            DatabaseRepository::open(&database_path).map_err(map_repository_error)?;
        // 关闭“检查到不存在后被替换”的明显窗口；不回传或记录实际路径。
        validate_regular_non_link_file(&database_path)?;

        let version =
            DatabaseRepository::schema_version(&connection).map_err(map_repository_error)?;
        if version > DATABASE_SCHEMA_VERSION {
            return Err(DatabaseServiceError::VersionUnsupported);
        }
        if version == DATABASE_SCHEMA_VERSION {
            DatabaseRepository::validate_v10(&connection).map_err(map_repository_error)?;
            return Ok(DatabaseStatus {
                schema_version: DATABASE_SCHEMA_VERSION,
                migrated: false,
                backup_created: false,
            });
        }
        if version == 1 {
            DatabaseRepository::validate_v1(&connection).map_err(map_repository_error)?;
        }
        if version == 2 {
            DatabaseRepository::validate_v2(&connection).map_err(map_repository_error)?;
        }
        if version == 3 {
            DatabaseRepository::validate_v3(&connection).map_err(map_repository_error)?;
        }
        if version == 4 {
            DatabaseRepository::validate_v4(&connection).map_err(map_repository_error)?;
        }
        if version == 5 {
            DatabaseRepository::validate_v5(&connection).map_err(map_repository_error)?;
        }
        if version == 6 {
            DatabaseRepository::validate_v6(&connection).map_err(map_repository_error)?;
        }
        if version == 7 {
            DatabaseRepository::validate_v7(&connection).map_err(map_repository_error)?;
        }
        if version == 8 {
            DatabaseRepository::validate_v8(&connection).map_err(map_repository_error)?;
        }
        if version == 9 {
            DatabaseRepository::validate_v9(&connection).map_err(map_repository_error)?;
        }

        let backup_created = if database_existed {
            create_consistent_backup(
                &connection,
                &workspace_root.join(BACKUP_RELATIVE_DIRECTORY),
                version,
            )?;
            true
        } else {
            false
        };

        DatabaseRepository::migrate_to_v10_atomic(
            &mut connection,
            version,
            initial_migration_sql,
            search_migration_sql,
            ai_migration_sql,
            p1_library_migration_sql,
            media_integrity_migration_sql,
            asset_order_migration_sql,
            common_taxonomy_migration_sql,
            metadata_presets_migration_sql,
            media_order_repair_migration_sql,
            canvas_projects_migration_sql,
        )
        .map_err(map_repository_error)?;

        Ok(DatabaseStatus {
            schema_version: DATABASE_SCHEMA_VERSION,
            migrated: true,
            backup_created,
        })
    }
}

fn map_repository_error(error: DatabaseRepositoryError) -> DatabaseServiceError {
    match error {
        DatabaseRepositoryError::OpenFailed | DatabaseRepositoryError::ReadVersionFailed => {
            DatabaseServiceError::OpenFailed
        }
        DatabaseRepositoryError::MigrationFailed => DatabaseServiceError::MigrationFailed,
        DatabaseRepositoryError::SchemaInvalid => DatabaseServiceError::SchemaInvalid,
        DatabaseRepositoryError::BackupFailed => DatabaseServiceError::BackupFailed,
    }
}

struct ReservedBackup {
    path: PathBuf,
    file: File,
}

fn create_consistent_backup(
    source: &rusqlite::Connection,
    directory: &Path,
    from_version: u32,
) -> Result<(), DatabaseServiceError> {
    let reserved = reserve_backup_file(directory, from_version)?;
    let result = (|| {
        let mut destination = DatabaseRepository::open_backup_destination(&reserved.path)
            .map_err(map_repository_error)?;
        validate_reserved_backup(&reserved)?;
        DatabaseRepository::backup(source, &mut destination).map_err(map_repository_error)?;
        drop(destination);
        reserved
            .file
            .sync_all()
            .map_err(|_| DatabaseServiceError::BackupFailed)?;
        validate_reserved_backup(&reserved)
    })();

    if result.is_err() {
        let path = reserved.path.clone();
        drop(reserved);
        let _ = fs::remove_file(path);
        return Err(DatabaseServiceError::BackupFailed);
    }
    Ok(())
}

fn reserve_backup_file(
    directory: &Path,
    from_version: u32,
) -> Result<ReservedBackup, DatabaseServiceError> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| DatabaseServiceError::BackupFailed)?
        .as_millis();

    for suffix in 0_u8..100 {
        let file_name = if suffix == 0 {
            format!("library-before-v{}-{timestamp}.sqlite3", from_version + 1)
        } else {
            format!(
                "library-before-v{}-{timestamp}-{suffix}.sqlite3",
                from_version + 1
            )
        };
        let candidate = directory.join(file_name);
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // 允许 SQLite 另开读写句柄，但不共享删除权限，预留期间无法替换目标。
            const FILE_SHARE_READ: u32 = 0x0000_0001;
            const FILE_SHARE_WRITE: u32 = 0x0000_0002;
            options.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
        }
        match options.open(&candidate) {
            Ok(file) => {
                file.sync_all()
                    .map_err(|_| DatabaseServiceError::BackupFailed)?;
                let reserved = ReservedBackup {
                    path: candidate,
                    file,
                };
                validate_reserved_backup(&reserved)?;
                return Ok(reserved);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(DatabaseServiceError::BackupFailed),
        }
    }
    Err(DatabaseServiceError::BackupFailed)
}

fn validate_reserved_backup(reserved: &ReservedBackup) -> Result<(), DatabaseServiceError> {
    validate_regular_non_link_file(&reserved.path)?;

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::MetadataExt;
        let open_metadata = reserved
            .file
            .metadata()
            .map_err(|_| DatabaseServiceError::BackupFailed)?;
        let path_metadata =
            fs::metadata(&reserved.path).map_err(|_| DatabaseServiceError::BackupFailed)?;
        if open_metadata.dev() != path_metadata.dev() || open_metadata.ino() != path_metadata.ino()
        {
            return Err(DatabaseServiceError::BackupFailed);
        }
    }

    Ok(())
}

fn validate_regular_non_link_file(path: &Path) -> Result<(), DatabaseServiceError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| DatabaseServiceError::UnsafePath)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(DatabaseServiceError::UnsafePath);
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(DatabaseServiceError::UnsafePath);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
        time::Instant,
    };

    use rusqlite::Connection;

    use super::{DatabaseService, DatabaseServiceError};
    use crate::services::WorkspaceService;

    static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(1);

    struct TestWorkspace {
        root: PathBuf,
    }

    impl TestWorkspace {
        fn create(name: &str) -> Self {
            let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
            let root = env::temp_dir().join(format!(
                "ai-gallery-database-{name}-{}-{id}",
                std::process::id()
            ));
            if root.exists() {
                fs::remove_dir_all(&root).expect("应能清理冲突测试目录");
            }
            WorkspaceService::new()
                .create_workspace(&root)
                .expect("应能创建测试工作区");
            Self { root }
        }

        fn database_path(&self) -> PathBuf {
            self.root.join("data/library.sqlite3")
        }

        fn backup_directory(&self) -> PathBuf {
            self.root.join("data/backups")
        }
    }

    impl Drop for TestWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn backup_files(directory: &Path) -> Vec<PathBuf> {
        fs::read_dir(directory)
            .expect("应能读取备份目录")
            .map(|entry| entry.expect("备份目录项应可读").path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("sqlite3"))
            .collect()
    }

    fn prepare_v9_database(workspace: &TestWorkspace) {
        let connection =
            Connection::open(workspace.database_path()).expect("应能创建 v9 测试数据库");
        for migration in [
            super::INITIAL_MIGRATION,
            super::SEARCH_MIGRATION,
            super::AI_MIGRATION,
            super::P1_LIBRARY_MIGRATION,
            super::MEDIA_INTEGRITY_MIGRATION,
            super::ASSET_ORDER_MIGRATION,
            super::COMMON_TAXONOMY_MIGRATION,
            super::METADATA_PRESETS_MIGRATION,
            super::MEDIA_ORDER_REPAIR_MIGRATION,
        ] {
            connection
                .execute_batch(migration)
                .expect("应能应用历史 migration 构造 v9 数据库");
        }
        connection
            .execute(
                "INSERT INTO projects(id,title,description,notes,created_at,updated_at)
                 VALUES(41,'既有项目','既有说明','既有项目备注',100,100)",
                [],
            )
            .expect("应能写入既有项目");
        connection
            .execute(
                "INSERT INTO models(id,name,provider,created_at,updated_at)
                 VALUES(42,'既有模型','既有提供方',100,100)",
                [],
            )
            .expect("应能写入既有模型");
        connection
            .execute(
                "INSERT INTO platforms(id,name,created_at,updated_at)
                 VALUES(43,'既有平台',100,100)",
                [],
            )
            .expect("应能写入既有平台");
        connection
            .execute(
                "INSERT INTO prompts(id,project_id,title,prompt_zh,prompt_en,negative_prompt,notes,created_at,updated_at)
                 VALUES(44,41,'既有提示词','中文提示词','English prompt','不要模糊','既有提示词备注',100,100)",
                [],
            )
            .expect("应能写入既有提示词");
        connection
            .execute(
                "INSERT INTO assets(
                    id,project_id,prompt_id,model_id,platform_id,media_type,path_kind,
                    stored_path,file_name,notes,created_at,updated_at
                 ) VALUES(
                    45,41,44,42,43,'image','managed','media/images/keep.png',
                    'keep.png','既有作品备注',100,100
                 )",
                [],
            )
            .expect("应能写入既有作品");
        connection
            .pragma_update(None, "user_version", 9_u32)
            .expect("应能设置 v9 版本");
    }

    fn assert_v9_fixture_data_is_preserved(connection: &Connection) {
        let preserved = connection
            .query_row(
                "SELECT project.title,project.description,project.notes,asset.file_name,asset.notes,
                        model.name,platform.name,prompt.prompt_zh,prompt.prompt_en,prompt.negative_prompt
                   FROM assets asset
                   JOIN projects project ON project.id=asset.project_id
                   JOIN models model ON model.id=asset.model_id
                   JOIN platforms platform ON platform.id=asset.platform_id
                   JOIN prompts prompt ON prompt.id=asset.prompt_id
                  WHERE asset.id=45",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, String>(9)?,
                    ))
                },
            )
            .expect("迁移前后的既有作品信息都应可读取");
        assert_eq!(
            preserved,
            (
                "既有项目".to_owned(),
                "既有说明".to_owned(),
                "既有项目备注".to_owned(),
                "keep.png".to_owned(),
                "既有作品备注".to_owned(),
                "既有模型".to_owned(),
                "既有平台".to_owned(),
                "中文提示词".to_owned(),
                "English prompt".to_owned(),
                "不要模糊".to_owned(),
            )
        );
    }

    #[test]
    fn new_database_migrates_without_backup() {
        let workspace = TestWorkspace::create("new");
        let status = DatabaseService::new()
            .prepare_workspace_database(&workspace.root)
            .expect("新数据库应迁移成功");

        assert_eq!(status.schema_version, 10);
        assert!(status.migrated);
        assert!(!status.backup_created);
        assert!(backup_files(&workspace.backup_directory()).is_empty());

        let connection = Connection::open(workspace.database_path()).expect("应能打开新数据库");
        let version: u32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("应能读取版本");
        assert_eq!(version, 10);
        let common_category_exists: bool = connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM categories c
                    JOIN dimensions d ON d.id = c.dimension_id
                    WHERE d.name = '视觉风格' AND c.name = '像素艺术'
                )",
                [],
                |row| row.get(0),
            )
            .expect("应能读取常用分类");
        assert!(common_category_exists);
        let preset_count: i64 = connection
            .query_row(
                "SELECT
                    (SELECT count(*) FROM platforms WHERE name IN ('豆包','千问','Gemini','Google Flow','即梦')) +
                    (SELECT count(*) FROM models WHERE name IN ('Seedream 5.0 Lite','Seedance 2.0','GPT-Image 2'))",
                [],
                |row| row.get(0),
            )
            .expect("应能读取默认元数据预设");
        assert_eq!(preset_count, 8);
    }

    #[test]
    fn existing_v3_database_is_backed_up_and_migrated_to_v5() {
        let workspace = TestWorkspace::create("v3-to-v4");
        let connection = Connection::open(workspace.database_path()).expect("应能创建 v3 数据库");
        connection
            .execute_batch(&format!(
                "{}\n{}\n{}\nPRAGMA user_version=3;",
                super::INITIAL_MIGRATION,
                super::SEARCH_MIGRATION,
                super::AI_MIGRATION
            ))
            .expect("应能准备 v3 数据库");
        drop(connection);

        let status = DatabaseService::new()
            .prepare_workspace_database(&workspace.root)
            .expect("v3 数据库应升级至 v4");
        assert_eq!(status.schema_version, 10);
        assert!(status.migrated);
        assert!(status.backup_created);
        let upgraded = Connection::open(workspace.database_path()).expect("应能打开升级后的数据库");
        let exists: bool = upgraded
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='saved_filters')",
                [],
                |row| row.get(0),
            )
            .expect("应能检查 P1 表");
        assert!(exists);
    }

    #[test]
    fn existing_v0_database_is_backed_up_with_wal_content() {
        let workspace = TestWorkspace::create("backup");
        let source = Connection::open(workspace.database_path()).expect("应能创建旧数据库");
        source
            .execute_batch(
                "PRAGMA journal_mode=WAL;
                 CREATE TABLE legacy_items (value TEXT NOT NULL);
                 INSERT INTO legacy_items VALUES ('保留内容');",
            )
            .expect("应能准备 WAL 旧库");

        let status = DatabaseService::new()
            .prepare_workspace_database(&workspace.root)
            .expect("旧数据库应备份并迁移");
        assert!(status.migrated);
        assert!(status.backup_created);

        let backups = backup_files(&workspace.backup_directory());
        assert_eq!(backups.len(), 1);
        let backup = Connection::open(&backups[0]).expect("备份应可作为 SQLite 打开");
        let backup_version: u32 = backup
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("应能读取备份版本");
        let value: String = backup
            .query_row("SELECT value FROM legacy_items", [], |row| row.get(0))
            .expect("备份应包含 WAL 中已提交的数据");
        assert_eq!(value, "保留内容");
        assert_eq!(backup_version, 0, "备份必须保持迁移前版本");
    }

    #[test]
    fn prepared_v1_database_is_idempotent() {
        let workspace = TestWorkspace::create("idempotent");
        let service = DatabaseService::new();
        service
            .prepare_workspace_database(&workspace.root)
            .expect("首次准备应成功");
        let status = service
            .prepare_workspace_database(&workspace.root)
            .expect("再次准备应成功");

        assert!(!status.migrated);
        assert!(!status.backup_created);
        assert!(backup_files(&workspace.backup_directory()).is_empty());
    }

    #[test]
    fn v1_database_is_backed_up_and_upgraded_to_v2() {
        let workspace = TestWorkspace::create("v1-upgrade");
        let connection = Connection::open(workspace.database_path()).expect("应能创建 v1 数据库");
        connection
            .execute_batch(super::INITIAL_MIGRATION)
            .expect("应能建立 v1 结构");
        connection
            .pragma_update(None, "user_version", 1_u32)
            .expect("应能设置 v1 版本");
        drop(connection);

        let status = DatabaseService::new()
            .prepare_workspace_database(&workspace.root)
            .expect("v1 数据库应能升级");
        assert_eq!(status.schema_version, 10);
        assert!(status.migrated);
        assert!(status.backup_created);

        let backup = Connection::open(backup_files(&workspace.backup_directory()).remove(0))
            .expect("迁移前备份应可打开");
        let backup_version: u32 = backup
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("应能读取备份版本");
        assert_eq!(backup_version, 1);
        let upgraded = Connection::open(workspace.database_path()).expect("应能打开升级后的数据库");
        let fts_exists: bool = upgraded
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='asset_search')",
                [],
                |row| row.get(0),
            )
            .expect("应能检查 FTS 表");
        assert!(fts_exists);
    }

    #[test]
    fn v8_upgrade_clears_image_duration_and_repairs_image_first_order() {
        let workspace = TestWorkspace::create("v8-media-order-repair");
        let connection = Connection::open(workspace.database_path()).expect("应能创建 v8 数据库");
        for sql in [
            super::INITIAL_MIGRATION,
            super::SEARCH_MIGRATION,
            super::AI_MIGRATION,
            super::P1_LIBRARY_MIGRATION,
            super::MEDIA_INTEGRITY_MIGRATION,
            super::ASSET_ORDER_MIGRATION,
            super::COMMON_TAXONOMY_MIGRATION,
        ] {
            connection.execute_batch(sql).expect("应能建立 v7 结构");
        }
        connection
            .execute(
                "INSERT INTO assets(media_type,path_kind,stored_path,file_name,duration_ms,created_at,updated_at)
                 VALUES('video','managed','media/videos/first.mp4','first.mp4',3000,1,1)",
                [],
            )
            .expect("应能插入历史视频");
        connection
            .execute(
                "INSERT INTO assets(media_type,path_kind,stored_path,file_name,duration_ms,created_at,updated_at)
                 VALUES('image','managed','media/images/late.png','late.png',3000,2,2)",
                [],
            )
            .expect("应能构造带时长且编号靠后的历史图片");
        connection
            .execute_batch(super::METADATA_PRESETS_MIGRATION)
            .expect("应能建立 v8 结构");
        connection
            .pragma_update(None, "user_version", 8_u32)
            .expect("应能设置 v8 版本");
        drop(connection);

        let status = DatabaseService::new()
            .prepare_workspace_database(&workspace.root)
            .expect("v8 数据库应能安全升级");
        assert_eq!(status.schema_version, 10);
        assert!(status.migrated);
        assert!(status.backup_created);

        let upgraded = Connection::open(workspace.database_path()).expect("应能打开升级库");
        let rows = upgraded
            .prepare(
                "SELECT a.media_type, a.duration_ms
                   FROM asset_display_order ao
                   JOIN assets a ON a.id=ao.asset_id
                  ORDER BY ao.position",
            )
            .expect("应能读取修复后顺序")
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?))
            })
            .expect("应能遍历修复后作品")
            .collect::<Result<Vec<_>, _>>()
            .expect("修复后作品应有效");
        assert_eq!(
            rows,
            vec![("image".to_owned(), None), ("video".to_owned(), Some(3000))]
        );
    }

    #[test]
    fn v9_upgrade_adds_canvas_schema_without_changing_existing_information() {
        let workspace = TestWorkspace::create("v9-canvas-preserves-data");
        prepare_v9_database(&workspace);

        let status = DatabaseService::new()
            .prepare_workspace_database(&workspace.root)
            .expect("v9 数据库应能安全升级至 v10");
        assert_eq!(status.schema_version, 10);
        assert!(status.migrated);
        assert!(status.backup_created);

        let upgraded = Connection::open(workspace.database_path()).expect("应能打开 v10 数据库");
        assert_v9_fixture_data_is_preserved(&upgraded);
        assert_eq!(
            upgraded
                .query_row("SELECT kind FROM projects WHERE id=41", [], |row| {
                    row.get::<_, String>(0)
                })
                .expect("旧项目应获得默认类型"),
            "simple"
        );
        assert_eq!(
            upgraded
                .query_row("SELECT count(*) FROM canvas_project_members", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("新增关系表应可读取"),
            0
        );

        let backups = backup_files(&workspace.backup_directory());
        assert_eq!(backups.len(), 1);
        let backup = Connection::open(&backups[0]).expect("迁移前备份应可打开");
        assert_eq!(
            backup
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .expect("应能读取备份版本"),
            9
        );
        assert_v9_fixture_data_is_preserved(&backup);
    }

    #[test]
    fn failed_v10_migration_keeps_v9_data_and_backup() {
        let workspace = TestWorkspace::create("v9-v10-rollback");
        prepare_v9_database(&workspace);

        let error = DatabaseService::new()
            .prepare_with_migrations(
                &workspace.root,
                super::INITIAL_MIGRATION,
                super::SEARCH_MIGRATION,
                super::AI_MIGRATION,
                super::P1_LIBRARY_MIGRATION,
                super::MEDIA_INTEGRITY_MIGRATION,
                super::ASSET_ORDER_MIGRATION,
                super::COMMON_TAXONOMY_MIGRATION,
                super::METADATA_PRESETS_MIGRATION,
                super::MEDIA_ORDER_REPAIR_MIGRATION,
                "INVALID V10 SQL;",
            )
            .expect_err("v10 migration 失败必须完整回滚");
        assert_eq!(error, DatabaseServiceError::MigrationFailed);
        assert_eq!(backup_files(&workspace.backup_directory()).len(), 1);

        let rolled_back =
            Connection::open(workspace.database_path()).expect("v9 数据库应保持可打开");
        assert_eq!(
            rolled_back
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .expect("应能读取回滚版本"),
            9
        );
        assert_v9_fixture_data_is_preserved(&rolled_back);
        let canvas_schema_exists: bool = rolled_back
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM sqlite_master WHERE name='canvas_project_members'
                 )",
                [],
                |row| row.get(0),
            )
            .expect("应能检查 v10 结构是否回滚");
        assert!(!canvas_schema_exists);
        let kind_column_exists: bool = rolled_back
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM pragma_table_info('projects') WHERE name='kind'
                 )",
                [],
                |row| row.get(0),
            )
            .expect("应能检查 kind 列是否回滚");
        assert!(!kind_column_exists);
    }

    #[test]
    fn failed_v1_to_v2_migration_keeps_v1_and_backup() {
        let workspace = TestWorkspace::create("v1-v2-rollback");
        let connection = Connection::open(workspace.database_path()).expect("应能创建 v1 数据库");
        connection
            .execute_batch(super::INITIAL_MIGRATION)
            .expect("应能建立 v1 结构");
        connection
            .pragma_update(None, "user_version", 1_u32)
            .expect("应能设置 v1 版本");
        drop(connection);

        let error = DatabaseService::new()
            .prepare_with_migrations(
                &workspace.root,
                super::INITIAL_MIGRATION,
                "INVALID SQL;",
                super::AI_MIGRATION,
                super::P1_LIBRARY_MIGRATION,
                super::MEDIA_INTEGRITY_MIGRATION,
                super::ASSET_ORDER_MIGRATION,
                super::COMMON_TAXONOMY_MIGRATION,
                super::METADATA_PRESETS_MIGRATION,
                super::MEDIA_ORDER_REPAIR_MIGRATION,
                super::CANVAS_PROJECTS_MIGRATION,
            )
            .expect_err("失败的 v2 migration 必须回滚");
        assert_eq!(error, DatabaseServiceError::MigrationFailed);
        assert_eq!(backup_files(&workspace.backup_directory()).len(), 1);
        let rolled_back =
            Connection::open(workspace.database_path()).expect("v1 数据库应保持可打开");
        let version: u32 = rolled_back
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("应能读取回滚后版本");
        assert_eq!(version, 1);
        let fts_exists: bool = rolled_back
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='asset_search')",
                [],
                |row| row.get(0),
            )
            .expect("应能检查回滚结构");
        assert!(!fts_exists);
    }

    #[test]
    fn failed_v4_step_rolls_back_the_entire_v1_upgrade_chain() {
        let workspace = TestWorkspace::create("v1-v4-atomic-rollback");
        let connection = Connection::open(workspace.database_path()).expect("应能创建 v1 数据库");
        connection
            .execute_batch(super::INITIAL_MIGRATION)
            .expect("应能建立 v1 结构");
        connection
            .pragma_update(None, "user_version", 1_u32)
            .expect("应能设置 v1 版本");
        drop(connection);

        let error = DatabaseService::new()
            .prepare_with_migrations(
                &workspace.root,
                super::INITIAL_MIGRATION,
                super::SEARCH_MIGRATION,
                super::AI_MIGRATION,
                "INVALID FINAL SQL;",
                super::MEDIA_INTEGRITY_MIGRATION,
                super::ASSET_ORDER_MIGRATION,
                super::COMMON_TAXONOMY_MIGRATION,
                super::METADATA_PRESETS_MIGRATION,
                super::MEDIA_ORDER_REPAIR_MIGRATION,
                super::CANVAS_PROJECTS_MIGRATION,
            )
            .expect_err("末级 v4 失败必须回滚整个升级链");
        assert_eq!(error, DatabaseServiceError::MigrationFailed);
        assert_eq!(backup_files(&workspace.backup_directory()).len(), 1);
        let rolled_back =
            Connection::open(workspace.database_path()).expect("原 v1 数据库应保持可打开");
        assert_eq!(
            rolled_back
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .expect("应能读取回滚版本"),
            1
        );
        let later_tables: i64 = rolled_back
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name IN ('asset_search','ai_providers','saved_filters')",
                [],
                |row| row.get(0),
            )
            .expect("应能检查后续结构");
        assert_eq!(later_tables, 0);
    }

    #[test]
    fn failed_v5_step_keeps_v4_database_and_backup() {
        let workspace = TestWorkspace::create("v4-v5-rollback");
        let connection = Connection::open(workspace.database_path()).expect("应能创建 v4 数据库");
        for sql in [
            super::INITIAL_MIGRATION,
            super::SEARCH_MIGRATION,
            super::AI_MIGRATION,
            super::P1_LIBRARY_MIGRATION,
        ] {
            connection.execute_batch(sql).expect("应能建立 v4 结构");
        }
        connection
            .pragma_update(None, "user_version", 4_u32)
            .expect("应能设置 v4 版本");
        drop(connection);

        let error = DatabaseService::new()
            .prepare_with_migrations(
                &workspace.root,
                super::INITIAL_MIGRATION,
                super::SEARCH_MIGRATION,
                super::AI_MIGRATION,
                super::P1_LIBRARY_MIGRATION,
                "INVALID V5 SQL;",
                super::ASSET_ORDER_MIGRATION,
                super::COMMON_TAXONOMY_MIGRATION,
                super::METADATA_PRESETS_MIGRATION,
                super::MEDIA_ORDER_REPAIR_MIGRATION,
                super::CANVAS_PROJECTS_MIGRATION,
            )
            .expect_err("v5 失败必须保留 v4 活库");
        assert_eq!(error, DatabaseServiceError::MigrationFailed);
        assert_eq!(backup_files(&workspace.backup_directory()).len(), 1);
        let rolled_back =
            Connection::open(workspace.database_path()).expect("原 v4 数据库应保持可打开");
        assert_eq!(
            rolled_back
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .expect("应能读取回滚版本"),
            4
        );
        let cover_table_exists: bool = rolled_back
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='asset_covers')",
                [],
                |row| row.get(0),
            )
            .expect("应能检查 v5 表未残留");
        assert!(!cover_table_exists);
    }

    #[test]
    fn damaged_v4_is_rejected_before_backup_or_v5_migration() {
        let workspace = TestWorkspace::create("damaged-v4");
        let connection = Connection::open(workspace.database_path()).expect("应能创建 v4 数据库");
        for sql in [
            super::INITIAL_MIGRATION,
            super::SEARCH_MIGRATION,
            super::AI_MIGRATION,
            super::P1_LIBRARY_MIGRATION,
        ] {
            connection.execute_batch(sql).expect("应能建立 v4 结构");
        }
        connection
            .execute_batch("PRAGMA user_version=4; DROP TABLE asset_search;")
            .expect("应能构造损坏 v4");
        drop(connection);

        assert_eq!(
            DatabaseService::new().prepare_workspace_database(&workspace.root),
            Err(DatabaseServiceError::SchemaInvalid)
        );
        assert!(backup_files(&workspace.backup_directory()).is_empty());
    }

    #[test]
    fn damaged_v5_missing_ai_trigger_is_rejected() {
        let workspace = TestWorkspace::create("damaged-v5");
        DatabaseService::new()
            .prepare_workspace_database(&workspace.root)
            .expect("应能创建 v5 数据库");
        let connection = Connection::open(workspace.database_path()).expect("应能打开 v5 数据库");
        connection
            .execute_batch("DROP TRIGGER ai_suggestions_reject_pending_after_asset_delete;")
            .expect("应能构造损坏 v5");
        drop(connection);

        assert_eq!(
            DatabaseService::new().prepare_workspace_database(&workspace.root),
            Err(DatabaseServiceError::SchemaInvalid)
        );
    }

    #[test]
    fn v2_database_is_backed_up_and_upgraded_to_v3() {
        let workspace = TestWorkspace::create("v2-upgrade");
        let connection = Connection::open(workspace.database_path()).expect("应能创建 v2 数据库");
        connection
            .execute_batch(super::INITIAL_MIGRATION)
            .expect("应能建立 v1 结构");
        connection
            .execute_batch(super::SEARCH_MIGRATION)
            .expect("应能建立 v2 结构");
        connection
            .pragma_update(None, "user_version", 2_u32)
            .expect("应能设置 v2 版本");
        drop(connection);

        let status = DatabaseService::new()
            .prepare_workspace_database(&workspace.root)
            .expect("v2 数据库应能升级");
        assert_eq!(status.schema_version, 10);
        assert!(status.migrated);
        assert!(status.backup_created);
        let backup = Connection::open(backup_files(&workspace.backup_directory()).remove(0))
            .expect("迁移前备份应可打开");
        let backup_version: u32 = backup
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("应能读取备份版本");
        assert_eq!(backup_version, 2);
        let upgraded = Connection::open(workspace.database_path()).expect("应能打开升级后的数据库");
        let providers_exists: bool = upgraded.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='ai_providers')", [], |row| row.get(0)).expect("应能检查 Provider 表");
        assert!(providers_exists);
    }

    #[test]
    fn failed_v2_to_v3_migration_keeps_v2_and_backup() {
        let workspace = TestWorkspace::create("v2-v3-rollback");
        let connection = Connection::open(workspace.database_path()).expect("应能创建 v2 数据库");
        connection
            .execute_batch(super::INITIAL_MIGRATION)
            .expect("应能建立 v1 结构");
        connection
            .execute_batch(super::SEARCH_MIGRATION)
            .expect("应能建立 v2 结构");
        connection
            .pragma_update(None, "user_version", 2_u32)
            .expect("应能设置 v2 版本");
        drop(connection);

        let error = DatabaseService::new()
            .prepare_with_migrations(
                &workspace.root,
                super::INITIAL_MIGRATION,
                super::SEARCH_MIGRATION,
                "INVALID SQL;",
                super::P1_LIBRARY_MIGRATION,
                super::MEDIA_INTEGRITY_MIGRATION,
                super::ASSET_ORDER_MIGRATION,
                super::COMMON_TAXONOMY_MIGRATION,
                super::METADATA_PRESETS_MIGRATION,
                super::MEDIA_ORDER_REPAIR_MIGRATION,
                super::CANVAS_PROJECTS_MIGRATION,
            )
            .expect_err("失败的 v3 migration 必须回滚");
        assert_eq!(error, DatabaseServiceError::MigrationFailed);
        assert_eq!(backup_files(&workspace.backup_directory()).len(), 1);
        let rolled_back =
            Connection::open(workspace.database_path()).expect("v2 数据库应保持可打开");
        let version: u32 = rolled_back
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("应能读取回滚后的版本");
        assert_eq!(version, 2);
        let providers_exists: bool = rolled_back
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='ai_providers')",
                [],
                |row| row.get(0),
            )
            .expect("应能检查回滚结构");
        assert!(!providers_exists);
    }

    #[test]
    fn future_schema_version_is_rejected_without_backup() {
        let workspace = TestWorkspace::create("future");
        let connection = Connection::open(workspace.database_path()).expect("应能创建未来数据库");
        connection
            .pragma_update(None, "user_version", 11_u32)
            .expect("应能设置未来版本");
        drop(connection);

        let error = DatabaseService::new()
            .prepare_workspace_database(&workspace.root)
            .expect_err("未来版本必须拒绝");
        assert_eq!(error, DatabaseServiceError::VersionUnsupported);
        assert!(backup_files(&workspace.backup_directory()).is_empty());
    }

    #[test]
    fn failed_migration_rolls_back_schema_and_version_but_keeps_backup() {
        let workspace = TestWorkspace::create("rollback");
        let connection = Connection::open(workspace.database_path()).expect("应能创建旧数据库");
        connection
            .execute_batch("CREATE TABLE legacy_items (id INTEGER PRIMARY KEY);")
            .expect("应能准备旧表");
        drop(connection);

        let error = DatabaseService::new()
            .prepare_with_migration(
                &workspace.root,
                &format!("{}\nINVALID SQL;", super::INITIAL_MIGRATION),
            )
            .expect_err("错误 migration 必须失败");
        assert_eq!(error, DatabaseServiceError::MigrationFailed);
        assert_eq!(backup_files(&workspace.backup_directory()).len(), 1);

        let connection = Connection::open(workspace.database_path()).expect("原数据库应仍可打开");
        let version: u32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("应能读取回滚后的版本");
        let project_table_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'projects')",
                [],
                |row| row.get(0),
            )
            .expect("应能检查部分表");
        assert_eq!(version, 0);
        assert!(!project_table_exists, "v1 表不得在失败后残留");
        let legacy_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'legacy_items')",
                [],
                |row| row.get(0),
            )
            .expect("应能检查旧表");
        assert!(legacy_exists, "旧库结构必须保留");
    }

    #[test]
    fn incomplete_but_valid_migration_is_validated_before_commit() {
        let workspace = TestWorkspace::create("incomplete-migration");
        let connection = Connection::open(workspace.database_path()).expect("应能创建旧数据库");
        connection
            .execute_batch("CREATE TABLE legacy_items (id INTEGER PRIMARY KEY);")
            .expect("应能准备旧表");
        drop(connection);

        let error = DatabaseService::new()
            .prepare_with_migration(
                &workspace.root,
                "CREATE TABLE projects (id INTEGER PRIMARY KEY);",
            )
            .expect_err("缺少关键表的有效 migration 必须回滚");
        assert_eq!(error, DatabaseServiceError::SchemaInvalid);
        assert_eq!(backup_files(&workspace.backup_directory()).len(), 1);

        let connection = Connection::open(workspace.database_path()).expect("原数据库应仍可打开");
        let version: u32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("应能读取回滚后的版本");
        let project_table_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'projects')",
                [],
                |row| row.get(0),
            )
            .expect("应能检查回滚结果");
        assert_eq!(version, 0);
        assert!(!project_table_exists, "契约失败必须回滚已执行的有效 SQL");
    }

    #[cfg(windows)]
    #[test]
    fn reserved_backup_cannot_be_replaced_while_guard_is_open() {
        let workspace = TestWorkspace::create("backup-replacement");
        let reserved =
            super::reserve_backup_file(&workspace.backup_directory(), 1).expect("应能预留备份文件");

        let remove_result = fs::remove_file(&reserved.path);
        assert!(remove_result.is_err(), "预留句柄持有期间不得删除或替换备份");
        let path = reserved.path.clone();
        drop(reserved);
        fs::remove_file(path).expect("释放句柄后应能清理预留文件");
    }

    #[test]
    fn v1_with_missing_required_table_is_rejected() {
        let workspace = TestWorkspace::create("damaged-v1");
        let connection = Connection::open(workspace.database_path()).expect("应能创建损坏数据库");
        connection
            .pragma_update(None, "user_version", 1_u32)
            .expect("应能设置 v1 版本");
        drop(connection);

        let error = DatabaseService::new()
            .prepare_workspace_database(&workspace.root)
            .expect_err("缺少必需表的 v1 必须拒绝");
        assert_eq!(error, DatabaseServiceError::SchemaInvalid);
        assert!(backup_files(&workspace.backup_directory()).is_empty());
    }

    #[test]
    fn v2_with_missing_fts_table_is_rejected() {
        let workspace = TestWorkspace::create("damaged-v2");
        let connection = Connection::open(workspace.database_path()).expect("应能创建损坏数据库");
        connection
            .execute_batch(super::INITIAL_MIGRATION)
            .expect("应能建立不完整 v2 的基础表");
        connection
            .pragma_update(None, "user_version", 2_u32)
            .expect("应能设置 v2 版本");
        drop(connection);

        let error = DatabaseService::new()
            .prepare_workspace_database(&workspace.root)
            .expect_err("缺少 FTS 表的 v2 必须拒绝");
        assert_eq!(error, DatabaseServiceError::SchemaInvalid);
        assert!(backup_files(&workspace.backup_directory()).is_empty());
    }

    #[test]
    fn foreign_keys_and_schema_contract_are_enforced() {
        let workspace = TestWorkspace::create("contract");
        DatabaseService::new()
            .prepare_workspace_database(&workspace.root)
            .expect("数据库应准备成功");

        let connection = super::DatabaseRepository::open(&workspace.database_path())
            .expect("应能按运行配置打开数据库");
        let expected_tables = [
            "projects",
            "assets",
            "prompts",
            "models",
            "platforms",
            "dimensions",
            "categories",
            "project_categories",
            "asset_categories",
            "tags",
            "project_tags",
            "asset_tags",
            "ai_suggestions",
            "ai_providers",
            "trash_entries",
            "app_settings",
        ];
        for table in expected_tables {
            let exists: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                    [table],
                    |row| row.get(0),
                )
                .expect("应能检查关键表");
            assert!(exists, "缺少关键表 {table}");
        }
        let result = connection.execute(
            "INSERT INTO assets (
                project_id, media_type, path_kind, stored_path, file_name, created_at, updated_at
             ) VALUES (999, 'image', 'managed', 'media/images/a.png', 'a.png', 1, 1)",
            [],
        );
        assert!(result.is_err(), "不存在的项目外键必须被拒绝");

        let invalid_media_type = connection.execute(
            "INSERT INTO assets (
                media_type, path_kind, stored_path, file_name, created_at, updated_at
             ) VALUES ('audio', 'managed', 'media/audio/a.mp3', 'a.mp3', 1, 1)",
            [],
        );
        assert!(invalid_media_type.is_err(), "非法媒体类型必须被 CHECK 拒绝");
    }

    #[test]
    #[ignore = "发布构建数据库轻量打开基准，仅手动运行"]
    fn benchmark_prepared_v1_database_open_by_asset_count() {
        for asset_count in [100_u32, 1_000, 10_000] {
            let workspace = TestWorkspace::create(&format!("open-benchmark-{asset_count}"));
            let service = DatabaseService::new();
            service
                .prepare_workspace_database(&workspace.root)
                .expect("应能创建 v1 基准数据库");

            let mut connection = Connection::open(workspace.database_path())
                .expect("应能打开 v1 基准数据库写入数据");
            let transaction = connection.transaction().expect("应能开启基准数据事务");
            {
                let mut statement = transaction
                    .prepare(
                        "INSERT INTO assets (
                            media_type, path_kind, stored_path, file_name, created_at, updated_at
                         ) VALUES ('image', 'managed', ?1, ?2, 1, 1)",
                    )
                    .expect("应能准备基准数据插入语句");
                for index in 0..asset_count {
                    statement
                        .execute((
                            format!("media/images/benchmark-{index}.png"),
                            format!("benchmark-{index}.png"),
                        ))
                        .expect("应能插入最小 asset 基准记录");
                }
            }
            transaction.commit().expect("应能提交基准数据事务");
            drop(connection);

            // 数据准备不计时；这里仅测量已有 v1 数据库的轻量再次打开与结构校验。
            let started_at = Instant::now();
            let status = service
                .prepare_workspace_database(&workspace.root)
                .expect("已有 v1 基准数据库应能再次打开");
            let elapsed = started_at.elapsed();

            assert!(!status.migrated);
            assert!(!status.backup_created);
            println!(
                "database_open_benchmark asset_count={asset_count} elapsed_us={} elapsed_ms={:.3}",
                elapsed.as_micros(),
                elapsed.as_secs_f64() * 1_000.0
            );
        }
    }
}
