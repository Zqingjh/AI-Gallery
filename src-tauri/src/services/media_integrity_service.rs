use std::{
    fs::{self, File, OpenOptions},
    io::{BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use image::{
    ColorType, GenericImageView, ImageEncoder, ImageReader, Limits, codecs::png::PngEncoder,
};

use crate::{
    adapters::{
        file_hash::{FileHashCancellationToken, hash_reader},
        video_frame::{VideoFrameCancellationToken, VideoFrameRequest, generate_video_frame},
    },
    domain::{
        AssetCover, AssetPathRecord, AssetPathRepairUpdate, CoverSourceType, MediaType, PathKind,
        SaveAssetCover, StoredPathKind,
    },
    repositories::{MediaIntegrityRepository, MediaIntegrityRepositoryError},
    services::{
        AccessModeServiceError, DatabaseService, DatabaseServiceError, WorkspaceService,
        WriteAccessGuard,
    },
};

const MAX_COVER_SOURCE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_COVER_RESPONSE_BYTES: u64 = 10 * 1024 * 1024;
const MAX_COVER_PIXELS: u64 = 40_000_000;
const MAX_COVER_EDGE: u32 = 8192;
const MAX_DECODE_BYTES: u64 = 160 * 1024 * 1024;
const HASH_TIMEOUT: Duration = Duration::from_secs(30);
static FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MediaIntegrityServiceError {
    InvalidInput,
    NotFound,
    Conflict,
    ReadOnly,
    DatabaseUnavailable,
    MediaUnavailable,
    ProcessingFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CoverContent {
    pub(crate) cover: AssetCover,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KeyFramePreview {
    pub(crate) asset_id: i64,
    pub(crate) frame_timestamp_ms: i64,
    pub(crate) bytes: Vec<u8>,
}

/// 导入完成后自动生成的视频首帧封面请求。
#[derive(Debug, Clone)]
pub(crate) struct DefaultVideoCoverRequest {
    pub(crate) asset_id: i64,
    pub(crate) video_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DefaultVideoCoverResult {
    pub(crate) requested: usize,
    pub(crate) generated: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PathRepairVerification {
    Matched,
    Unverified,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PathRepairPreview {
    pub(crate) asset_id: i64,
    pub(crate) file_name: String,
    pub(crate) path_kind: PathKind,
    pub(crate) file_size: i64,
    pub(crate) verification: PathRepairVerification,
}

struct CandidateFile {
    file_name: String,
    file_size: i64,
    content_hash: String,
    stored_path: String,
}

pub(crate) struct MediaIntegrityService;

impl MediaIntegrityService {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn get_asset_cover(
        &self,
        root: &Path,
        asset_id: i64,
    ) -> Result<Option<CoverContent>, MediaIntegrityServiceError> {
        validate_id(asset_id)?;
        let connection = DatabaseService::new()
            .open_current_workspace_database(root)
            .map_err(map_database_error)?;
        let Some(record) = MediaIntegrityRepository::get_asset_cover(&connection, asset_id)
            .map_err(map_repository_error)?
        else {
            return Ok(None);
        };
        let bytes = read_managed_png(root, &record.stored_path)?;
        Ok(Some(CoverContent {
            cover: record.cover,
            bytes,
        }))
    }

    pub(crate) fn preview_video_key_frame(
        &self,
        root: &Path,
        asset_id: i64,
        frame_timestamp_ms: i64,
        ffmpeg_path: &Path,
        video_path: &Path,
    ) -> Result<KeyFramePreview, MediaIntegrityServiceError> {
        validate_key_frame_input(asset_id, frame_timestamp_ms)?;
        WriteAccessGuard::ensure_writable(root).map_err(map_access_error)?;
        let record = self.asset_record(root, asset_id)?;
        let video_path = validate_selected_video(root, &record, video_path)?;
        let temporary = unique_cover_path(root, asset_id, "preview")?;
        let result = generate_video_frame(
            &VideoFrameRequest {
                ffmpeg_executable: ffmpeg_path.to_path_buf(),
                input_path: video_path,
                output_path: temporary.clone(),
                timestamp_ms: frame_timestamp_ms as u64,
            },
            &VideoFrameCancellationToken::default(),
        )
        .map_err(|_| MediaIntegrityServiceError::ProcessingFailed)
        .and_then(|()| read_png_file(&temporary));
        let _ = fs::remove_file(&temporary);
        result.map(|bytes| KeyFramePreview {
            asset_id,
            frame_timestamp_ms,
            bytes,
        })
    }

    pub(crate) fn set_video_key_frame_cover(
        &self,
        root: &Path,
        asset_id: i64,
        frame_timestamp_ms: i64,
        ffmpeg_path: &Path,
        video_path: &Path,
    ) -> Result<CoverContent, MediaIntegrityServiceError> {
        validate_key_frame_input(asset_id, frame_timestamp_ms)?;
        let _lease = WriteAccessGuard::acquire_writable(root).map_err(map_access_error)?;
        let record = self.asset_record(root, asset_id)?;
        let video_path = validate_selected_video(root, &record, video_path)?;
        let destination = unique_cover_path(root, asset_id, "frame")?;
        generate_video_frame(
            &VideoFrameRequest {
                ffmpeg_executable: ffmpeg_path.to_path_buf(),
                input_path: video_path,
                output_path: destination.clone(),
                timestamp_ms: frame_timestamp_ms as u64,
            },
            &VideoFrameCancellationToken::default(),
        )
        .map_err(|_| MediaIntegrityServiceError::ProcessingFailed)?;
        self.commit_cover(
            root,
            asset_id,
            CoverSourceType::KeyFrame,
            Some(frame_timestamp_ms),
            &destination,
        )
    }

    /// 使用应用已配置的 FFmpeg 从作品自身视频生成首帧封面。
    pub(crate) fn set_default_video_cover(
        &self,
        root: &Path,
        asset_id: i64,
        ffmpeg_path: &Path,
    ) -> Result<CoverContent, MediaIntegrityServiceError> {
        validate_id(asset_id)?;
        let record = self.asset_record(root, asset_id)?;
        let video_path = match record.path_kind {
            PathKind::Managed => root.join(&record.stored_path),
            PathKind::External => PathBuf::from(&record.stored_path),
        };
        self.set_video_key_frame_cover(root, asset_id, 0, ffmpeg_path, &video_path)
    }

    /// 导入时按顺序生成首帧封面。单个视频失败不会影响已入库的作品。
    pub(crate) fn generate_default_video_covers(
        &self,
        root: &Path,
        ffmpeg_path: &Path,
        requests: &[DefaultVideoCoverRequest],
    ) -> DefaultVideoCoverResult {
        let generated = requests
            .iter()
            .filter(|request| {
                self.set_video_key_frame_cover(
                    root,
                    request.asset_id,
                    0,
                    ffmpeg_path,
                    &request.video_path,
                )
                .is_ok()
            })
            .count();
        DefaultVideoCoverResult {
            requested: requests.len(),
            generated,
        }
    }

    pub(crate) fn set_custom_video_cover(
        &self,
        root: &Path,
        asset_id: i64,
        source_path: &Path,
    ) -> Result<CoverContent, MediaIntegrityServiceError> {
        validate_id(asset_id)?;
        let _lease = WriteAccessGuard::acquire_writable(root).map_err(map_access_error)?;
        let record = self.asset_record(root, asset_id)?;
        if record.media_type != MediaType::Video {
            return Err(MediaIntegrityServiceError::InvalidInput);
        }
        let destination = unique_cover_path(root, asset_id, "custom")?;
        if let Err(error) = convert_custom_cover(source_path, &destination) {
            let _ = fs::remove_file(&destination);
            return Err(error);
        }
        self.commit_cover(root, asset_id, CoverSourceType::Custom, None, &destination)
    }

    pub(crate) fn remove_video_cover(
        &self,
        root: &Path,
        asset_id: i64,
    ) -> Result<(), MediaIntegrityServiceError> {
        validate_id(asset_id)?;
        let _lease = WriteAccessGuard::acquire_writable(root).map_err(map_access_error)?;
        let mut connection = DatabaseService::new()
            .open_current_workspace_database(root)
            .map_err(map_database_error)?;
        let previous = MediaIntegrityRepository::remove_asset_cover(&mut connection, asset_id)
            .map_err(map_repository_error)?
            .ok_or(MediaIntegrityServiceError::NotFound)?;
        remove_managed_cover_best_effort(root, &previous.stored_path);
        Ok(())
    }

    pub(crate) fn preview_asset_path_repair(
        &self,
        root: &Path,
        asset_id: i64,
        candidate_path: &Path,
    ) -> Result<PathRepairPreview, MediaIntegrityServiceError> {
        validate_id(asset_id)?;
        let record = self.asset_record(root, asset_id)?;
        ensure_current_path_missing(root, &record)?;
        let candidate = inspect_candidate(root, &record, candidate_path)?;
        preview_from(&record, &candidate)
    }

    pub(crate) fn execute_asset_path_repair(
        &self,
        root: &Path,
        asset_id: i64,
        candidate_path: &Path,
        allow_unverified: bool,
    ) -> Result<PathRepairPreview, MediaIntegrityServiceError> {
        validate_id(asset_id)?;
        let _lease = WriteAccessGuard::acquire_writable(root).map_err(map_access_error)?;
        let mut connection = DatabaseService::new()
            .open_current_workspace_database(root)
            .map_err(map_database_error)?;
        let record = MediaIntegrityRepository::get_asset_for_path_repair(&connection, asset_id)
            .map_err(map_repository_error)?;
        ensure_current_path_missing(root, &record)?;
        let candidate = inspect_candidate(root, &record, candidate_path)?;
        let preview = preview_from(&record, &candidate)?;
        if preview.verification == PathRepairVerification::Unverified && !allow_unverified {
            return Err(MediaIntegrityServiceError::Conflict);
        }
        MediaIntegrityRepository::repair_asset_path(
            &mut connection,
            &AssetPathRepairUpdate {
                asset_id,
                expected_updated_at: record.updated_at,
                expected_path_kind: record.path_kind,
                stored_path: candidate.stored_path,
                file_name: candidate.file_name,
                file_size: Some(candidate.file_size),
                content_hash: candidate.content_hash,
            },
            now()?,
        )
        .map_err(map_repository_error)?;
        Ok(preview)
    }

    fn asset_record(
        &self,
        root: &Path,
        asset_id: i64,
    ) -> Result<AssetPathRecord, MediaIntegrityServiceError> {
        let connection = DatabaseService::new()
            .open_current_workspace_database(root)
            .map_err(map_database_error)?;
        MediaIntegrityRepository::get_asset_for_path_repair(&connection, asset_id)
            .map_err(map_repository_error)
    }

    fn commit_cover(
        &self,
        root: &Path,
        asset_id: i64,
        source_type: CoverSourceType,
        frame_timestamp_ms: Option<i64>,
        destination: &Path,
    ) -> Result<CoverContent, MediaIntegrityServiceError> {
        let stored_path = cover_stored_path(root, destination)?;
        let bytes = match read_png_file(destination) {
            Ok(bytes) => bytes,
            Err(error) => {
                let _ = fs::remove_file(destination);
                return Err(error);
            }
        };
        let mut connection = DatabaseService::new()
            .open_current_workspace_database(root)
            .map_err(map_database_error)?;
        let previous = match MediaIntegrityRepository::save_asset_cover(
            &mut connection,
            &SaveAssetCover {
                asset_id,
                source_type,
                stored_path: stored_path.clone(),
                frame_timestamp_ms,
            },
            now()?,
        ) {
            Ok(value) => value,
            Err(error) => {
                let _ = fs::remove_file(destination);
                return Err(map_repository_error(error));
            }
        };
        if let Some(previous) = previous {
            remove_managed_cover_best_effort(root, &previous.stored_path);
        }
        let cover = MediaIntegrityRepository::get_asset_cover(&connection, asset_id)
            .map_err(map_repository_error)?
            .ok_or(MediaIntegrityServiceError::NotFound)?
            .cover;
        Ok(CoverContent { cover, bytes })
    }
}

fn validate_id(id: i64) -> Result<(), MediaIntegrityServiceError> {
    if id <= 0 {
        Err(MediaIntegrityServiceError::InvalidInput)
    } else {
        Ok(())
    }
}

fn validate_key_frame_input(
    asset_id: i64,
    frame_timestamp_ms: i64,
) -> Result<(), MediaIntegrityServiceError> {
    validate_id(asset_id)?;
    if frame_timestamp_ms < 0 {
        return Err(MediaIntegrityServiceError::InvalidInput);
    }
    Ok(())
}

fn validate_selected_video(
    root: &Path,
    record: &AssetPathRecord,
    selected: &Path,
) -> Result<PathBuf, MediaIntegrityServiceError> {
    if record.media_type != MediaType::Video || !selected.is_absolute() {
        return Err(MediaIntegrityServiceError::InvalidInput);
    }
    let selected_metadata =
        fs::symlink_metadata(selected).map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?;
    if !selected_metadata.is_file() || is_link_or_reparse(&selected_metadata) {
        return Err(MediaIntegrityServiceError::InvalidInput);
    }
    let expected = match record.path_kind {
        PathKind::Managed => root.join(&record.stored_path),
        PathKind::External => PathBuf::from(&record.stored_path),
    };
    let selected = selected
        .canonicalize()
        .map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?;
    let expected = expected
        .canonicalize()
        .map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?;
    if selected != expected {
        return Err(MediaIntegrityServiceError::Conflict);
    }
    Ok(selected)
}

fn inspect_candidate(
    root: &Path,
    record: &AssetPathRecord,
    candidate_path: &Path,
) -> Result<CandidateFile, MediaIntegrityServiceError> {
    if !candidate_path.is_absolute() {
        return Err(MediaIntegrityServiceError::InvalidInput);
    }
    let selected_metadata = fs::symlink_metadata(candidate_path)
        .map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?;
    if is_link_or_reparse(&selected_metadata) {
        return Err(MediaIntegrityServiceError::InvalidInput);
    }
    let canonical = candidate_path
        .canonicalize()
        .map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?;
    let metadata = fs::symlink_metadata(&canonical)
        .map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(MediaIntegrityServiceError::InvalidInput);
    }
    let file_size =
        i64::try_from(metadata.len()).map_err(|_| MediaIntegrityServiceError::InvalidInput)?;
    let file_name = canonical
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or(MediaIntegrityServiceError::InvalidInput)?
        .to_owned();
    let stored_path = match record.path_kind {
        PathKind::Managed => {
            let media_root = root
                .join("media")
                .canonicalize()
                .map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?;
            if !canonical.starts_with(&media_root) {
                return Err(MediaIntegrityServiceError::Conflict);
            }
            let relative = canonical
                .strip_prefix(
                    root.canonicalize()
                        .map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?,
                )
                .map_err(|_| MediaIntegrityServiceError::Conflict)?;
            relative.to_string_lossy().replace('\\', "/")
        }
        PathKind::External => canonical.to_string_lossy().into_owned(),
    };
    let content_hash = hash_stable_file(&canonical, &metadata)?;
    let final_path = candidate_path
        .canonicalize()
        .map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?;
    if final_path != canonical {
        return Err(MediaIntegrityServiceError::Conflict);
    }
    Ok(CandidateFile {
        file_name,
        file_size,
        content_hash,
        stored_path,
    })
}

fn hash_stable_file(
    path: &Path,
    expected_metadata: &fs::Metadata,
) -> Result<String, MediaIntegrityServiceError> {
    let mut file = File::open(path).map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?;
    let opened_metadata = file
        .metadata()
        .map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?;
    if !opened_metadata.is_file()
        || opened_metadata.len() != expected_metadata.len()
        || opened_metadata.modified().ok() != expected_metadata.modified().ok()
    {
        return Err(MediaIntegrityServiceError::Conflict);
    }
    let cancellation = FileHashCancellationToken::default();
    let worker_cancellation = cancellation.clone();
    let (sender, receiver) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        let result = hash_reader(&mut file, &worker_cancellation).and_then(|hash| {
            let metadata = file
                .metadata()
                .map_err(|_| crate::adapters::file_hash::FileHashError::ReadFailed)?;
            Ok((hash, metadata.len(), metadata.modified().ok()))
        });
        let _ = sender.send(result);
    });
    let result = match receiver.recv_timeout(HASH_TIMEOUT) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            cancellation.cancel();
            let _ = worker.join();
            return Err(MediaIntegrityServiceError::ProcessingFailed);
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let _ = worker.join();
            return Err(MediaIntegrityServiceError::MediaUnavailable);
        }
    };
    worker
        .join()
        .map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?;
    let (hash, final_length, final_modified) =
        result.map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?;
    if final_length != opened_metadata.len() || final_modified != opened_metadata.modified().ok() {
        return Err(MediaIntegrityServiceError::Conflict);
    }
    Ok(hash)
}

fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn preview_from(
    record: &AssetPathRecord,
    candidate: &CandidateFile,
) -> Result<PathRepairPreview, MediaIntegrityServiceError> {
    let verification = match record.content_hash.as_deref() {
        Some(expected) if expected == candidate.content_hash => PathRepairVerification::Matched,
        Some(_) => return Err(MediaIntegrityServiceError::Conflict),
        None => PathRepairVerification::Unverified,
    };
    Ok(PathRepairPreview {
        asset_id: record.asset_id,
        file_name: candidate.file_name.clone(),
        path_kind: record.path_kind,
        file_size: candidate.file_size,
        verification,
    })
}

fn ensure_current_path_missing(
    root: &Path,
    record: &AssetPathRecord,
) -> Result<(), MediaIntegrityServiceError> {
    let kind = match record.path_kind {
        PathKind::Managed => StoredPathKind::Managed,
        PathKind::External => StoredPathKind::External,
    };
    let status = WorkspaceService::new()
        .check_stored_path(root, kind, Path::new(&record.stored_path))
        .map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?;
    if status.availability == crate::domain::PathAvailability::Available {
        return Err(MediaIntegrityServiceError::Conflict);
    }
    Ok(())
}

fn unique_cover_path(
    root: &Path,
    asset_id: i64,
    label: &str,
) -> Result<PathBuf, MediaIntegrityServiceError> {
    let cover_root = root
        .join("media/covers")
        .canonicalize()
        .map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?;
    let workspace_root = root
        .canonicalize()
        .map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?;
    if !cover_root.starts_with(&workspace_root) {
        return Err(MediaIntegrityServiceError::InvalidInput);
    }
    let sequence = FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    Ok(cover_root.join(format!(
        "{label}-{asset_id}-{}-{sequence}.png",
        std::process::id()
    )))
}

fn cover_stored_path(root: &Path, path: &Path) -> Result<String, MediaIntegrityServiceError> {
    let root = root
        .canonicalize()
        .map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?;
    let path = path
        .canonicalize()
        .map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?;
    let relative = path
        .strip_prefix(&root)
        .map_err(|_| MediaIntegrityServiceError::InvalidInput)?;
    let portable = relative.to_string_lossy().replace('\\', "/");
    if !portable.starts_with("media/covers/") {
        return Err(MediaIntegrityServiceError::InvalidInput);
    }
    Ok(portable)
}

fn convert_custom_cover(
    source_path: &Path,
    destination: &Path,
) -> Result<(), MediaIntegrityServiceError> {
    if !source_path.is_absolute() {
        return Err(MediaIntegrityServiceError::InvalidInput);
    }
    let metadata = fs::symlink_metadata(source_path)
        .map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() == 0
        || metadata.len() > MAX_COVER_SOURCE_BYTES
    {
        return Err(MediaIntegrityServiceError::InvalidInput);
    }
    let file = File::open(source_path).map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?;
    let mut reader = ImageReader::new(BufReader::new(file));
    reader = reader
        .with_guessed_format()
        .map_err(|_| MediaIntegrityServiceError::InvalidInput)?;
    let format = reader
        .format()
        .ok_or(MediaIntegrityServiceError::InvalidInput)?;
    if !matches!(
        format,
        image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP
    ) {
        return Err(MediaIntegrityServiceError::InvalidInput);
    }
    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|_| MediaIntegrityServiceError::InvalidInput)?;
    let (width, height) = image.dimensions();
    if width == 0
        || height == 0
        || width > MAX_COVER_EDGE
        || height > MAX_COVER_EDGE
        || u64::from(width) * u64::from(height) > MAX_COVER_PIXELS
    {
        return Err(MediaIntegrityServiceError::InvalidInput);
    }
    let rgba = image.to_rgba8();
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .map_err(|_| MediaIntegrityServiceError::ProcessingFailed)?;
    PngEncoder::new(&mut output)
        .write_image(rgba.as_raw(), width, height, ColorType::Rgba8.into())
        .map_err(|_| MediaIntegrityServiceError::ProcessingFailed)?;
    output
        .flush()
        .and_then(|()| output.sync_all())
        .map_err(|_| MediaIntegrityServiceError::ProcessingFailed)
}

fn read_managed_png(root: &Path, stored_path: &str) -> Result<Vec<u8>, MediaIntegrityServiceError> {
    WorkspaceService::new()
        .check_stored_path(root, StoredPathKind::Managed, Path::new(stored_path))
        .map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?;
    read_png_file(&root.join(stored_path))
}

fn read_png_file(path: &Path) -> Result<Vec<u8>, MediaIntegrityServiceError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() == 0
        || metadata.len() > MAX_COVER_RESPONSE_BYTES
    {
        return Err(MediaIntegrityServiceError::MediaUnavailable);
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)
        .map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?
        .take(MAX_COVER_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| MediaIntegrityServiceError::MediaUnavailable)?;
    if bytes.len() as u64 > MAX_COVER_RESPONSE_BYTES || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(MediaIntegrityServiceError::MediaUnavailable);
    }
    Ok(bytes)
}

