use std::{
    ffi::c_void,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

use serde::Deserialize;
use tauri::State;

use crate::{
    commands::CommandErrorDto,
    domain::{
        AssetDetail, AssetListQuery, AssetOrderChangeMode, AssetSearchField, AssetSummary,
        BulkAssetEditInput, BulkAssetEditPreview, BulkNullableTextEdit, CanvasMemberRole,
        CanvasProjectMember, Category, CategoryDeleteAction, CategoryInput, CreateAsset,
        CreateProject, Dimension, DimensionInput, DuplicateAssetGroupPage, DuplicateGroupCursor,
        MediaType, MetadataPresetKind, MetadataPresets, ModelComparisonItem, ModelComparisonQuery,
        ModelComparisonScope, NumberedAssetPage, Page, PageCursor, ProjectDetail, ProjectSummary,
        PromptText, RelationImpact, Tag, TrashEntry, UpdateAsset, UpdateProject,
    },
    services::{
        ExportSelectionRequest, ExportService, ExportServiceError, LibraryService,
        LibraryServiceError, MediaIntegrityService, ThumbnailResponse, ThumbnailService,
        ThumbnailSource, WorkspaceService,
    },
};

const MAX_PREVIEW_BYTES: u64 = 25 * 1024 * 1024;

#[derive(Clone, Default)]
pub(crate) struct ThumbnailCommandState {
    service: ThumbnailService,
}

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

#[cfg(windows)]
fn ensure_managed_preview_handle(
    file: &File,
    trusted_managed_root: &Path,
) -> Result<(), CommandErrorDto> {
    let mut buffer = vec![0_u16; 32_768];
    // 文件已经打开；此处校验句柄最终指向，避免路径在预检与打开之间被替换。
    let length = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            0,
        )
    };
    if length == 0 || length as usize >= buffer.len() {
        return Err(preview_unavailable());
    }
    let opened =
        String::from_utf16(&buffer[..length as usize]).map_err(|_| preview_unavailable())?;
    let opened = normalize_windows_final_path(&opened);
    let managed_root = normalize_windows_final_path(&trusted_managed_root.to_string_lossy());
    let boundary = format!("{managed_root}\\");
    if opened != managed_root && !opened.starts_with(&boundary) {
        return Err(preview_unavailable());
    }
    Ok(())
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
fn ensure_managed_preview_handle(
    _file: &File,
    trusted_managed_root: &Path,
) -> Result<(), CommandErrorDto> {
    // P0 发布目标为 Windows；非 Windows 构建仍要求调用方提供已冻结的绝对边界。
    if trusted_managed_root.is_absolute() {
        Ok(())
    } else {
        Err(preview_unavailable())
    }
}

