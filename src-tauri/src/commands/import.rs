use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
};

use serde::{Deserialize, Serialize};
use tauri::State;

#[cfg(test)]
use std::sync::atomic::AtomicUsize;

use crate::{
    commands::CommandErrorDto,
    domain::{
        AssetMetadata, CreateAsset, ImportCancellationToken, ImportMode, MediaKind, MediaType,
        PathKind, PromptText,
    },
    services::{
        AccessModeServiceError, DefaultVideoCoverRequest, ImportService, ImportServiceError,
        LibraryService, MediaIntegrityService, PrepareImportBatchRequest, WriteAccessGuard,
    },
};

const MAX_TRACKED_IMPORTS: usize = 32;
const MAX_SCANNED_DIRECTORY_ENTRIES: usize = 10_000;
const MAX_IMPORT_ITEMS: usize = 100;

#[cfg(test)]
static ACTIVE_IMPORT_JOBS: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
static MAX_ACTIVE_IMPORT_JOBS: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ImportModeDto {
    Copy,
    Reference,
}

impl From<ImportModeDto> for ImportMode {
    fn from(value: ImportModeDto) -> Self {
        match value {
            ImportModeDto::Copy => Self::Copy,
            ImportModeDto::Reference => Self::Reference,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StartImportRequestDto {
    root_path: String,
    source_paths: Vec<String>,
    mode: ImportModeDto,
    project_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ImportTaskRequestDto {
    task_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum ImportTaskStateDto {
    Queued,
    Running,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportTaskDto {
    id: String,
    state: ImportTaskStateDto,
    completed: usize,
    total: usize,
    current_file_name: Option<String>,
    message: Option<&'static str>,
}

struct ImportJob {
    task: ImportTaskDto,
    cancellation: ImportCancellationToken,
}

struct QueuedImport {
    task_id: String,
    request: StartImportRequestDto,
    cancellation: ImportCancellationToken,
    bundled_ffmpeg_path: Option<PathBuf>,
}

#[derive(Clone)]
pub(crate) struct ImportCommandState {
    next_id: Arc<AtomicU64>,
    jobs: Arc<Mutex<HashMap<String, ImportJob>>>,
    queue: Arc<Mutex<VecDeque<QueuedImport>>>,
    worker_running: Arc<AtomicBool>,
    bundled_ffmpeg_path: Option<PathBuf>,
}

impl Default for ImportCommandState {
    fn default() -> Self {
        Self {
            next_id: Arc::new(AtomicU64::new(1)),
            jobs: Arc::new(Mutex::new(HashMap::new())),
            queue: Arc::new(Mutex::new(VecDeque::new())),
            worker_running: Arc::new(AtomicBool::new(false)),
            bundled_ffmpeg_path: None,
        }
    }
}

impl ImportCommandState {
    pub(crate) fn with_bundled_ffmpeg(path: PathBuf) -> Self {
        Self {
            bundled_ffmpeg_path: Some(path),
            ..Self::default()
        }
    }

    pub(crate) fn bundled_ffmpeg_path(&self) -> Option<&Path> {
        self.bundled_ffmpeg_path.as_deref()
    }
}

impl From<ImportServiceError> for CommandErrorDto {
    fn from(error: ImportServiceError) -> Self {
        match error {
            ImportServiceError::EmptyBatch
            | ImportServiceError::BatchTooLarge
            | ImportServiceError::InvalidSource
            | ImportServiceError::UnsupportedFormat => {
                Self::new("IMPORT_INVALID_INPUT", "请选择受支持的图片或 MP4 文件。")
            }
            ImportServiceError::Cancelled => Self::new("IMPORT_CANCELLED", "导入已取消。"),
            ImportServiceError::ReadOnly => Self::new(
                "WORKSPACE_READ_ONLY",
                "工作区正处于只读展示模式，无法修改内容。",
            ),
            ImportServiceError::CompensationFailed => Self::new(
                "IMPORT_COMPENSATION_FAILED",
                "导入失败，且部分新复制文件未能自动清理。",
            ),
            ImportServiceError::InvalidWorkspace
            | ImportServiceError::SourceUnavailable
            | ImportServiceError::SourceChanged
            | ImportServiceError::ReadFailed
            | ImportServiceError::CopyFailed => Self::new("IMPORT_FAILED", "无法完成媒体导入。"),
        }
    }
}

#[tauri::command]
pub(crate) fn start_media_import(
    state: State<'_, ImportCommandState>,
    request: StartImportRequestDto,
) -> Result<ImportTaskDto, CommandErrorDto> {
    if request.root_path.trim().is_empty()
        || request.source_paths.is_empty()
        || request.source_paths.len() > 100
        || request.project_id.is_some_and(|id| id <= 0)
    {
        return Err(CommandErrorDto::new(
            "IMPORT_INVALID_INPUT",
            "请选择受支持的图片或 MP4 文件。",
        ));
    }
    ensure_workspace_writable(Path::new(&request.root_path))?;

    let task_id = state.next_id.fetch_add(1, Ordering::Relaxed).to_string();
    let cancellation = ImportCancellationToken::default();
    let task = ImportTaskDto {
        id: task_id.clone(),
        state: ImportTaskStateDto::Queued,
        completed: 0,
        total: request.source_paths.len(),
        current_file_name: None,
        message: None,
    };
    {
        let mut jobs = state
            .jobs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        prune_finished_jobs(&mut jobs);
        if jobs.len() >= MAX_TRACKED_IMPORTS {
            return Err(CommandErrorDto::new(
                "IMPORT_QUEUE_FULL",
                "当前导入任务过多，请稍后重试。",
            ));
        }
        jobs.insert(
            task_id.clone(),
            ImportJob {
                task: task.clone(),
                cancellation: cancellation.clone(),
            },
        );
    }

    state
        .queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push_back(QueuedImport {
            task_id,
            request,
            cancellation,
            bundled_ffmpeg_path: state.bundled_ffmpeg_path.clone(),
        });
    start_worker_if_needed(&state);
    Ok(task)
}

#[tauri::command]
pub(crate) fn get_media_import_task(
    state: State<'_, ImportCommandState>,
    request: ImportTaskRequestDto,
) -> Result<ImportTaskDto, CommandErrorDto> {
    state
        .jobs
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&request.task_id)
        .map(|job| job.task.clone())
        .ok_or_else(|| CommandErrorDto::new("IMPORT_TASK_NOT_FOUND", "未找到导入任务。"))
}

#[tauri::command]
pub(crate) fn cancel_media_import(
    state: State<'_, ImportCommandState>,
    request: ImportTaskRequestDto,
) -> Result<(), CommandErrorDto> {
    let jobs = state
        .jobs
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let job = jobs
        .get(&request.task_id)
        .ok_or_else(|| CommandErrorDto::new("IMPORT_TASK_NOT_FOUND", "未找到导入任务。"))?;
    job.cancellation.cancel();
    Ok(())
}

fn run_import_job(
    jobs: Arc<Mutex<HashMap<String, ImportJob>>>,
    task_id: String,
    request: StartImportRequestDto,
    cancellation: ImportCancellationToken,
    bundled_ffmpeg_path: Option<PathBuf>,
) {
    #[cfg(test)]
    let _active_job = ActiveImportJobGuard::enter();
    update_task(&jobs, &task_id, |task| {
        task.state = ImportTaskStateDto::Running;
    });
    let StartImportRequestDto {
        root_path,
        source_paths,
        mode,
        project_id,
    } = request;
    let root = PathBuf::from(&root_path);
    if ensure_workspace_writable(&root).is_err() {
        finish_task(
            &jobs,
            &task_id,
            ImportTaskStateDto::Failed,
            "工作区已切换为只读展示模式，未开始导入。",
        );
        return;
    }
    let import_service = ImportService::new();
    let source_paths = match expand_import_sources(source_paths, &cancellation) {
        Ok(paths) => paths,
        Err(_) => {
            finish_task(
                &jobs,
                &task_id,
                if cancellation.is_cancelled() {
                    ImportTaskStateDto::Cancelled
                } else {
                    ImportTaskStateDto::Failed
                },
                "无法读取所选文件夹，或其中媒体数量超过限制。",
            );
            return;
        }
    };
    update_task(&jobs, &task_id, |task| task.total = source_paths.len());
    let progress_jobs = Arc::clone(&jobs);
    let progress_task_id = task_id.clone();
    let result = import_service.prepare_batch_with_progress(
        PrepareImportBatchRequest {
            workspace_root: root.clone(),
            import_mode: mode.into(),
            source_paths,
        },
        &cancellation,
        &move |completed, file_name| {
            update_task(&progress_jobs, &progress_task_id, |task| {
                task.completed = completed;
                task.current_file_name = Some(file_name.to_owned());
            });
        },
    );

    let batch = match result {
        Ok(batch) => batch,
        Err(ImportServiceError::Cancelled) => {
            finish_task(
                &jobs,
                &task_id,
                ImportTaskStateDto::Cancelled,
                "导入已取消。",
            );
            return;
        }
        Err(_) => {
            finish_task(
                &jobs,
                &task_id,
                ImportTaskStateDto::Failed,
                "无法读取或复制媒体文件。",
            );
            return;
        }
    };

    match compensate_cancelled_prepared_batch(&import_service, &batch, &cancellation) {
        Ok(true) => {
            finish_task(
                &jobs,
                &task_id,
                ImportTaskStateDto::Cancelled,
                "导入已取消。",
            );
            return;
        }
        Ok(false) => {}
        Err(_) => {
            finish_task(
                &jobs,
                &task_id,
                ImportTaskStateDto::Failed,
                "取消导入时未能清理新复制文件。",
            );
            return;
        }
    }

    let inputs = match prepared_assets(&batch, project_id) {
        Some(inputs) => inputs,
        None => {
            let _ = import_service.compensate_batch(&batch);
            finish_task(
                &jobs,
                &task_id,
                ImportTaskStateDto::Failed,
                "媒体信息超出可保存范围。",
            );
            return;
        }
    };

    match LibraryService::new().create_imported_assets_batch(Path::new(&root), &inputs) {
        Ok(asset_ids) => {
            let cover_result = bundled_ffmpeg_path.as_deref().map(|ffmpeg_path| {
                MediaIntegrityService::new().generate_default_video_covers(
                    &root,
                    ffmpeg_path,
                    &default_video_cover_requests(&root, &batch, &asset_ids),
                )
            });
            update_task(&jobs, &task_id, |task| {
                task.state = ImportTaskStateDto::Completed;
                task.completed = task.total;
                task.message = match cover_result {
                    Some(result) if result.requested > result.generated => {
                        Some("导入完成，部分视频未能生成预览封面。")
                    }
                    _ => Some("导入完成。"),
                };
            });
        }
        Err(_) => {
            let compensation_failed = import_service.compensate_batch(&batch).is_err();
            finish_task(
                &jobs,
                &task_id,
                ImportTaskStateDto::Failed,
                if compensation_failed {
                    "写入失败，部分新复制文件需要手动检查。"
                } else {
                    "写入失败，已撤销本批新复制文件。"
                },
            );
        }
    }
}

fn ensure_workspace_writable(root: &Path) -> Result<(), CommandErrorDto> {
    WriteAccessGuard::ensure_writable(root).map_err(|error| match error {
        AccessModeServiceError::ReadOnly => CommandErrorDto::new(
            "WORKSPACE_READ_ONLY",
            "工作区正处于只读展示模式，无法修改内容。",
        ),
        AccessModeServiceError::Workspace(_)
        | AccessModeServiceError::DatabaseUnavailable
        | AccessModeServiceError::InvalidSetting
        | AccessModeServiceError::ClockUnavailable => {
            CommandErrorDto::new("IMPORT_FAILED", "无法完成媒体导入。")
        }
    })
}

fn compensate_cancelled_prepared_batch(
    import_service: &ImportService,
    batch: &crate::services::PreparedImportBatch,
    cancellation: &ImportCancellationToken,
) -> Result<bool, ImportServiceError> {
    if !cancellation.is_cancelled() {
        return Ok(false);
    }
    import_service.compensate_batch(batch)?;
    Ok(true)
}

fn expand_import_sources(
    sources: Vec<String>,
    cancellation: &ImportCancellationToken,
) -> Result<Vec<PathBuf>, ()> {
    let mut files = Vec::new();
    let mut directories = VecDeque::new();
    let mut scanned_entries = 0_usize;

    for source in sources {
        let path = PathBuf::from(source);
        let metadata = fs::symlink_metadata(&path).map_err(|_| ())?;
        if metadata.is_dir() {
            if is_link_or_reparse(&metadata) {
                return Err(());
            }
            directories.push_back(path);
        } else if metadata.is_file() {
            files.push(path);
        } else {
            return Err(());
        }
    }

    while let Some(directory) = directories.pop_front() {
        if cancellation.is_cancelled() {
            return Err(());
        }
        let entries = fs::read_dir(directory).map_err(|_| ())?;
        for entry in entries {
            if cancellation.is_cancelled() {
                return Err(());
            }
            scanned_entries += 1;
            if scanned_entries > MAX_SCANNED_DIRECTORY_ENTRIES {
                return Err(());
            }
            let entry = entry.map_err(|_| ())?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|_| ())?;
            if is_link_or_reparse(&metadata) {
                continue;
            }
            if metadata.is_dir() {
                directories.push_back(path);
            } else if metadata.is_file() && is_supported_media_path(&path) {
                files.push(path);
                if files.len() > MAX_IMPORT_ITEMS {
                    return Err(());
                }
            }
        }
    }

    if files.is_empty() || files.len() > MAX_IMPORT_ITEMS {
        return Err(());
    }
    Ok(files)
}

fn is_supported_media_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "webp" | "gif" | "mp4"
            )
        })
}

fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

#[cfg(test)]
struct ActiveImportJobGuard;

#[cfg(test)]
impl ActiveImportJobGuard {
    fn enter() -> Self {
        let active = ACTIVE_IMPORT_JOBS.fetch_add(1, Ordering::AcqRel) + 1;
        MAX_ACTIVE_IMPORT_JOBS.fetch_max(active, Ordering::AcqRel);
        Self
    }
}

#[cfg(test)]
impl Drop for ActiveImportJobGuard {
    fn drop(&mut self) {
        ACTIVE_IMPORT_JOBS.fetch_sub(1, Ordering::AcqRel);
    }
}

fn start_worker_if_needed(state: &ImportCommandState) {
    if state
        .worker_running
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    let jobs = Arc::clone(&state.jobs);
    let queue = Arc::clone(&state.queue);
    let worker_running = Arc::clone(&state.worker_running);
    thread::spawn(move || {
        loop {
            let next = {
                let mut queued = queue
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                match queued.pop_front() {
                    Some(next) => Some(next),
                    None => {
                        // 与入队使用同一把锁更新标志，避免任务在 worker 退出边界丢失唤醒。
                        worker_running.store(false, Ordering::Release);
                        None
                    }
                }
            };
            let Some(next) = next else {
                break;
            };
            run_import_job(
                Arc::clone(&jobs),
                next.task_id,
                next.request,
                next.cancellation,
                next.bundled_ffmpeg_path,
            );
        }
    });
}