fn remove_managed_cover_best_effort(root: &Path, stored_path: &str) {
    if stored_path.starts_with("media/covers/") && !stored_path.contains("..") {
        let _ = fs::remove_file(root.join(stored_path));
    }
}

fn now() -> Result<i64, MediaIntegrityServiceError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| MediaIntegrityServiceError::ProcessingFailed)?
        .as_millis();
    i64::try_from(millis).map_err(|_| MediaIntegrityServiceError::ProcessingFailed)
}

fn map_repository_error(error: MediaIntegrityRepositoryError) -> MediaIntegrityServiceError {
    match error {
        MediaIntegrityRepositoryError::NotFound => MediaIntegrityServiceError::NotFound,
        MediaIntegrityRepositoryError::Conflict => MediaIntegrityServiceError::Conflict,
        MediaIntegrityRepositoryError::InvalidData => MediaIntegrityServiceError::InvalidInput,
        MediaIntegrityRepositoryError::DatabaseFailed => {
            MediaIntegrityServiceError::DatabaseUnavailable
        }
    }
}

fn map_database_error(_: DatabaseServiceError) -> MediaIntegrityServiceError {
    MediaIntegrityServiceError::DatabaseUnavailable
}

fn map_access_error(error: AccessModeServiceError) -> MediaIntegrityServiceError {
    if error == AccessModeServiceError::ReadOnly {
        MediaIntegrityServiceError::ReadOnly
    } else {
        MediaIntegrityServiceError::DatabaseUnavailable
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        fs::File,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    use image::{DynamicImage, Rgba, RgbaImage};
    use rusqlite::params;

    use super::{
        MAX_COVER_RESPONSE_BYTES, MediaIntegrityService, MediaIntegrityServiceError,
        PathRepairVerification,
    };
    use crate::{
        adapters::file_hash::{FileHashCancellationToken, hash_reader},
        domain::CoverSourceType,
        repositories::DatabaseRepository,
        services::{DatabaseService, WorkspaceService},
    };

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestWorkspace {
        parent: PathBuf,
        root: PathBuf,
    }

    impl TestWorkspace {
        fn create(label: &str) -> Self {
            let parent = std::env::temp_dir().join(format!(
                "ai-gallery-media-integrity-{label}-{}-{}",
                std::process::id(),
                TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&parent).expect("应能创建测试父目录");
            let root = parent.join("workspace");
            WorkspaceService::new()
                .create_workspace(&root)
                .expect("应能创建工作区");
            DatabaseService::new()
                .prepare_workspace_database(&root)
                .expect("应能准备 v5 数据库");
            Self { parent, root }
        }

        fn connection(&self) -> rusqlite::Connection {
            DatabaseRepository::open(&self.root.join("data/library.sqlite3"))
                .expect("应能打开测试数据库")
        }
    }

    impl Drop for TestWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.parent);
        }
    }

    fn insert_external_video(
        workspace: &TestWorkspace,
        missing_path: &Path,
        content_hash: Option<&str>,
    ) -> i64 {
        let connection = workspace.connection();
        connection
            .execute(
                "INSERT INTO assets(media_type,path_kind,stored_path,file_name,content_hash,created_at,updated_at)
                 VALUES('video','external',?1,'missing.mp4',?2,1,1)",
                params![missing_path.to_string_lossy(), content_hash],
            )
            .expect("应能创建外部视频记录");
        connection.last_insert_rowid()
    }

    #[test]
    fn path_repair_requires_hash_match_or_explicit_unverified_confirmation() {
        let workspace = TestWorkspace::create("repair");
        let candidate = workspace.parent.join("candidate.mp4");
        fs::write(&candidate, b"same video bytes").expect("应能写入候选视频");
        let hash = hash_reader(
            File::open(&candidate).expect("应能打开候选文件"),
            &FileHashCancellationToken::default(),
        )
        .expect("应能计算候选哈希");
        let matched_id = insert_external_video(
            &workspace,
            &workspace.parent.join("missing-matched.mp4"),
            Some(&hash),
        );
        let service = MediaIntegrityService::new();
        let preview = service
            .preview_asset_path_repair(&workspace.root, matched_id, &candidate)
            .expect("相同哈希应能预览修复");
        assert_eq!(preview.verification, PathRepairVerification::Matched);
        service
            .execute_asset_path_repair(&workspace.root, matched_id, &candidate, false)
            .expect("相同哈希不需要额外确认");

        let unverified_id = insert_external_video(
            &workspace,
            &workspace.parent.join("missing-unverified.mp4"),
            None,
        );
        let unverified_candidate = workspace.parent.join("unverified-candidate.mp4");
        fs::write(&unverified_candidate, b"unverified video bytes")
            .expect("应能写入未验证候选视频");
        let unverified = service
            .preview_asset_path_repair(&workspace.root, unverified_id, &unverified_candidate)
            .expect("无历史哈希时应返回未验证预览");
        assert_eq!(unverified.verification, PathRepairVerification::Unverified);
        assert_eq!(
            service.execute_asset_path_repair(
                &workspace.root,
                unverified_id,
                &unverified_candidate,
                false,
            ),
            Err(MediaIntegrityServiceError::Conflict)
        );
        service
            .execute_asset_path_repair(&workspace.root, unverified_id, &unverified_candidate, true)
            .expect("明确允许后应写入当前哈希");
    }

    #[test]
    fn custom_cover_is_converted_to_managed_png_and_removal_preserves_video() {
        let workspace = TestWorkspace::create("custom-cover");
        let missing_video = workspace.parent.join("missing-video.mp4");
        let asset_id = insert_external_video(&workspace, &missing_video, None);
        let source = workspace.parent.join("cover.jpg");
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(16, 16, Rgba([1, 2, 3, 255])))
            .save_with_format(&source, image::ImageFormat::Jpeg)
            .expect("应能写入 JPEG 封面");

        let service = MediaIntegrityService::new();
        let cover = service
            .set_custom_video_cover(&workspace.root, asset_id, &source)
            .expect("应能设置自定义封面");
        assert_eq!(cover.cover.asset_id, asset_id);
        assert!(cover.bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        service
            .remove_video_cover(&workspace.root, asset_id)
            .expect("应能移除封面");
        assert!(
            service
                .get_asset_cover(&workspace.root, asset_id)
                .expect("应能读取封面状态")
                .is_none()
        );
        let asset_exists: bool = workspace
            .connection()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM assets WHERE id=?1)",
                [asset_id],
                |row| row.get(0),
            )
            .expect("应能检查原视频记录");
        assert!(asset_exists, "移除封面不得删除视频记录");
    }

    #[test]
    fn oversized_new_cover_is_rejected_before_database_change() {
        let workspace = TestWorkspace::create("oversized-cover");
        let asset_id = insert_external_video(
            &workspace,
            &workspace.parent.join("missing-video.mp4"),
            None,
        );
        let old_path = workspace.root.join("media/covers/old.png");
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(8, 8, Rgba([1, 2, 3, 255])))
            .save_with_format(&old_path, image::ImageFormat::Png)
            .expect("应能写入旧封面");
        workspace
            .connection()
            .execute(
                "INSERT INTO asset_covers(asset_id,source_type,stored_path,frame_timestamp_ms,created_at,updated_at)
                 VALUES(?1,'custom','media/covers/old.png',NULL,1,1)",
                [asset_id],
            )
            .expect("应能写入旧封面元数据");
        let oversized = workspace.root.join("media/covers/oversized.png");
        let mut bytes = vec![0_u8; MAX_COVER_RESPONSE_BYTES as usize + 1];
        bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        fs::write(&oversized, bytes).expect("应能写入超限测试文件");

        assert_eq!(
            MediaIntegrityService::new().commit_cover(
                &workspace.root,
                asset_id,
                CoverSourceType::Custom,
                None,
                &oversized,
            ),
            Err(MediaIntegrityServiceError::MediaUnavailable)
        );
        let stored_path: String = workspace
            .connection()
            .query_row(
                "SELECT stored_path FROM asset_covers WHERE asset_id=?1",
                [asset_id],
                |row| row.get(0),
            )
            .expect("旧封面元数据必须保留");
        assert_eq!(stored_path, "media/covers/old.png");
        assert!(old_path.exists());
        assert!(!oversized.exists());
    }
}