fn preview_unavailable() -> CommandErrorDto {
    CommandErrorDto::new("MEDIA_PREVIEW_UNAVAILABLE", "无法读取媒体预览。")
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RootRequestDto {
    root_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PageRequestDto {
    root_path: String,
    cursor: Option<PageCursor>,
    limit: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EntityRequestDto {
    root_path: String,
    id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BatchEntityRequestDto {
    root_path: String,
    ids: Vec<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProjectWriteRequestDto {
    root_path: String,
    id: Option<i64>,
    input: CreateProject,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProjectAssetAssignmentRequestDto {
    root_path: String,
    project_id: i64,
    display_numbers: Vec<i64>,
    confirmed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProjectAssetRemovalRequestDto {
    root_path: String,
    project_id: i64,
    asset_ids: Vec<i64>,
    confirmed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CanvasMemberPageRequestDto {
    root_path: String,
    project_id: i64,
    cursor: Option<PageCursor>,
    limit: u32,
    role: Option<CanvasMemberRole>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CanvasMemberWriteRequestDto {
    root_path: String,
    project_id: i64,
    asset_id: i64,
    role: CanvasMemberRole,
    reference_name: Option<String>,
    confirmed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CanvasPromptTextDto {
    prompt_zh: String,
    prompt_en: String,
    negative_prompt: String,
}

impl From<CanvasPromptTextDto> for PromptText {
    fn from(value: CanvasPromptTextDto) -> Self {
        Self {
            prompt_zh: value.prompt_zh,
            prompt_en: value.prompt_en,
            negative_prompt: value.negative_prompt,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CanvasOutputPromptWriteRequestDto {
    root_path: String,
    project_id: i64,
    asset_id: i64,
    prompt: CanvasPromptTextDto,
    confirmed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AssetPageRequestDto {
    root_path: String,
    project_id: Option<i64>,
    media_type: Option<MediaType>,
    keyword: Option<String>,
    #[serde(default)]
    search_field: AssetSearchField,
    #[serde(default)]
    exact_match: bool,
    model: Option<String>,
    platform: Option<String>,
    #[serde(default)]
    category_ids: Vec<i64>,
    rating: Option<u8>,
    is_favorite: Option<bool>,
    is_public: Option<bool>,
    created_after: Option<i64>,
    created_before: Option<i64>,
    min_aspect_ratio: Option<f64>,
    max_aspect_ratio: Option<f64>,
    cursor: Option<PageCursor>,
    limit: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NumberedAssetPageRequestDto {
    root_path: String,
    project_id: Option<i64>,
    media_type: Option<MediaType>,
    keyword: Option<String>,
    #[serde(default)]
    search_field: AssetSearchField,
    #[serde(default)]
    exact_match: bool,
    model: Option<String>,
    platform: Option<String>,
    #[serde(default)]
    category_ids: Vec<i64>,
    rating: Option<u8>,
    is_favorite: Option<bool>,
    is_public: Option<bool>,
    created_after: Option<i64>,
    created_before: Option<i64>,
    min_aspect_ratio: Option<f64>,
    max_aspect_ratio: Option<f64>,
    page: u32,
    page_size: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DuplicateGroupPageRequestDto {
    root_path: String,
    cursor: Option<DuplicateGroupCursor>,
    limit: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BulkAssetEditPreviewRequestDto {
    root_path: String,
    input: BulkAssetEditInputDto,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BulkAssetEditRequestDto {
    root_path: String,
    input: BulkAssetEditInputDto,
    confirmed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BulkAssetEditInputDto {
    asset_ids: Vec<i64>,
    rating: Option<u8>,
    is_favorite: Option<bool>,
    is_public: Option<bool>,
    #[serde(default)]
    model: BulkNullableTextEditDto,
    #[serde(default)]
    platform: BulkNullableTextEditDto,
    add_category_ids: Vec<i64>,
    remove_category_ids: Vec<i64>,
    add_tag_ids: Vec<i64>,
    remove_tag_ids: Vec<i64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum BulkNullableTextEditDto {
    #[default]
    Keep,
    Set {
        value: String,
    },
    Clear,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ModelComparisonRequestDto {
    root_path: String,
    scope: ModelComparisonScopeDto,
    cursor: Option<ModelComparisonCursorDto>,
    limit: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ModelComparisonCursorDto {
    updated_at: i64,
    id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum ModelComparisonScopeDto {
    Project { project_id: i64 },
    MatchingPrompt { baseline_asset_id: i64 },
}

impl From<BulkNullableTextEditDto> for BulkNullableTextEdit {
    fn from(value: BulkNullableTextEditDto) -> Self {
        match value {
            BulkNullableTextEditDto::Keep => Self::Keep,
            BulkNullableTextEditDto::Set { value } => Self::Set(value),
            BulkNullableTextEditDto::Clear => Self::Clear,
        }
    }
}

impl From<BulkAssetEditInputDto> for BulkAssetEditInput {
    fn from(value: BulkAssetEditInputDto) -> Self {
        Self {
            asset_ids: value.asset_ids,
            rating: value.rating,
            is_favorite: value.is_favorite,
            is_public: value.is_public,
            model: value.model.into(),
            platform: value.platform.into(),
            add_category_ids: value.add_category_ids,
            remove_category_ids: value.remove_category_ids,
            add_tag_ids: value.add_tag_ids,
            remove_tag_ids: value.remove_tag_ids,
        }
    }
}

impl From<ModelComparisonScopeDto> for ModelComparisonScope {
    fn from(value: ModelComparisonScopeDto) -> Self {
        match value {
            ModelComparisonScopeDto::Project { project_id } => Self::Project { project_id },
            ModelComparisonScopeDto::MatchingPrompt { baseline_asset_id } => {
                Self::MatchingPrompt { baseline_asset_id }
            }
        }
    }
}

impl From<ModelComparisonCursorDto> for PageCursor {
    fn from(value: ModelComparisonCursorDto) -> Self {
        Self {
            updated_at: value.updated_at,
            id: value.id,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AssetWriteRequestDto {
    root_path: String,
    id: i64,
    input: UpdateAsset,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AssetCreateRequestDto {
    root_path: String,
    input: CreateAsset,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AssetDisplayOrderRequestDto {
    root_path: String,
    id: i64,
    target_position: i64,
    mode: AssetOrderChangeMode,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RelationWriteRequestDto {
    root_path: String,
    id: i64,
    ids: Vec<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DimensionWriteRequestDto {
    root_path: String,
    id: Option<i64>,
    input: DimensionInput,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CategoryListRequestDto {
    root_path: String,
    dimension_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CategoryWriteRequestDto {
    root_path: String,
    id: Option<i64>,
    input: CategoryInput,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CategoryDeleteRequestDto {
    root_path: String,
    id: i64,
    replacement_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TagWriteRequestDto {
    root_path: String,
    id: Option<i64>,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MetadataPresetWriteRequestDto {
    root_path: String,
    kind: MetadataPresetKind,
    id: Option<i64>,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MetadataPresetDeleteRequestDto {
    root_path: String,
    kind: MetadataPresetKind,
    id: i64,
}

impl From<LibraryServiceError> for CommandErrorDto {
    fn from(error: LibraryServiceError) -> Self {
        match error {
            LibraryServiceError::InvalidInput => {
                Self::new("LIBRARY_INVALID_INPUT", "提交的作品库数据无法识别。")
            }
            LibraryServiceError::NotFound => {
                Self::new("LIBRARY_NOT_FOUND", "未找到指定的作品库记录。")
            }
            LibraryServiceError::Conflict => {
                Self::new("LIBRARY_CONFLICT", "该名称或媒体记录已存在。")
            }
            LibraryServiceError::DatabaseUnavailable => {
                Self::new("LIBRARY_DATABASE_UNAVAILABLE", "作品库数据库暂时不可用。")
            }
            LibraryServiceError::DataInvalid => {
                Self::new("LIBRARY_DATA_INVALID", "作品库数据不完整或已损坏。")
            }
            LibraryServiceError::ReadOnly => Self::new(
                "WORKSPACE_READ_ONLY",
                "工作区正处于只读展示模式，无法修改内容。",
            ),
        }
    }
}

impl From<ExportServiceError> for CommandErrorDto {
    fn from(error: ExportServiceError) -> Self {
        match error {
            ExportServiceError::InvalidInput => {
                Self::new("EXPORT_INVALID_INPUT", "请选择有效的作品或项目后再导出。")
            }
            ExportServiceError::SourceUnavailable => Self::new(
                "EXPORT_SOURCE_UNAVAILABLE",
                "部分作品原文件不可读取，导出已取消。",
            ),
            ExportServiceError::DestinationUnavailable => Self::new(
                "EXPORT_DESTINATION_UNAVAILABLE",
                "无法在所选位置创建导出文件。",
            ),
            ExportServiceError::Library(error) => Self::from(error),
            ExportServiceError::CustomFields(_) => Self::new(
                "EXPORT_SOURCE_UNAVAILABLE",
                "作品库或作品扩展字段当前不可读取，导出已取消。",
            ),
            ExportServiceError::Workspace(_) => {
                Self::new("EXPORT_SOURCE_UNAVAILABLE", "作品库或原文件当前不可读取。")
            }
        }
    }
}

#[tauri::command]
pub(crate) fn library_list_projects(
    request: PageRequestDto,
) -> Result<Page<ProjectSummary>, CommandErrorDto> {
    LibraryService::new()
        .list_projects(Path::new(&request.root_path), request.cursor, request.limit)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_get_project(
    request: EntityRequestDto,
) -> Result<ProjectDetail, CommandErrorDto> {
    LibraryService::new()
        .get_project(Path::new(&request.root_path), request.id)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_create_project(
    request: ProjectWriteRequestDto,
) -> Result<ProjectDetail, CommandErrorDto> {
    LibraryService::new()
        .create_project(Path::new(&request.root_path), &request.input)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_update_project(
    request: ProjectWriteRequestDto,
) -> Result<ProjectDetail, CommandErrorDto> {
    let id = request
        .id
        .ok_or_else(|| CommandErrorDto::from(LibraryServiceError::InvalidInput))?;
    LibraryService::new()
        .update_project(
            Path::new(&request.root_path),
            id,
            &request.input as &UpdateProject,
        )
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_assign_assets_to_project(
    request: ProjectAssetAssignmentRequestDto,
) -> Result<u32, CommandErrorDto> {
    if !request.confirmed {
        return Err(CommandErrorDto::from(LibraryServiceError::InvalidInput));
    }
    LibraryService::new()
        .assign_assets_to_project(
            Path::new(&request.root_path),
            request.project_id,
            &request.display_numbers,
        )
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_remove_assets_from_project(
    request: ProjectAssetRemovalRequestDto,
) -> Result<u32, CommandErrorDto> {
    if !request.confirmed {
        return Err(CommandErrorDto::from(LibraryServiceError::InvalidInput));
    }
    LibraryService::new()
        .remove_assets_from_project(
            Path::new(&request.root_path),
            request.project_id,
            &request.asset_ids,
        )
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_list_canvas_members(
    request: CanvasMemberPageRequestDto,
) -> Result<Page<CanvasProjectMember>, CommandErrorDto> {
    LibraryService::new()
        .list_canvas_members(
            Path::new(&request.root_path),
            request.project_id,
            request.cursor,
            request.limit,
            request.role,
        )
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_set_canvas_member(
    request: CanvasMemberWriteRequestDto,
) -> Result<CanvasProjectMember, CommandErrorDto> {
    if !request.confirmed {
        return Err(CommandErrorDto::from(LibraryServiceError::InvalidInput));
    }
    LibraryService::new()
        .set_canvas_member(
            Path::new(&request.root_path),
            request.project_id,
            request.asset_id,
            request.role,
            request.reference_name.as_deref(),
        )
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_update_canvas_output_prompt(
    request: CanvasOutputPromptWriteRequestDto,
) -> Result<CanvasProjectMember, CommandErrorDto> {
    if !request.confirmed {
        return Err(CommandErrorDto::from(LibraryServiceError::InvalidInput));
    }
    LibraryService::new()
        .update_canvas_output_prompt(
            Path::new(&request.root_path),
            request.project_id,
            request.asset_id,
            &request.prompt.into(),
        )
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_list_assets(
    request: AssetPageRequestDto,
) -> Result<Page<AssetSummary>, CommandErrorDto> {
    LibraryService::new()
        .list_assets(
            Path::new(&request.root_path),
            AssetListQuery {
                project_id: request.project_id,
                media_type: request.media_type,
                keyword: request.keyword,
                search_field: request.search_field,
                exact_match: request.exact_match,
                model: request.model,
                platform: request.platform,
                category_ids: request.category_ids,
                rating: request.rating,
                is_favorite: request.is_favorite,
                is_public: request.is_public,
                created_after: request.created_after,
                created_before: request.created_before,
                min_aspect_ratio: request.min_aspect_ratio,
                max_aspect_ratio: request.max_aspect_ratio,
                cursor: request.cursor,
                limit: request.limit,
            },
        )
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_list_assets_numbered(
    request: NumberedAssetPageRequestDto,
) -> Result<NumberedAssetPage, CommandErrorDto> {
    LibraryService::new()
        .list_assets_numbered(
            Path::new(&request.root_path),
            AssetListQuery {
                project_id: request.project_id,
                media_type: request.media_type,
                keyword: request.keyword,
                search_field: request.search_field,
                exact_match: request.exact_match,
                model: request.model,
                platform: request.platform,
                category_ids: request.category_ids,
                rating: request.rating,
                is_favorite: request.is_favorite,
                is_public: request.is_public,
                created_after: request.created_after,
                created_before: request.created_before,
                min_aspect_ratio: request.min_aspect_ratio,
                max_aspect_ratio: request.max_aspect_ratio,
                cursor: None,
                limit: request.page_size,
            },
            request.page,
            request.page_size,
        )
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_preview_bulk_asset_edit(
    request: BulkAssetEditPreviewRequestDto,
) -> Result<BulkAssetEditPreview, CommandErrorDto> {
    let input = request.input.into();
    LibraryService::new()
        .preview_bulk_asset_edit(Path::new(&request.root_path), &input)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_bulk_edit_assets(
    request: BulkAssetEditRequestDto,
) -> Result<BulkAssetEditPreview, CommandErrorDto> {
    if !request.confirmed {
        return Err(CommandErrorDto::from(LibraryServiceError::InvalidInput));
    }
    let input = request.input.into();
    LibraryService::new()
        .bulk_edit_assets(Path::new(&request.root_path), &input)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_list_model_comparison(
    request: ModelComparisonRequestDto,
) -> Result<Page<ModelComparisonItem>, CommandErrorDto> {
    LibraryService::new()
        .list_model_comparison(
            Path::new(&request.root_path),
            ModelComparisonQuery {
                scope: request.scope.into(),
                cursor: request.cursor.map(Into::into),
                limit: request.limit,
            },
        )
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_list_duplicate_groups(
    request: DuplicateGroupPageRequestDto,
) -> Result<DuplicateAssetGroupPage, CommandErrorDto> {
    LibraryService::new()
        .list_duplicate_groups(Path::new(&request.root_path), request.cursor, request.limit)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_get_asset(request: EntityRequestDto) -> Result<AssetDetail, CommandErrorDto> {
    LibraryService::new()
        .get_asset(Path::new(&request.root_path), request.id)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_read_asset_preview(
    request: EntityRequestDto,
) -> Result<tauri::ipc::Response, CommandErrorDto> {
    let root = Path::new(&request.root_path);
    let asset = LibraryService::new()
        .get_asset(root, request.id)
        .map_err(CommandErrorDto::from)?;
    let stored_path = PathBuf::from(&asset.summary.media.stored_path);
    let path = match asset.summary.media.path_kind {
        crate::domain::PathKind::Managed => root.join(&stored_path),
        crate::domain::PathKind::External => stored_path,
    };
    let kind = match asset.summary.media.path_kind {
        crate::domain::PathKind::Managed => crate::domain::StoredPathKind::Managed,
        crate::domain::PathKind::External => crate::domain::StoredPathKind::External,
    };
    // 在任何候选文件校验和打开前冻结可信根，避免 media 目录在竞态中被整体替换。
    let trusted_managed_root = matches!(
        asset.summary.media.path_kind,
        crate::domain::PathKind::Managed
    )
    .then(|| root.join("media").canonicalize())
    .transpose()
    .map_err(|_| preview_unavailable())?;
    WorkspaceService::new()
        .check_stored_path(root, kind, Path::new(&asset.summary.media.stored_path))
        .map_err(|_| CommandErrorDto::new("MEDIA_PREVIEW_UNAVAILABLE", "无法读取媒体预览。"))?;
    // 校验后只打开一次，后续长度检查与读取都针对同一文件句柄。
    let file = File::open(path).map_err(|_| preview_unavailable())?;
    if matches!(
        asset.summary.media.path_kind,
        crate::domain::PathKind::Managed
    ) {
        ensure_managed_preview_handle(
            &file,
            trusted_managed_root
                .as_deref()
                .ok_or_else(preview_unavailable)?,
        )?;
    }
    let metadata = file
        .metadata()
        .map_err(|_| CommandErrorDto::new("MEDIA_PREVIEW_UNAVAILABLE", "无法读取媒体预览。"))?;
    if !metadata.is_file() || metadata.len() > MAX_PREVIEW_BYTES {
        return Err(CommandErrorDto::new(
            "MEDIA_PREVIEW_TOO_LARGE",
            "媒体文件过大，无法在详情中直接预览。",
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_PREVIEW_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| CommandErrorDto::new("MEDIA_PREVIEW_UNAVAILABLE", "无法读取媒体预览。"))?;
    if bytes.len() as u64 > MAX_PREVIEW_BYTES {
        return Err(CommandErrorDto::new(
            "MEDIA_PREVIEW_TOO_LARGE",
            "媒体文件过大，无法在详情中直接预览。",
        ));
    }
    Ok(tauri::ipc::Response::new(bytes))
}

/// 缩略图仅接收资产 ID；原始路径始终由后端从数据库记录中解析。
#[tauri::command]
pub(crate) fn library_read_asset_thumbnail(
    state: State<'_, ThumbnailCommandState>,
    request: EntityRequestDto,
) -> Result<ThumbnailResponse, CommandErrorDto> {
    let root = Path::new(&request.root_path);
    let asset = LibraryService::new()
        .get_asset(root, request.id)
        .map_err(CommandErrorDto::from)?;
    if asset.summary.media.media_type == MediaType::Video {
        let cover = MediaIntegrityService::new()
            .get_asset_cover(root, request.id)
            .map_err(CommandErrorDto::from)?;
        return Ok(match cover {
            Some(content) => ThumbnailResponse::Ready {
                bytes: content.bytes,
                mime_type: "image/png",
            },
            None => ThumbnailResponse::Unavailable,
        });
    }
    Ok(state.service.read_or_enqueue(ThumbnailSource {
        workspace_root: root.to_path_buf(),
        asset_id: asset.summary.id,
        media_type: asset.summary.media.media_type,
        path_kind: asset.summary.media.path_kind,
        stored_path: asset.summary.media.stored_path,
        content_hash: asset.summary.media.content_hash,
    }))
}

#[tauri::command]
pub(crate) async fn library_export_selection(
    request: ExportSelectionRequest,
) -> Result<String, CommandErrorDto> {
    tauri::async_runtime::spawn_blocking(move || ExportService::new().export_selection(request))
        .await
        .map_err(|_| CommandErrorDto::new("EXPORT_FAILED", "导出任务意外中断。"))?
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_create_asset(
    request: AssetCreateRequestDto,
) -> Result<AssetDetail, CommandErrorDto> {
    LibraryService::new()
        .create_asset(Path::new(&request.root_path), &request.input)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_update_asset(
    request: AssetWriteRequestDto,
) -> Result<AssetDetail, CommandErrorDto> {
    LibraryService::new()
        .update_asset(Path::new(&request.root_path), request.id, &request.input)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_update_asset_display_order(
    request: AssetDisplayOrderRequestDto,
) -> Result<AssetDetail, CommandErrorDto> {
    LibraryService::new()
        .update_asset_display_order(
            Path::new(&request.root_path),
            request.id,
            request.target_position,
            request.mode,
        )
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_set_project_categories(
    request: RelationWriteRequestDto,
) -> Result<ProjectDetail, CommandErrorDto> {
    LibraryService::new()
        .set_project_categories(Path::new(&request.root_path), request.id, &request.ids)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_set_project_tags(
    request: RelationWriteRequestDto,
) -> Result<ProjectDetail, CommandErrorDto> {
    LibraryService::new()
        .set_project_tags(Path::new(&request.root_path), request.id, &request.ids)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_set_asset_categories(
    request: RelationWriteRequestDto,
) -> Result<AssetDetail, CommandErrorDto> {
    LibraryService::new()
        .set_asset_categories(Path::new(&request.root_path), request.id, &request.ids)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_set_asset_tags(
    request: RelationWriteRequestDto,
) -> Result<AssetDetail, CommandErrorDto> {
    LibraryService::new()
        .set_asset_tags(Path::new(&request.root_path), request.id, &request.ids)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_list_metadata_presets(
    request: RootRequestDto,
) -> Result<MetadataPresets, CommandErrorDto> {
    LibraryService::new()
        .list_metadata_presets(Path::new(&request.root_path))
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_write_metadata_preset(
    request: MetadataPresetWriteRequestDto,
) -> Result<i64, CommandErrorDto> {
    let service = LibraryService::new();
    if let Some(id) = request.id {
        service
            .update_metadata_preset(
                Path::new(&request.root_path),
                request.kind,
                id,
                &request.name,
            )
            .map(|()| id)
            .map_err(CommandErrorDto::from)
    } else {
        service
            .create_metadata_preset(Path::new(&request.root_path), request.kind, &request.name)
            .map_err(CommandErrorDto::from)
    }
}

#[tauri::command]
pub(crate) fn library_delete_metadata_preset(
    request: MetadataPresetDeleteRequestDto,
) -> Result<(), CommandErrorDto> {
    LibraryService::new()
        .delete_metadata_preset(Path::new(&request.root_path), request.kind, request.id)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_list_dimensions(
    request: RootRequestDto,
) -> Result<Vec<Dimension>, CommandErrorDto> {
    LibraryService::new()
        .list_dimensions(Path::new(&request.root_path))
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_write_dimension(
    request: DimensionWriteRequestDto,
) -> Result<i64, CommandErrorDto> {
    let service = LibraryService::new();
    if let Some(id) = request.id {
        service
            .update_dimension(Path::new(&request.root_path), id, &request.input)
            .map(|()| id)
            .map_err(CommandErrorDto::from)
    } else {
        service
            .create_dimension(Path::new(&request.root_path), &request.input)
            .map_err(CommandErrorDto::from)
    }
}

#[tauri::command]
pub(crate) fn library_delete_dimension(request: EntityRequestDto) -> Result<(), CommandErrorDto> {
    LibraryService::new()
        .delete_dimension(Path::new(&request.root_path), request.id)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_list_categories(
    request: CategoryListRequestDto,
) -> Result<Vec<Category>, CommandErrorDto> {
    LibraryService::new()
        .list_categories(Path::new(&request.root_path), request.dimension_id)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_write_category(
    request: CategoryWriteRequestDto,
) -> Result<i64, CommandErrorDto> {
    let service = LibraryService::new();
    if let Some(id) = request.id {
        service
            .update_category(Path::new(&request.root_path), id, &request.input)
            .map(|()| id)
            .map_err(CommandErrorDto::from)
    } else {
        service
            .create_category(Path::new(&request.root_path), &request.input)
            .map_err(CommandErrorDto::from)
    }
}

#[tauri::command]
pub(crate) fn library_category_impact(
    request: EntityRequestDto,
) -> Result<RelationImpact, CommandErrorDto> {
    LibraryService::new()
        .category_impact(Path::new(&request.root_path), request.id)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_delete_category(
    request: CategoryDeleteRequestDto,
) -> Result<RelationImpact, CommandErrorDto> {
    let action = request.replacement_id.map_or(
        CategoryDeleteAction::Remove,
        CategoryDeleteAction::ReplaceWith,
    );
    LibraryService::new()
        .delete_category(Path::new(&request.root_path), request.id, action)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_list_tags(request: RootRequestDto) -> Result<Vec<Tag>, CommandErrorDto> {
    LibraryService::new()
        .list_tags(Path::new(&request.root_path))
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_write_tag(request: TagWriteRequestDto) -> Result<i64, CommandErrorDto> {
    let service = LibraryService::new();
    if let Some(id) = request.id {
        service
            .update_tag(Path::new(&request.root_path), id, &request.name)
            .map(|()| id)
            .map_err(CommandErrorDto::from)
    } else {
        service
            .create_tag(Path::new(&request.root_path), &request.name)
            .map_err(CommandErrorDto::from)
    }
}

#[tauri::command]
pub(crate) fn library_delete_tag(
    request: EntityRequestDto,
) -> Result<RelationImpact, CommandErrorDto> {
    LibraryService::new()
        .delete_tag(Path::new(&request.root_path), request.id)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_move_project_to_trash(
    request: EntityRequestDto,
) -> Result<i64, CommandErrorDto> {
    LibraryService::new()
        .move_project_to_trash(Path::new(&request.root_path), request.id)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_move_asset_to_trash(
    request: EntityRequestDto,
) -> Result<i64, CommandErrorDto> {
    LibraryService::new()
        .move_asset_to_trash(Path::new(&request.root_path), request.id)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_move_assets_to_trash(
    request: BatchEntityRequestDto,
) -> Result<Vec<i64>, CommandErrorDto> {
    LibraryService::new()
        .move_assets_to_trash(Path::new(&request.root_path), &request.ids)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_list_trash(
    request: PageRequestDto,
) -> Result<Page<TrashEntry>, CommandErrorDto> {
    LibraryService::new()
        .list_trash(Path::new(&request.root_path), request.cursor, request.limit)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_restore_trash(
    request: EntityRequestDto,
) -> Result<TrashEntry, CommandErrorDto> {
    LibraryService::new()
        .restore_trash(Path::new(&request.root_path), request.id)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_restore_trash_batch(
    request: BatchEntityRequestDto,
) -> Result<(), CommandErrorDto> {
    LibraryService::new()
        .restore_trash_batch(Path::new(&request.root_path), &request.ids)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_undo_recent_delete(
    request: RootRequestDto,
) -> Result<TrashEntry, CommandErrorDto> {
    LibraryService::new()
        .undo_recent_delete(Path::new(&request.root_path))
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_purge_trash_record(request: EntityRequestDto) -> Result<(), CommandErrorDto> {
    LibraryService::new()
        .purge_trash_record(Path::new(&request.root_path), request.id)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn library_purge_trash_records(
    request: BatchEntityRequestDto,
) -> Result<(), CommandErrorDto> {
    LibraryService::new()
        .purge_trash_records(Path::new(&request.root_path), &request.ids)
        .map_err(CommandErrorDto::from)
}

#[cfg(all(test, windows))]
mod preview_handle_tests {
    use std::{fs, fs::File, os::windows::fs::symlink_dir};

    use super::ensure_managed_preview_handle;

    #[test]
    fn opened_handle_must_resolve_inside_managed_media_root() {
        let base =
            std::env::temp_dir().join(format!("ai-gallery-preview-handle-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("workspace");
        let media = root.join("media/images");
        fs::create_dir_all(&media).expect("应能创建受管媒体目录");
        let inside = media.join("inside.png");
        let outside = base.join("outside.png");
        fs::write(&inside, b"inside").expect("应能写入受管文件");
        fs::write(&outside, b"outside").expect("应能写入外部文件");
        let trusted_managed_root = root
            .join("media")
            .canonicalize()
            .expect("受管媒体根应可冻结");

        assert!(
            ensure_managed_preview_handle(
                &File::open(inside).expect("应能打开受管文件"),
                &trusted_managed_root,
            )
            .is_ok()
        );
        assert!(
            ensure_managed_preview_handle(
                &File::open(outside).expect("应能打开外部文件"),
                &trusted_managed_root,
            )
            .is_err(),
            "即使路径预检曾通过，最终句柄指向边界外也必须拒绝"
        );
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn frozen_boundary_rejects_file_after_media_root_replacement() {
        let base = std::env::temp_dir().join(format!(
            "ai-gallery-preview-root-race-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("workspace");
        let media = root.join("media");
        let outside = base.join("outside");
        fs::create_dir_all(&media).expect("应能创建原受管媒体目录");
        fs::create_dir_all(&outside).expect("应能创建边界外目录");
        fs::write(outside.join("raced.png"), b"outside").expect("应能写入边界外文件");

        let trusted_managed_root = media.canonicalize().expect("应能冻结可信媒体根");
        fs::rename(&media, root.join("media-original")).expect("应能移走原媒体根");
        symlink_dir(&outside, &media).expect("测试环境应允许创建目录链接");
        let raced = File::open(media.join("raced.png")).expect("链接后的候选文件应可打开");

        assert!(
            ensure_managed_preview_handle(&raced, &trusted_managed_root).is_err(),
            "候选打开前冻结的可信根不能随目录替换漂移"
        );

        drop(raced);
        fs::remove_dir(&media).expect("应能移除测试目录链接");
        let _ = fs::remove_dir_all(base);
    }
}

#[cfg(test)]
mod read_only_tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::{ProjectWriteRequestDto, library_create_project};
    use crate::{
        domain::{AccessMode, CreateProject, ProjectKind, PromptText},
        services::{AccessModeService, DatabaseService, WorkspaceService},
    };

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "ai-gallery-library-read-only-{}-{}",
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
    fn library_write_command_rejects_read_only_workspace_without_creating_project() {
        let parent = TestDirectory::new();
        let root = parent.0.join("workspace");
        WorkspaceService::new()
            .create_workspace(&root)
            .expect("应能创建工作区");
        DatabaseService::new()
            .prepare_workspace_database(&root)
            .expect("应能准备当前数据库");
        AccessModeService::new()
            .set_mode(&root, AccessMode::ReadOnly)
            .expect("应能设为只读");

        let error = library_create_project(ProjectWriteRequestDto {
            root_path: root.to_string_lossy().into_owned(),
            id: None,
            input: CreateProject {
                kind: ProjectKind::Simple,
                title: "不得写入".to_owned(),
                description: String::new(),
                prompt: PromptText::default(),
                rating: 0,
                is_favorite: false,
                is_public: false,
                notes: String::new(),
                category_ids: vec![],
                tag_ids: vec![],
            },
        })
        .expect_err("只读模式不得绕过作品库写入命令");

        assert_eq!(error.code, "WORKSPACE_READ_ONLY");
        let count: i64 = rusqlite::Connection::open(root.join("data/library.sqlite3"))
            .expect("应能打开数据库")
            .query_row("SELECT count(*) FROM projects", [], |row| row.get(0))
            .expect("应能统计项目");
        assert_eq!(count, 0, "拒绝后不得创建项目记录");
    }
}

#[cfg(test)]
mod bulk_command_dto_tests {
    use super::{
        BulkAssetEditPreviewRequestDto, BulkAssetEditRequestDto, ModelComparisonRequestDto,
        ProjectAssetAssignmentRequestDto, ProjectAssetRemovalRequestDto,
        library_assign_assets_to_project, library_bulk_edit_assets,
        library_remove_assets_from_project,
    };
    use crate::domain::ModelComparisonItem;

    const BULK_INPUT: &str = r#"{
        "assetIds":[1,2],
        "rating":4,
        "isFavorite":true,
        "isPublic":false,
        "model":{"action":"set","value":"model-a"},
        "platform":{"action":"clear"},
        "addCategoryIds":[3],
        "removeCategoryIds":[],
        "addTagIds":[4],
        "removeTagIds":[]
    }"#;

    #[test]
    fn bulk_edit_dto_rejects_unknown_sensitive_fields_and_missing_confirmation() {
        let unknown = format!(
            r#"{{"rootPath":"workspace","input":{},"prompt":"不得进入命令"}}"#,
            BULK_INPUT
        );
        let missing_confirmation = format!(r#"{{"rootPath":"workspace","input":{}}}"#, BULK_INPUT);

        assert!(serde_json::from_str::<BulkAssetEditPreviewRequestDto>(&unknown).is_err());
        assert!(serde_json::from_str::<BulkAssetEditRequestDto>(&missing_confirmation).is_err());
    }

    #[test]
    fn bulk_edit_execution_rejects_false_confirmation_before_accessing_workspace() {
        let request = format!(
            r#"{{"rootPath":"Z:\\private\\workspace","input":{},"confirmed":false}}"#,
            BULK_INPUT
        );
        let request = serde_json::from_str::<BulkAssetEditRequestDto>(&request)
            .expect("合法请求应能反序列化");

        let error = library_bulk_edit_assets(request).expect_err("未确认必须拒绝");
        assert_eq!(error.code, "LIBRARY_INVALID_INPUT");
        assert!(!error.message.contains("private"));
    }

    #[test]
    fn project_assignment_rejects_false_confirmation_before_accessing_workspace() {
        let request = ProjectAssetAssignmentRequestDto {
            root_path: r"Z:\private\workspace".to_owned(),
            project_id: 1,
            display_numbers: vec![1, 2],
            confirmed: false,
        };
        let error = library_assign_assets_to_project(request).expect_err("未确认必须拒绝");
        assert_eq!(error.code, "LIBRARY_INVALID_INPUT");
        assert!(!error.message.contains("private"));
    }

    #[test]
    fn project_removal_rejects_false_confirmation_before_accessing_workspace() {
        let request = ProjectAssetRemovalRequestDto {
            root_path: r"Z:\private\workspace".to_owned(),
            project_id: 1,
            asset_ids: vec![1, 2],
            confirmed: false,
        };
        let error = library_remove_assets_from_project(request).expect_err("未确认必须拒绝");
        assert_eq!(error.code, "LIBRARY_INVALID_INPUT");
        assert!(!error.message.contains("private"));
    }

    #[test]
    fn nested_bulk_and_comparison_dtos_reject_extra_fields() {
        let nested_unknown = format!(
            r#"{{"rootPath":"workspace","input":{},"confirmed":true}}"#,
            BULK_INPUT.replace(
                r#"{"action":"set","value":"model-a"}"#,
                r#"{"action":"set","value":"model-a","prompt":"secret"}"#,
            )
        );
        let comparison_unknown = r#"{
            "rootPath":"workspace",
            "scope":{"kind":"matchingPrompt","baselineAssetId":1,"filePath":"secret"},
            "cursor":null,
            "limit":20
        }"#;
        let cursor_unknown = r#"{
            "rootPath":"workspace",
            "scope":{"kind":"project","projectId":1},
            "cursor":{"updatedAt":2,"id":3,"prompt":"secret"},
            "limit":20
        }"#;

        assert!(serde_json::from_str::<BulkAssetEditRequestDto>(&nested_unknown).is_err());
        assert!(serde_json::from_str::<ModelComparisonRequestDto>(comparison_unknown).is_err());
        assert!(serde_json::from_str::<ModelComparisonRequestDto>(cursor_unknown).is_err());
    }

    #[test]
    fn model_comparison_response_contains_only_public_summary_fields() {
        let value = serde_json::to_value(ModelComparisonItem {
            id: 7,
            file_name: "preview.png".to_owned(),
            model: "local-model".to_owned(),
            platform: "local-platform".to_owned(),
            updated_at: 9,
        })
        .expect("对比 DTO 应可序列化");
        let object = value.as_object().expect("对比 DTO 应为对象");
        assert_eq!(
            object.keys().map(String::as_str).collect::<Vec<_>>(),
            ["fileName", "id", "model", "platform", "updatedAt"]
        );
        for sensitive in [
            "storedPath",
            "promptZh",
            "promptEn",
            "negativePrompt",
            "notes",
            "generationParams",
        ] {
            assert!(object.get(sensitive).is_none());
        }
    }
}

#[cfg(test)]
mod canvas_command_dto_tests {
    use super::{
        CanvasMemberWriteRequestDto, CanvasOutputPromptWriteRequestDto, library_set_canvas_member,
        library_update_canvas_output_prompt,
    };
    use crate::domain::{CanvasMemberRole, CanvasProjectMember, MediaType};

    #[test]
    fn canvas_write_dtos_reject_unknown_fields() {
        let member_unknown = r#"{
            "rootPath":"workspace",
            "projectId":1,
            "assetId":2,
            "role":"reference",
            "referenceName":"ref",
            "confirmed":true,
            "storedPath":"secret"
        }"#;
        let prompt_unknown = r#"{
            "rootPath":"workspace",
            "projectId":1,
            "assetId":2,
            "prompt":{
                "promptZh":"@ref 画面",
                "promptEn":"",
                "negativePrompt":"",
                "storedPath":"secret"
            },
            "confirmed":true
        }"#;

        assert!(serde_json::from_str::<CanvasMemberWriteRequestDto>(member_unknown).is_err());
        assert!(serde_json::from_str::<CanvasOutputPromptWriteRequestDto>(prompt_unknown).is_err());
    }

    #[test]
    fn canvas_writes_reject_false_confirmation_before_workspace_access() {
        let member = serde_json::from_str::<CanvasMemberWriteRequestDto>(
            r#"{
                "rootPath":"Z:\\private\\workspace",
                "projectId":1,
                "assetId":2,
                "role":"output",
                "referenceName":null,
                "confirmed":false
            }"#,
        )
        .expect("合法请求应能反序列化");
        let member_error = library_set_canvas_member(member).expect_err("未确认必须拒绝");
        assert_eq!(member_error.code, "LIBRARY_INVALID_INPUT");
        assert!(!member_error.message.contains("private"));

        let prompt = serde_json::from_str::<CanvasOutputPromptWriteRequestDto>(
            r#"{
                "rootPath":"Z:\\private\\workspace",
                "projectId":1,
                "assetId":2,
                "prompt":{"promptZh":"secret","promptEn":"","negativePrompt":""},
                "confirmed":false
            }"#,
        )
        .expect("合法请求应能反序列化");
        let prompt_error = library_update_canvas_output_prompt(prompt).expect_err("未确认必须拒绝");
        assert_eq!(prompt_error.code, "LIBRARY_INVALID_INPUT");
        assert!(!prompt_error.message.contains("secret"));
    }

    #[test]
    fn canvas_member_response_contains_only_frozen_fields() {
        let value = serde_json::to_value(CanvasProjectMember {
            asset_id: 2,
            display_order: 3,
            file_name: "preview.png".to_owned(),
            media_type: MediaType::Image,
            model_name: Some("model".to_owned()),
            platform_name: Some("platform".to_owned()),
            width: Some(1024),
            height: Some(768),
            duration_ms: None,
            updated_at: 4,
            role: CanvasMemberRole::Output,
            reference_name: None,
            prompt_zh: "@ref 画面".to_owned(),
            prompt_en: String::new(),
            negative_prompt: String::new(),
        })
        .expect("画布成员应可序列化");
        let object = value.as_object().expect("画布成员应为对象");
        assert_eq!(object.len(), 15);
        for field in [
            "assetId",
            "displayOrder",
            "fileName",
            "mediaType",
            "modelName",
            "platformName",
            "width",
            "height",
            "durationMs",
            "updatedAt",
            "role",
            "referenceName",
            "promptZh",
            "promptEn",
            "negativePrompt",
        ] {
            assert!(object.contains_key(field), "缺少白名单字段 {field}");
        }
        for sensitive in ["storedPath", "notes", "generationParams", "contentHash"] {
            assert!(object.get(sensitive).is_none());
        }
    }
}