fn prepared_assets(
    batch: &crate::services::PreparedImportBatch,
    project_id: Option<i64>,
) -> Option<Vec<(CreateAsset, Option<i64>)>> {
    batch
        .items()
        .iter()
        .map(|item| {
            let file_size = i64::try_from(item.file_size).ok()?;
            let duration_ms = item
                .metadata
                .duration_ms
                .map(i64::try_from)
                .transpose()
                .ok()?;
            let stored_path = match item.import_mode {
                ImportMode::Copy => item.stored_path.to_string_lossy().replace('\\', "/"),
                ImportMode::Reference => item.stored_path.to_string_lossy().into_owned(),
            };
            Some((
                CreateAsset {
                    media: AssetMetadata {
                        project_id,
                        media_type: match item.media_kind {
                            MediaKind::Image => MediaType::Image,
                            MediaKind::Video => MediaType::Video,
                        },
                        path_kind: match item.import_mode {
                            ImportMode::Copy => PathKind::Managed,
                            ImportMode::Reference => PathKind::External,
                        },
                        stored_path,
                        file_name: item.file_name.clone(),
                        mime_type: Some(item.mime_type.to_owned()),
                        file_size: Some(file_size),
                        content_hash: Some(item.content_hash.clone()),
                        width: item.metadata.width,
                        height: item.metadata.height,
                        duration_ms,
                        frame_rate: item.metadata.frame_rate,
                        has_audio: item.metadata.has_audio,
                    },
                    // 自动识别只进入待确认信封，不能直接污染正式提示词字段。
                    prompt: PromptText::default(),
                    model: None,
                    platform: None,
                    generation_params: pending_recognition_envelope(&item.recognized_generation),
                    rating: 0,
                    is_favorite: false,
                    is_public: false,
                    notes: String::new(),
                    category_ids: Vec::new(),
                    tag_ids: Vec::new(),
                },
                item.source_modified_at,
            ))
        })
        .collect()
}

