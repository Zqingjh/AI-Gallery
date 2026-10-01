use std::{
    path::Path,
    sync::{Mutex, MutexGuard, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    domain::AccessMode,
    repositories::{AccessModeRepository, AccessModeRepositoryError, DatabaseRepository},
    services::{DatabaseService, WorkspaceService, WorkspaceServiceError},
};

const DATABASE_RELATIVE_PATH: &str = "data/library.sqlite3";
static WORKSPACE_WRITE_COORDINATOR: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AccessModeServiceError {
    Workspace(WorkspaceServiceError),
    DatabaseUnavailable,
    InvalidSetting,
    ClockUnavailable,
    ReadOnly,
}

impl From<WorkspaceServiceError> for AccessModeServiceError {
    fn from(error: WorkspaceServiceError) -> Self {
        Self::Workspace(error)
    }
}

/// 工作区写操作可复用的最小权限守卫。
pub(crate) struct WriteAccessGuard;

/// 在实际写入期间持有独占租约；模式切换使用同一把锁，从而保证切换完成后不再有
/// 已获许可的写入继续执行。当前按进程协调，优先确保安全与实现边界明确。
pub(crate) struct WriteAccessLease {
    _guard: MutexGuard<'static, ()>,
}

impl WriteAccessGuard {
    pub(crate) fn ensure_writable(root: &Path) -> Result<(), AccessModeServiceError> {
        WorkspaceService::new().open_workspace(root)?;
        if !root
            .join(DATABASE_RELATIVE_PATH)
            .try_exists()
            .map_err(|_| AccessModeServiceError::DatabaseUnavailable)?
        {
            // 首次初始化前不存在可持久化的只读设置，允许调用方建立新数据库。
            return Ok(());
        }
        if AccessModeService::new().get_mode(root)? == AccessMode::ReadOnly {
            return Err(AccessModeServiceError::ReadOnly);
        }
        Ok(())
    }

    pub(crate) fn acquire_writable(
        root: &Path,
    ) -> Result<WriteAccessLease, AccessModeServiceError> {
        let guard = write_coordinator()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Self::ensure_writable(root)?;
        Ok(WriteAccessLease { _guard: guard })
    }
}

pub(crate) struct AccessModeService;

impl AccessModeService {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn get_mode(&self, root: &Path) -> Result<AccessMode, AccessModeServiceError> {
        self.open(root).and_then(|connection| {
            if DatabaseRepository::schema_version(&connection)
                .map_err(|_| AccessModeServiceError::DatabaseUnavailable)?
                == 0
            {
                // v0 还没有可持久化的访问模式；只允许数据库准备流程继续初始化。
                return Ok(AccessMode::ReadWrite);
            }
            AccessModeRepository::get(&connection).map_err(map_repository_error)
        })
    }

    pub(crate) fn set_mode(
        &self,
        root: &Path,
        mode: AccessMode,
    ) -> Result<AccessMode, AccessModeServiceError> {
        // 模式切换会等待所有文件复制和数据库写事务的租约结束，再持久化只读模式。
        let _guard = write_coordinator()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut connection = self.open(root)?;
        AccessModeRepository::set(&mut connection, mode, now()?).map_err(map_repository_error)?;
        Ok(mode)
    }

    fn open(&self, root: &Path) -> Result<rusqlite::Connection, AccessModeServiceError> {
        DatabaseService::new()
            .open_access_mode_database(root)
            .map_err(|_| AccessModeServiceError::DatabaseUnavailable)
    }
}

fn write_coordinator() -> &'static Mutex<()> {
    WORKSPACE_WRITE_COORDINATOR.get_or_init(|| Mutex::new(()))
}

fn now() -> Result<i64, AccessModeServiceError> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AccessModeServiceError::ClockUnavailable)?
            .as_millis(),
    )
    .map_err(|_| AccessModeServiceError::ClockUnavailable)
}

fn map_repository_error(error: AccessModeRepositoryError) -> AccessModeServiceError {
    match error {
        AccessModeRepositoryError::DatabaseFailed => AccessModeServiceError::DatabaseUnavailable,
        AccessModeRepositoryError::InvalidData => AccessModeServiceError::InvalidSetting,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        sync::mpsc,
        thread,
        time::Duration,
    };

    use super::{AccessModeService, AccessModeServiceError, WriteAccessGuard};
    use crate::{
        domain::AccessMode,
        services::{DatabaseService, WorkspaceService},
    };

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestWorkspace(PathBuf);

    impl TestWorkspace {
        fn new() -> Self {
            let directory = std::env::temp_dir().join(format!(
                "ai-gallery-access-mode-{}-{}",
                std::process::id(),
                TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&directory);
            fs::create_dir(&directory).expect("应能创建测试父目录");
            let root = directory.join("workspace");
            WorkspaceService::new()
                .create_workspace(&root)
                .expect("应能创建测试工作区");
            DatabaseService::new()
                .prepare_workspace_database(&root)
                .expect("应能准备测试数据库");
            Self(directory)
        }

        fn root(&self) -> PathBuf {
            self.0.join("workspace")
        }
    }

    impl Drop for TestWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn persists_mode_and_write_guard_rejects_read_only_workspace() {
        let workspace = TestWorkspace::new();
        let root = workspace.root();
        let service = AccessModeService::new();

        assert_eq!(
            service.get_mode(&root).expect("默认模式应可读取"),
            AccessMode::ReadWrite
        );
        service
            .set_mode(&root, AccessMode::ReadOnly)
            .expect("应能设为只读");
        assert_eq!(
            service.get_mode(&root).expect("已保存模式应可读取"),
            AccessMode::ReadOnly
        );
        assert_eq!(
            WriteAccessGuard::ensure_writable(&root),
            Err(AccessModeServiceError::ReadOnly)
        );

        service
            .set_mode(&root, AccessMode::ReadWrite)
            .expect("应能恢复读写");
        WriteAccessGuard::ensure_writable(&root).expect("读写模式应允许写入");
    }

    #[test]
    fn switching_to_read_only_waits_for_an_active_write_lease() {
        let workspace = TestWorkspace::new();
        let root = workspace.root();
        let lease = WriteAccessGuard::acquire_writable(&root).expect("应能获得写入租约");
        let (finished_sender, finished_receiver) = mpsc::channel();
        let switch_root = root.clone();
        let handle = thread::spawn(move || {
            let result = AccessModeService::new().set_mode(&switch_root, AccessMode::ReadOnly);
            finished_sender.send(result).expect("应能回传切换结果");
        });

        assert!(
            finished_receiver
                .recv_timeout(Duration::from_millis(50))
                .is_err(),
            "未释放的写入租约期间不得完成只读切换"
        );
        drop(lease);
        assert_eq!(
            finished_receiver
                .recv_timeout(Duration::from_secs(1))
                .expect("释放租约后应能完成切换"),
            Ok(AccessMode::ReadOnly)
        );
        handle.join().expect("切换线程不应恐慌");
        assert_eq!(
            AccessModeService::new().get_mode(&root),
            Ok(AccessMode::ReadOnly)
        );
    }
}
