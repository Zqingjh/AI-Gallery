use std::{
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{
    commands::{CommandErrorDto, import::ImportCommandState},
    domain::{AssetCover, CoverSourceType, PathKind},
    services::{
        CoverContent, KeyFramePreview, MediaIntegrityService, MediaIntegrityServiceError,
        PathRepairPreview, PathRepairVerification,
    },
};

const MAX_CONCURRENT_MEDIA_TASKS: usize = 2;

#[derive(Default)]
pub(crate) struct MediaIntegrityCommandState {
    active_tasks: AtomicUsize,
}

struct MediaTaskPermit<'a> {
    state: &'a MediaIntegrityCommandState,
}

impl Drop for MediaTaskPermit<'_> {
    fn drop(&mut self) {
        self.state.active_tasks.fetch_sub(1, Ordering::AcqRel);
    }
}

impl MediaIntegrityCommandState {
    fn try_acquire(&self) -> Result<MediaTaskPermit<'_>, CommandErrorDto> {
        self.active_tasks
            .try_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                (active < MAX_CONCURRENT_MEDIA_TASKS).then_some(active + 1)
            })
            .map_err(|_| {
                CommandErrorDto::new("MEDIA_TASK_BUSY", "媒体任务已达到并发上限，请稍后重试。")
            })?;
        Ok(MediaTaskPermit { state: self })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AssetRequestDto {
    root_path: String,
    asset_id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct KeyFrameRequestDto {
    root_path: String,
    asset_id: i64,
    frame_timestamp_ms: i64,
    ffmpeg_path: String,
    video_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ConfirmedKeyFrameRequestDto {
    root_path: String,
    asset_id: i64,
    frame_timestamp_ms: i64,
    ffmpeg_path: String,
    video_path: String,
    confirmed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CustomCoverRequestDto {
    root_path: String,
    asset_id: i64,
    cover_path: String,
    confirmed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ConfirmedAssetRequestDto {
    root_path: String,
    asset_id: i64,
    confirmed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PathRepairRequestDto {
    root_path: String,
    asset_id: i64,
    candidate_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ExecutePathRepairRequestDto {
    root_path: String,
    asset_id: i64,
    candidate_path: String,
    allow_unverified: bool,
    confirmed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssetCoverDto {
    asset_id: i64,
    source_type: CoverSourceType,
    frame_timestamp_ms: Option<i64>,
    mime_type: &'static str,
    bytes: Vec<u8>,
}

impl From<CoverContent> for AssetCoverDto {
    fn from(content: CoverContent) -> Self {
        let AssetCover {
            asset_id,
            source_type,
            frame_timestamp_ms,
            ..
        } = content.cover;
        Self {
            asset_id,
            source_type,
            frame_timestamp_ms,
            mime_type: "image/png",
            bytes: content.bytes,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KeyFramePreviewDto {
    asset_id: i64,
    frame_timestamp_ms: i64,
    mime_type: &'static str,
    bytes: Vec<u8>,
}

impl From<KeyFramePreview> for KeyFramePreviewDto {
    fn from(preview: KeyFramePreview) -> Self {
        Self {
            asset_id: preview.asset_id,
            frame_timestamp_ms: preview.frame_timestamp_ms,
            mime_type: "image/png",
            bytes: preview.bytes,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PathRepairPreviewDto {
    asset_id: i64,
    file_name: String,
    path_kind: &'static str,
    file_size: i64,
    verification: &'static str,
}

impl From<PathRepairPreview> for PathRepairPreviewDto {
    fn from(preview: PathRepairPreview) -> Self {
        Self {
            asset_id: preview.asset_id,
            file_name: preview.file_name,
            path_kind: match preview.path_kind {
                PathKind::Managed => "managed",
                PathKind::External => "external",
            },
            file_size: preview.file_size,
            verification: match preview.verification {
                PathRepairVerification::Matched => "matched",
                PathRepairVerification::Unverified => "unverified",
            },
        }
    }
}

impl From<MediaIntegrityServiceError> for CommandErrorDto {
    fn from(error: MediaIntegrityServiceError) -> Self {
        match error {
            MediaIntegrityServiceError::InvalidInput => {
                Self::new("MEDIA_INPUT_INVALID", "媒体完整性输入无效。")
            }
            MediaIntegrityServiceError::NotFound => {
                Self::new("MEDIA_NOT_FOUND", "没有找到对应的媒体记录。")
            }
            MediaIntegrityServiceError::Conflict => {
                Self::new("MEDIA_CONFLICT", "媒体状态已变化或所选文件与作品不匹配。")
            }
            MediaIntegrityServiceError::ReadOnly => {
                Self::new("WORKSPACE_READ_ONLY", "只读模式下不能修改媒体信息。")
            }
            MediaIntegrityServiceError::DatabaseUnavailable => {
                Self::new("DATABASE_OPEN_FAILED", "无法读取工作区数据库。")
            }
            MediaIntegrityServiceError::MediaUnavailable => {
                Self::new("MEDIA_UNAVAILABLE", "无法读取所选媒体文件。")
            }
            MediaIntegrityServiceError::ProcessingFailed => {
                Self::new("MEDIA_PROCESSING_FAILED", "媒体处理失败或超时。")
            }
        }
    }
}

fn require_confirmation(confirmed: bool) -> Result<(), CommandErrorDto> {
    if confirmed {
        Ok(())
    } else {
        Err(CommandErrorDto::new(
            "CONFIRMATION_REQUIRED",
            "请确认后再执行此操作。",
        ))
    }
}

#[tauri::command]
pub(crate) fn get_asset_cover(
    request: AssetRequestDto,
) -> Result<Option<AssetCoverDto>, CommandErrorDto> {
    MediaIntegrityService::new()
        .get_asset_cover(Path::new(&request.root_path), request.asset_id)
        .map(|value| value.map(Into::into))
        .map_err(Into::into)
}

#[tauri::command]
pub(crate) fn preview_video_key_frame(
    state: State<'_, MediaIntegrityCommandState>,
    request: KeyFrameRequestDto,
) -> Result<KeyFramePreviewDto, CommandErrorDto> {
    let _permit = state.try_acquire()?;
    MediaIntegrityService::new()
        .preview_video_key_frame(
            Path::new(&request.root_path),
            request.asset_id,
            request.frame_timestamp_ms,
            Path::new(&request.ffmpeg_path),
            Path::new(&request.video_path),
        )
        .map(Into::into)
        .map_err(Into::into)
}

#[tauri::command]
pub(crate) fn set_video_key_frame_cover(
    state: State<'_, MediaIntegrityCommandState>,
    request: ConfirmedKeyFrameRequestDto,
) -> Result<AssetCoverDto, CommandErrorDto> {
    require_confirmation(request.confirmed)?;
    let _permit = state.try_acquire()?;
    MediaIntegrityService::new()
        .set_video_key_frame_cover(
            Path::new(&request.root_path),
            request.asset_id,
            request.frame_timestamp_ms,
            Path::new(&request.ffmpeg_path),
            Path::new(&request.video_path),
        )
        .map(Into::into)
        .map_err(Into::into)
}

#[tauri::command]
pub(crate) fn set_default_video_cover(
    state: State<'_, MediaIntegrityCommandState>,
    import_state: State<'_, ImportCommandState>,
    request: ConfirmedAssetRequestDto,
) -> Result<AssetCoverDto, CommandErrorDto> {
    require_confirmation(request.confirmed)?;
    let ffmpeg_path = import_state.bundled_ffmpeg_path().ok_or_else(|| {
        CommandErrorDto::new("MEDIA_PROCESSING_UNAVAILABLE", "未配置可用的 FFmpeg。")
    })?;
    let _permit = state.try_acquire()?;
    MediaIntegrityService::new()
        .set_default_video_cover(Path::new(&request.root_path), request.asset_id, ffmpeg_path)
        .map(Into::into)
        .map_err(Into::into)
}

#[tauri::command]
pub(crate) fn set_custom_video_cover(
    state: State<'_, MediaIntegrityCommandState>,
    request: CustomCoverRequestDto,
) -> Result<AssetCoverDto, CommandErrorDto> {
    require_confirmation(request.confirmed)?;
    let _permit = state.try_acquire()?;
    MediaIntegrityService::new()
        .set_custom_video_cover(
            Path::new(&request.root_path),
            request.asset_id,
            Path::new(&request.cover_path),
        )
        .map(Into::into)
        .map_err(Into::into)
}

#[tauri::command]
pub(crate) fn remove_video_cover(request: ConfirmedAssetRequestDto) -> Result<(), CommandErrorDto> {
    require_confirmation(request.confirmed)?;
    MediaIntegrityService::new()
        .remove_video_cover(Path::new(&request.root_path), request.asset_id)
        .map_err(Into::into)
}

#[tauri::command]
pub(crate) fn preview_asset_path_repair(
    state: State<'_, MediaIntegrityCommandState>,
    request: PathRepairRequestDto,
) -> Result<PathRepairPreviewDto, CommandErrorDto> {
    let _permit = state.try_acquire()?;
    MediaIntegrityService::new()
        .preview_asset_path_repair(
            Path::new(&request.root_path),
            request.asset_id,
            Path::new(&request.candidate_path),
        )
        .map(Into::into)
        .map_err(Into::into)
}

#[tauri::command]
pub(crate) fn execute_asset_path_repair(
    state: State<'_, MediaIntegrityCommandState>,
    request: ExecutePathRepairRequestDto,
) -> Result<PathRepairPreviewDto, CommandErrorDto> {
    require_confirmation(request.confirmed)?;
    let _permit = state.try_acquire()?;
    MediaIntegrityService::new()
        .execute_asset_path_repair(
            Path::new(&request.root_path),
            request.asset_id,
            Path::new(&request.candidate_path),
            request.allow_unverified,
        )
        .map(Into::into)
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::{ConfirmedAssetRequestDto, MediaIntegrityCommandState, PathRepairPreviewDto};

    #[test]
    fn write_request_requires_confirmation_and_rejects_unknown_fields() {
        assert!(
            serde_json::from_str::<ConfirmedAssetRequestDto>(
                r#"{"rootPath":"workspace","assetId":1}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<ConfirmedAssetRequestDto>(
                r#"{"rootPath":"workspace","assetId":1,"confirmed":true,"storedPath":"private"}"#
            )
            .is_err()
        );
    }

    #[test]
    fn path_repair_response_contains_no_local_path() {
        let value = serde_json::to_value(PathRepairPreviewDto {
            asset_id: 1,
            file_name: "repaired.mp4".to_owned(),
            path_kind: "external",
            file_size: 100,
            verification: "matched",
        })
        .expect("修复响应应可序列化");
        assert!(value.get("storedPath").is_none());
        assert!(value.get("candidatePath").is_none());
    }

    #[test]
    fn media_task_permits_never_exceed_two() {
        let state = MediaIntegrityCommandState::default();
        let first = state.try_acquire().expect("第一个任务应进入");
        let second = state.try_acquire().expect("第二个任务应进入");
        assert!(state.try_acquire().is_err(), "第三个并发任务必须被拒绝");
        drop(first);
        assert!(state.try_acquire().is_ok(), "释放后应允许新任务进入");
        drop(second);
    }
}