fn default_video_cover_requests(
    root: &Path,
    batch: &crate::services::PreparedImportBatch,
    asset_ids: &[i64],
) -> Vec<DefaultVideoCoverRequest> {
    batch
        .items()
        .iter()
        .zip(asset_ids)
        .filter(|(item, _)| item.media_kind == MediaKind::Video)
        .map(|(item, asset_id)| DefaultVideoCoverRequest {
            asset_id: *asset_id,
            video_path: match item.import_mode {
                ImportMode::Copy => root.join(&item.stored_path),
                ImportMode::Reference => item.stored_path.clone(),
            },
        })
        .collect()
}

fn pending_recognition_envelope(
    recognized: &crate::domain::RecognizedGenerationMetadata,
) -> serde_json::Value {
    let has_value = !recognized.prompt_zh.is_empty()
        || !recognized.prompt_en.is_empty()
        || !recognized.negative_prompt.is_empty()
        || recognized
            .generation_params
            .as_object()
            .is_some_and(|params| !params.is_empty());
    if !has_value {
        return serde_json::json!({});
    }
    serde_json::json!({
        "_pendingRecognition": {
            "promptZh": recognized.prompt_zh,
            "promptEn": recognized.prompt_en,
            "negativePrompt": recognized.negative_prompt,
            "generationParams": recognized.generation_params,
        }
    })
}

