use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{
    commands::CommandErrorDto,
    domain::DatabaseStatus,
    repositories::DatabaseRepository,
    services::{AccessModeServiceError, DatabaseService, DatabaseServiceError, WriteAccessGuard},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PrepareDatabaseRequestDto {
    root_path: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DatabaseStatusDto {
    schema_version: u32,
    migrated: bool,
    backup_created: bool,
}

impl From<DatabaseStatus> for DatabaseStatusDto {
    fn from(status: DatabaseStatus) -> Self {
        Self {
            schema_version: status.schema_version,
            migrated: status.migrated,
            backup_created: status.backup_created,
        }
    }
}

impl From<DatabaseServiceError> for CommandErrorDto {
    fn from(error: DatabaseServiceError) -> Self {
        match error {
            DatabaseServiceError::Workspace(workspace_error) => Self::from(workspace_error),
            DatabaseServiceError::UnsafePath => {
                Self::new("DATABASE_PATH_UNSAFE", "数据库位置不安全，无法打开工作区。")
            }
            DatabaseServiceError::OpenFailed => {
                Self::new("DATABASE_OPEN_FAILED", "无法打开工作区数据库。")
            }
            DatabaseServiceError::BackupFailed => Self::new(
                "DATABASE_BACKUP_FAILED",
                "数据库迁移前备份失败，未执行迁移。",
            ),
            DatabaseServiceError::MigrationFailed => Self::new(
                "DATABASE_MIGRATION_FAILED",
                "数据库升级失败，已撤销本次结构变更。",
            ),
            DatabaseServiceError::VersionUnsupported => {
                Self::new("DATABASE_VERSION_UNSUPPORTED", "当前版本无法打开此数据库。")
            }
            DatabaseServiceError::SchemaInvalid => {
                Self::new("DATABASE_SCHEMA_INVALID", "数据库结构不完整或已损坏。")
            }
        }
    }
}

#[tauri::command]
pub(crate) fn prepare_workspace_database(
    request: PrepareDatabaseRequestDto,
) -> Result<DatabaseStatusDto, CommandErrorDto> {
    let root = Path::new(&request.root_path);
    let database_exists = root
        .join("data/library.sqlite3")
        .try_exists()
        .map_err(|_| CommandErrorDto::new("DATABASE_OPEN_FAILED", "无法打开工作区数据库。"))?;
    if database_exists
        && matches!(
            WriteAccessGuard::ensure_writable(root),
            Err(AccessModeServiceError::ReadOnly)
        )
    {
        // 已持久化为只读的当前版本工作区需要允许重新连接；这里只做无写入校验，
        // 版本不匹配仍不能借由只读路径绕过迁移或结构检查。
        let connection = DatabaseRepository::open(&root.join("data/library.sqlite3"))
            .map_err(|_| CommandErrorDto::new("DATABASE_OPEN_FAILED", "无法打开工作区数据库。"))?;
        let version = DatabaseRepository::schema_version(&connection)
            .map_err(|_| CommandErrorDto::new("DATABASE_OPEN_FAILED", "无法读取数据库版本。"))?;
        if version != crate::domain::DATABASE_SCHEMA_VERSION {
            return Err(CommandErrorDto::new(
                "WORKSPACE_READ_ONLY",
                "工作区为只读且数据库需要升级；请先切换为读写模式。",
            ));
        }
        DatabaseRepository::validate_v10(&connection).map_err(|_| {
            CommandErrorDto::new("DATABASE_SCHEMA_INVALID", "数据库结构不完整或已损坏。")
        })?;
        return Ok(DatabaseStatusDto {
            schema_version: crate::domain::DATABASE_SCHEMA_VERSION,
            migrated: false,
            backup_created: false,
        });
    }
    // 新建库和迁移会产生真实写入，必须持有租约直到 prepare 完成；这样只读切换
    // 返回后，不会再有已通过检查但尚未执行的数据库结构写入。
    let _lease = WriteAccessGuard::acquire_writable(root).map_err(|error| match error {
        AccessModeServiceError::ReadOnly => CommandErrorDto::new(
            "WORKSPACE_READ_ONLY",
            "工作区正处于只读展示模式，无法修改内容。",
        ),
        _ => CommandErrorDto::new("DATABASE_OPEN_FAILED", "无法打开工作区数据库。"),
    })?;
    DatabaseService::new()
        .prepare_workspace_database(root)
        .map(DatabaseStatusDto::from)
        .map_err(CommandErrorDto::from)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::{
            atomic::{AtomicU64, Ordering},
            mpsc,
        },
        thread,
        time::Duration,
    };

    use super::{DatabaseStatusDto, PrepareDatabaseRequestDto, prepare_workspace_database};
    use crate::{
        domain::{AccessMode, DatabaseStatus},
        repositories::DatabaseRepository,
        services::{AccessModeService, WorkspaceService, WriteAccessGuard},
    };

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "ai-gallery-database-command-{}-{}",
                std::process::id(),
                TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir(&path).expect("应能创建测试目录");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn database_status_dto_does_not_expose_paths() {
        let dto = DatabaseStatusDto::from(DatabaseStatus {
            schema_version: 1,
            migrated: true,
            backup_created: true,
        });
        let json = serde_json::to_value(dto).expect("状态应可序列化");

        assert_eq!(json["schemaVersion"], 1);
        assert_eq!(json["migrated"], true);
        assert_eq!(json["backupCreated"], true);
        assert!(json.get("path").is_none());
        assert!(json.get("backupPath").is_none());
    }

    fn downgrade_to_v3(root: &std::path::Path) {
        let database_path = root.join("data/library.sqlite3");
        let connection = DatabaseRepository::open(&database_path).expect("应能打开测试数据库");
        let access_mode: Option<String> = connection
            .query_row(
                "SELECT value_json FROM app_settings WHERE key='workspace.accessMode'",
                [],
                |row| row.get(0),
            )
            .ok();
        drop(connection);

        fs::remove_file(&database_path).expect("应能移除临时当前版本测试库");
        let connection = DatabaseRepository::open(&database_path).expect("应能重建 v3 测试数据库");
        for migration in [
            include_str!("../migrations/0001_initial.sql"),
            include_str!("../migrations/0002_search.sql"),
            include_str!("../migrations/0003_ai.sql"),
        ] {
            connection
                .execute_batch(migration)
                .expect("应能应用历史 migration 构造 v3 测试库");
        }
        if let Some(value_json) = access_mode {
            connection
                .execute(
                    "INSERT INTO app_settings(key,value_json,updated_at)
                     VALUES('workspace.accessMode',?1,1)",
                    [value_json],
                )
                .expect("应能保留测试工作区访问模式");
        }
        connection
            .pragma_update(None, "user_version", 3_u32)
            .expect("应能设置 v3 测试版本");
        DatabaseRepository::validate_v3(&connection).expect("降级 fixture 必须是合法 v3");
    }

    #[test]
    fn persisted_read_only_current_workspace_can_reconnect_without_migration() {
        let parent = TestDirectory::new();
        let root = parent.0.join("workspace");
        WorkspaceService::new()
            .create_workspace(&root)
            .expect("应能创建工作区");
        prepare_workspace_database(PrepareDatabaseRequestDto {
            root_path: root.to_string_lossy().into_owned(),
        })
        .expect("应能初始化当前数据库");
        AccessModeService::new()
            .set_mode(&root, AccessMode::ReadOnly)
            .expect("应能持久化只读模式");

        let status = prepare_workspace_database(PrepareDatabaseRequestDto {
            root_path: root.to_string_lossy().into_owned(),
        })
        .expect("只读当前工作区重新连接不得要求写入或迁移");

        assert_eq!(status.schema_version, 10);
        assert!(!status.migrated);
        assert!(!status.backup_created);
    }

    #[test]
    fn read_write_v3_workspace_is_backed_up_and_migrated_through_command() {
        let parent = TestDirectory::new();
        let root = parent.0.join("workspace");
        WorkspaceService::new()
            .create_workspace(&root)
            .expect("应能创建工作区");
        prepare_workspace_database(PrepareDatabaseRequestDto {
            root_path: root.to_string_lossy().into_owned(),
        })
        .expect("应能初始化当前数据库");
        downgrade_to_v3(&root);

        let status = prepare_workspace_database(PrepareDatabaseRequestDto {
            root_path: root.to_string_lossy().into_owned(),
        })
        .expect("读写 v3 应能通过真实命令链升级");

        assert_eq!(status.schema_version, 10);
        assert!(status.migrated);
        assert!(status.backup_created);
    }

    #[test]
    fn read_only_v3_workspace_is_not_migrated_until_mode_changes() {
        let parent = TestDirectory::new();
        let root = parent.0.join("workspace");
        WorkspaceService::new()
            .create_workspace(&root)
            .expect("应能创建工作区");
        prepare_workspace_database(PrepareDatabaseRequestDto {
            root_path: root.to_string_lossy().into_owned(),
        })
        .expect("应能初始化当前数据库");
        AccessModeService::new()
            .set_mode(&root, AccessMode::ReadOnly)
            .expect("应能持久化只读模式");
        downgrade_to_v3(&root);

        let error = prepare_workspace_database(PrepareDatabaseRequestDto {
            root_path: root.to_string_lossy().into_owned(),
        })
        .expect_err("只读旧库不得隐式迁移");
        assert_eq!(error.code, "WORKSPACE_READ_ONLY");
        let connection =
            DatabaseRepository::open(&root.join("data/library.sqlite3")).expect("应能重新打开旧库");
        assert_eq!(
            DatabaseRepository::schema_version(&connection).expect("应能读取版本"),
            3
        );
        drop(connection);

        AccessModeService::new()
            .set_mode(&root, AccessMode::ReadWrite)
            .expect("旧库仍应允许显式恢复读写模式");
        let status = prepare_workspace_database(PrepareDatabaseRequestDto {
            root_path: root.to_string_lossy().into_owned(),
        })
        .expect("切回读写后应能升级");
        assert!(status.migrated);
    }

    #[test]
    fn prepare_waits_for_active_write_lease_before_creating_database() {
        let parent = TestDirectory::new();
        let root = parent.0.join("workspace");
        WorkspaceService::new()
            .create_workspace(&root)
            .expect("应能创建工作区");
        let lease = WriteAccessGuard::acquire_writable(&root).expect("应能占用写入租约");
        let (sender, receiver) = mpsc::channel();
        let request_root = root.to_string_lossy().into_owned();
        let handle = thread::spawn(move || {
            sender
                .send(prepare_workspace_database(PrepareDatabaseRequestDto {
                    root_path: request_root,
                }))
                .expect("应能回传建库结果");
        });

        assert!(
            receiver.recv_timeout(Duration::from_millis(50)).is_err(),
            "数据库建库/迁移必须等待已有写入租约完成"
        );
        drop(lease);
        let status = receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("释放租约后应能完成数据库准备")
            .expect("数据库准备应成功");
        handle.join().expect("数据库准备线程不应恐慌");
        assert_eq!(status.schema_version, 10);
    }
}