fn update_task(
    jobs: &Mutex<HashMap<String, ImportJob>>,
    task_id: &str,
    update: impl FnOnce(&mut ImportTaskDto),
) {
    if let Some(job) = jobs
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get_mut(task_id)
    {
        update(&mut job.task);
    }
}

fn finish_task(
    jobs: &Mutex<HashMap<String, ImportJob>>,
    task_id: &str,
    state: ImportTaskStateDto,
    message: &'static str,
) {
    update_task(jobs, task_id, |task| {
        task.state = state;
        task.message = Some(message);
    });
}

fn prune_finished_jobs(jobs: &mut HashMap<String, ImportJob>) {
    while jobs.len() >= MAX_TRACKED_IMPORTS {
        let oldest_finished = jobs
            .iter()
            .filter(|(_, job)| {
                matches!(
                    job.task.state,
                    ImportTaskStateDto::Completed
                        | ImportTaskStateDto::Cancelled
                        | ImportTaskStateDto::Failed
                )
            })
            .filter_map(|(id, _)| id.parse::<u64>().ok().map(|number| (number, id.clone())))
            .min_by_key(|(number, _)| *number)
            .map(|(_, id)| id);
        let Some(id) = oldest_finished else {
            break;
        };
        jobs.remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs, time::Duration};

    use super::*;
    use crate::{
        domain::AccessMode,
        services::{AccessModeService, DatabaseService, WorkspaceService},
    };

    #[test]
    fn import_command_guard_rejects_read_only_workspace_before_queueing() {
        let root = env::temp_dir().join(format!(
            "ai-gallery-import-read-only-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        WorkspaceService::new()
            .create_workspace(&root)
            .expect("应能创建导入测试工作区");
        DatabaseService::new()
            .prepare_workspace_database(&root)
            .expect("应能准备当前数据库");
        AccessModeService::new()
            .set_mode(&root, AccessMode::ReadOnly)
            .expect("应能设为只读");

        let error =
            ensure_workspace_writable(&root).expect_err("只读模式不得进入导入队列或触发文件写入");
        assert_eq!(error.code, "WORKSPACE_READ_ONLY");
        assert!(
            !root.join("media/images/imported.png").exists(),
            "拒绝前不应创建受管媒体文件"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn multiple_jobs_share_one_global_batch_worker() {
        ACTIVE_IMPORT_JOBS.store(0, Ordering::Release);
        MAX_ACTIVE_IMPORT_JOBS.store(0, Ordering::Release);
        let root = env::temp_dir().join(format!(
            "ai-gallery-import-queue-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        WorkspaceService::new()
            .create_workspace(&root)
            .expect("应能创建导入队列测试工作区");
        let source = root.parent().expect("应存在父目录").join(format!(
            "ai-gallery-import-queue-source-{}.png",
            std::process::id()
        ));
        let mut png = vec![0_u8; 24];
        png[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        png[16..20].copy_from_slice(&16_u32.to_be_bytes());
        png[20..24].copy_from_slice(&16_u32.to_be_bytes());
        fs::write(&source, png).expect("应能创建测试图片");

        let state = ImportCommandState::default();
        for number in 1..=2 {
            let task_id = number.to_string();
            let cancellation = ImportCancellationToken::default();
            state.jobs.lock().expect("任务锁不应中毒").insert(
                task_id.clone(),
                ImportJob {
                    task: ImportTaskDto {
                        id: task_id.clone(),
                        state: ImportTaskStateDto::Queued,
                        completed: 0,
                        total: 1,
                        current_file_name: None,
                        message: None,
                    },
                    cancellation: cancellation.clone(),
                },
            );
            state
                .queue
                .lock()
                .expect("队列锁不应中毒")
                .push_back(QueuedImport {
                    task_id,
                    request: StartImportRequestDto {
                        root_path: root.to_string_lossy().into_owned(),
                        source_paths: vec![source.to_string_lossy().into_owned()],
                        mode: ImportModeDto::Reference,
                        project_id: None,
                    },
                    cancellation,
                    bundled_ffmpeg_path: None,
                });
        }
        start_worker_if_needed(&state);

        for _ in 0..500 {
            let finished = state
                .jobs
                .lock()
                .expect("任务锁不应中毒")
                .values()
                .all(|job| {
                    matches!(
                        job.task.state,
                        ImportTaskStateDto::Completed
                            | ImportTaskStateDto::Cancelled
                            | ImportTaskStateDto::Failed
                    )
                });
            if finished {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(MAX_ACTIVE_IMPORT_JOBS.load(Ordering::Acquire), 1);
        assert!(
            state
                .jobs
                .lock()
                .expect("任务锁不应中毒")
                .values()
                .all(|job| {
                    matches!(
                        job.task.state,
                        ImportTaskStateDto::Completed | ImportTaskStateDto::Failed
                    )
                })
        );
        let _ = fs::remove_file(source);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cancellation_after_copy_compensates_before_database_write() {
        let root = env::temp_dir().join(format!(
            "ai-gallery-import-cancel-after-copy-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        WorkspaceService::new()
            .create_workspace(&root)
            .expect("应能创建取消测试工作区");
        let source = root.parent().expect("应存在父目录").join(format!(
            "ai-gallery-import-cancel-source-{}.png",
            std::process::id()
        ));
        let mut png = vec![0_u8; 24];
        png[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        png[16..20].copy_from_slice(&16_u32.to_be_bytes());
        png[20..24].copy_from_slice(&16_u32.to_be_bytes());
        fs::write(&source, png).expect("应能创建测试图片");

        let service = ImportService::new();
        let cancellation = ImportCancellationToken::default();
        let batch = service
            .prepare_batch(
                PrepareImportBatchRequest {
                    workspace_root: root.clone(),
                    import_mode: ImportMode::Copy,
                    source_paths: vec![source.clone()],
                },
                &cancellation,
            )
            .expect("复制准备应成功");
        let copied = root.join(&batch.items()[0].stored_path);
        assert!(copied.exists(), "取消前应已生成本批副本");

        cancellation.cancel();
        assert_eq!(
            compensate_cancelled_prepared_batch(&service, &batch, &cancellation),
            Ok(true)
        );
        assert!(!copied.exists(), "写库前取消必须补偿本批新副本");
        assert!(
            !root.join("data/library.sqlite3").exists(),
            "取消后不得写库"
        );
        assert!(source.exists(), "源文件必须保留");

        let _ = fs::remove_file(source);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn selected_directory_expands_supported_media_recursively() {
        let root = env::temp_dir().join(format!(
            "ai-gallery-import-directory-{}",
            std::process::id()
        ));
        let nested = root.join("nested");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&nested).expect("应能创建文件夹导入测试目录");
        fs::write(root.join("ignore.txt"), b"ignore").expect("应能创建无关文件");
        fs::write(nested.join("image.PNG"), b"image").expect("应能创建媒体占位文件");

        let files = expand_import_sources(
            vec![root.to_string_lossy().into_owned()],
            &ImportCancellationToken::default(),
        )
        .expect("应能递归展开所选目录");
        assert_eq!(files, vec![nested.join("image.PNG")]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn recognized_png_parameters_flow_into_new_asset_fields() {
        let root = env::temp_dir().join(format!(
            "ai-gallery-import-recognized-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        WorkspaceService::new()
            .create_workspace(&root)
            .expect("应能创建识别参数测试工作区");
        let source = root
            .parent()
            .expect("应存在父目录")
            .join(format!("ai-gallery-recognized-{}.png", std::process::id()));
        let value = "future city\nNegative prompt: blur\nSteps: 20, Seed: 42";
        let mut payload = b"parameters\0".to_vec();
        payload.extend_from_slice(value.as_bytes());
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        png.extend_from_slice(b"tEXt");
        png.extend_from_slice(&payload);
        png.extend_from_slice(&[0_u8; 4]);
        fs::write(&source, png).expect("应能写入含生成参数的 PNG");

        let batch = ImportService::new()
            .prepare_batch(
                PrepareImportBatchRequest {
                    workspace_root: root.clone(),
                    import_mode: ImportMode::Reference,
                    source_paths: vec![source.clone()],
                },
                &ImportCancellationToken::default(),
            )
            .expect("含参数 PNG 应能准备导入");
        let inputs = prepared_assets(&batch, None).expect("识别结果应能转为作品输入");

        assert!(inputs[0].0.prompt.prompt_en.is_empty());
        assert!(inputs[0].0.prompt.negative_prompt.is_empty());
        let pending = &inputs[0].0.generation_params["_pendingRecognition"];
        assert_eq!(pending["promptEn"], "future city");
        assert_eq!(pending["negativePrompt"], "blur");
        assert_eq!(pending["generationParams"]["Steps"], 20);
        assert_eq!(pending["generationParams"]["Seed"], 42);
        let _ = fs::remove_file(source);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn default_video_cover_request_uses_the_imported_video_path() {
        let root = env::temp_dir().join(format!(
            "ai-gallery-import-default-cover-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        WorkspaceService::new()
            .create_workspace(&root)
            .expect("应能创建默认封面测试工作区");
        let source = root.parent().expect("应存在父目录").join(format!(
            "ai-gallery-default-cover-{}.mp4",
            std::process::id()
        ));
        fs::write(&source, b"not-a-real-video").expect("应能创建视频占位文件");

        let batch = ImportService::new()
            .prepare_batch(
                PrepareImportBatchRequest {
                    workspace_root: root.clone(),
                    import_mode: ImportMode::Reference,
                    source_paths: vec![source.clone()],
                },
                &ImportCancellationToken::default(),
            )
            .expect("MP4 引用导入应能准备完成");
        let requests = default_video_cover_requests(&root, &batch, &[42]);

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].asset_id, 42);
        assert_eq!(requests[0].video_path, source);

        let _ = fs::remove_file(requests[0].video_path.clone());
        let _ = fs::remove_dir_all(root);
    }
}
