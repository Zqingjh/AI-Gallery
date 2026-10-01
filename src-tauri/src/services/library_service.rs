use std::{
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::Connection;

use crate::{
    domain::{
        AssetDetail, AssetListQuery, AssetOrderChangeMode, AssetSummary, BulkAssetEditInput,
        BulkAssetEditPreview, BulkNullableTextEdit, CanvasMemberRole, CanvasProjectMember,
        Category, CategoryDeleteAction, CategoryInput, CreateAsset, CreateProject, Dimension,
        DimensionInput, DuplicateAssetGroupPage, DuplicateGroupCursor, MAX_PAGE_LIMIT,
        MIN_PAGE_LIMIT, MediaType, MetadataPresetKind, MetadataPresets, ModelComparisonItem,
        ModelComparisonQuery, NumberedAssetPage, Page, PageCursor, PathKind, ProjectDetail,
        ProjectSummary, PromptText, RelationImpact, Tag, TrashEntityType, TrashEntry, UpdateAsset,
        UpdateProject,
    },
    repositories::{LibraryRepository, LibraryRepositoryError},
    services::{
        AccessModeServiceError, DatabaseService, DatabaseServiceError, WorkspaceService,
        WriteAccessGuard, WriteAccessLease,
    },
};

const MAX_RELATIONS_PER_ENTITY: usize = 100;
const NUMBERED_PAGE_SIZES: [u32; 3] = [10, 25, 50];
const MAX_NAME_LENGTH: usize = 200;
const MAX_CANVAS_REFERENCE_NAME_LENGTH: usize = 50;
const MAX_PROMPT_TEXT_LENGTH: usize = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LibraryServiceError {
    InvalidInput,
    NotFound,
    Conflict,
    DatabaseUnavailable,
    DataInvalid,
    ReadOnly,
}

pub(crate) struct LibraryService;

struct WritableConnection {
    connection: Connection,
    _lease: WriteAccessLease,
}

impl Deref for WritableConnection {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        &self.connection
    }
}

impl DerefMut for WritableConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.connection
    }
}

impl LibraryService {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn list_projects(
        &self,
        root: &Path,
        cursor: Option<PageCursor>,
        limit: u32,
    ) -> Result<Page<ProjectSummary>, LibraryServiceError> {
        validate_limit(limit)?;
        let connection = self.open(root)?;
        LibraryRepository::list_projects(&connection, cursor, limit).map_err(map_repository_error)
    }
    pub(crate) fn get_project(
        &self,
        root: &Path,
        id: i64,
    ) -> Result<ProjectDetail, LibraryServiceError> {
        validate_id(id)?;
        let connection = self.open(root)?;
        LibraryRepository::get_project(&connection, id).map_err(map_repository_error)
    }
    pub(crate) fn create_project(
        &self,
        root: &Path,
        input: &CreateProject,
    ) -> Result<ProjectDetail, LibraryServiceError> {
        validate_project(input)?;
        let mut connection = self.open_writable(root)?;
        let id = LibraryRepository::create_project(&mut connection, input, now()?)
            .map_err(map_repository_error)?;
        LibraryRepository::get_project(&connection, id).map_err(map_repository_error)
    }
    pub(crate) fn update_project(
        &self,
        root: &Path,
        id: i64,
        input: &UpdateProject,
    ) -> Result<ProjectDetail, LibraryServiceError> {
        validate_id(id)?;
        validate_project(input)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::update_project(&mut connection, id, input, now()?)
            .map_err(map_repository_error)?;
        LibraryRepository::get_project(&connection, id).map_err(map_repository_error)
    }
    pub(crate) fn assign_assets_to_project(
        &self,
        root: &Path,
        project_id: i64,
        display_numbers: &[i64],
    ) -> Result<u32, LibraryServiceError> {
        validate_id(project_id)?;
        if display_numbers.is_empty() || display_numbers.len() > 100 {
            return Err(LibraryServiceError::InvalidInput);
        }
        let mut connection = self.open_writable(root)?;
        LibraryRepository::assign_assets_to_project_by_display_numbers(
            &mut connection,
            project_id,
            display_numbers,
            now()?,
        )
        .map_err(map_repository_error)
    }
    pub(crate) fn remove_assets_from_project(
        &self,
        root: &Path,
        project_id: i64,
        asset_ids: &[i64],
    ) -> Result<u32, LibraryServiceError> {
        validate_id(project_id)?;
        if asset_ids.is_empty() || asset_ids.len() > 100 {
            return Err(LibraryServiceError::InvalidInput);
        }
        for asset_id in asset_ids {
            validate_id(*asset_id)?;
        }
        let mut connection = self.open_writable(root)?;
        LibraryRepository::remove_assets_from_project(
            &mut connection,
            project_id,
            asset_ids,
            now()?,
        )
        .map_err(map_repository_error)
    }

    pub(crate) fn list_canvas_members(
        &self,
        root: &Path,
        project_id: i64,
        cursor: Option<PageCursor>,
        limit: u32,
        role: Option<CanvasMemberRole>,
    ) -> Result<Page<CanvasProjectMember>, LibraryServiceError> {
        validate_id(project_id)?;
        validate_limit(limit)?;
        validate_cursor(cursor)?;
        let connection = self.open(root)?;
        LibraryRepository::list_canvas_members(&connection, project_id, cursor, limit, role)
            .map_err(map_repository_error)
    }

    pub(crate) fn set_canvas_member(
        &self,
        root: &Path,
        project_id: i64,
        asset_id: i64,
        role: CanvasMemberRole,
        reference_name: Option<&str>,
    ) -> Result<CanvasProjectMember, LibraryServiceError> {
        validate_id(project_id)?;
        validate_id(asset_id)?;
        validate_canvas_reference_name(reference_name)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::set_canvas_member(
            &mut connection,
            project_id,
            asset_id,
            role,
            reference_name,
            now()?,
        )
        .map_err(map_repository_error)
    }

    pub(crate) fn update_canvas_output_prompt(
        &self,
        root: &Path,
        project_id: i64,
        asset_id: i64,
        prompt: &PromptText,
    ) -> Result<CanvasProjectMember, LibraryServiceError> {
        validate_id(project_id)?;
        validate_id(asset_id)?;
        validate_prompt_text(prompt)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::update_canvas_output_prompt(
            &mut connection,
            project_id,
            asset_id,
            prompt,
            now()?,
        )
        .map_err(map_repository_error)
    }

    pub(crate) fn list_assets(
        &self,
        root: &Path,
        query: AssetListQuery,
    ) -> Result<Page<AssetSummary>, LibraryServiceError> {
        validate_limit(query.limit)?;
        if let Some(id) = query.project_id {
            validate_id(id)?;
        }
        validate_asset_query(&query)?;
        let connection = self.open(root)?;
        LibraryRepository::list_assets(&connection, query).map_err(map_repository_error)
    }
    pub(crate) fn list_assets_numbered(
        &self,
        root: &Path,
        query: AssetListQuery,
        page: u32,
        page_size: u32,
    ) -> Result<NumberedAssetPage, LibraryServiceError> {
        if page == 0 || !NUMBERED_PAGE_SIZES.contains(&page_size) {
            return Err(LibraryServiceError::InvalidInput);
        }
        if let Some(id) = query.project_id {
            validate_id(id)?;
        }
        validate_asset_query(&query)?;
        let connection = self.open(root)?;
        LibraryRepository::list_assets_numbered(&connection, query, page, page_size)
            .map_err(map_repository_error)
    }
    pub(crate) fn preview_bulk_asset_edit(
        &self,
        root: &Path,
        input: &BulkAssetEditInput,
    ) -> Result<BulkAssetEditPreview, LibraryServiceError> {
        validate_bulk_asset_edit(input)?;
        LibraryRepository::preview_bulk_asset_edit(&self.open(root)?, input)
            .map_err(map_repository_error)
    }
    pub(crate) fn bulk_edit_assets(
        &self,
        root: &Path,
        input: &BulkAssetEditInput,
    ) -> Result<BulkAssetEditPreview, LibraryServiceError> {
        validate_bulk_asset_edit(input)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::bulk_edit_assets(&mut connection, input, now()?)
            .map_err(map_repository_error)
    }
    pub(crate) fn list_model_comparison(
        &self,
        root: &Path,
        query: ModelComparisonQuery,
    ) -> Result<Page<ModelComparisonItem>, LibraryServiceError> {
        LibraryRepository::list_model_comparison(&self.open(root)?, query)
            .map_err(map_repository_error)
    }
    pub(crate) fn list_duplicate_groups(
        &self,
        root: &Path,
        cursor: Option<DuplicateGroupCursor>,
        limit: u32,
    ) -> Result<DuplicateAssetGroupPage, LibraryServiceError> {
        validate_limit(limit)?;
        if let Some(cursor) = &cursor {
            if cursor.updated_at < 0 || cursor.content_hash.trim().is_empty() {
                return Err(LibraryServiceError::InvalidInput);
            }
        }
        let connection = self.open(root)?;
        LibraryRepository::list_duplicate_groups(&connection, cursor, limit)
            .map_err(map_repository_error)
    }
    pub(crate) fn get_asset(
        &self,
        root: &Path,
        id: i64,
    ) -> Result<AssetDetail, LibraryServiceError> {
        validate_id(id)?;
        let connection = self.open(root)?;
        LibraryRepository::get_asset(&connection, id).map_err(map_repository_error)
    }

    /// 单次打开数据库后读取导出所需的完整作品数据，避免每件作品重新初始化连接。
    pub(crate) fn get_asset_details_for_export(
        &self,
        root: &Path,
        ids: &[i64],
    ) -> Result<Vec<AssetDetail>, LibraryServiceError> {
        if ids.len() > 10_000
            || ids.iter().any(|id| *id <= 0)
            || ids.iter().collect::<std::collections::HashSet<_>>().len() != ids.len()
        {
            return Err(LibraryServiceError::InvalidInput);
        }
        let connection = self.open(root)?;
        ids.iter()
            .map(|id| LibraryRepository::get_asset(&connection, *id).map_err(map_repository_error))
            .collect()
    }

    pub(crate) fn create_asset(
        &self,
        root: &Path,
        input: &CreateAsset,
    ) -> Result<AssetDetail, LibraryServiceError> {
        validate_asset(root, input)?;
        let mut connection = self.open_writable(root)?;
        let id = LibraryRepository::create_asset(&mut connection, input, now()?)
            .map_err(map_repository_error)?;
        LibraryRepository::get_asset(&connection, id).map_err(map_repository_error)
    }
    pub(crate) fn create_imported_assets_batch(
        &self,
        root: &Path,
        inputs: &[(CreateAsset, Option<i64>)],
    ) -> Result<Vec<i64>, LibraryServiceError> {
        if inputs.is_empty() || inputs.len() > 100 {
            return Err(LibraryServiceError::InvalidInput);
        }
        for (input, _) in inputs {
            validate_asset(root, input)?;
        }
        let mut connection = self.open_writable(root)?;
        LibraryRepository::create_imported_assets_batch(&mut connection, inputs, now()?)
            .map_err(map_repository_error)
    }
    pub(crate) fn update_asset(
        &self,
        root: &Path,
        id: i64,
        input: &UpdateAsset,
    ) -> Result<AssetDetail, LibraryServiceError> {
        validate_id(id)?;
        validate_asset(root, input)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::update_asset(&mut connection, id, input, now()?)
            .map_err(map_repository_error)?;
        LibraryRepository::get_asset(&connection, id).map_err(map_repository_error)
    }

    pub(crate) fn update_asset_display_order(
        &self,
        root: &Path,
        id: i64,
        target_position: i64,
        mode: AssetOrderChangeMode,
    ) -> Result<AssetDetail, LibraryServiceError> {
        validate_id(id)?;
        if target_position < 1 {
            return Err(LibraryServiceError::InvalidInput);
        }
        let mut connection = self.open_writable(root)?;
        LibraryRepository::update_asset_display_order(&mut connection, id, target_position, mode)
            .map_err(map_repository_error)?;
        LibraryRepository::get_asset(&connection, id).map_err(map_repository_error)
    }

    pub(crate) fn set_project_categories(
        &self,
        root: &Path,
        id: i64,
        ids: &[i64],
    ) -> Result<ProjectDetail, LibraryServiceError> {
        validate_relation_ids(id, ids)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::set_project_categories(&mut connection, id, ids, now()?)
            .map_err(map_repository_error)?;
        LibraryRepository::get_project(&connection, id).map_err(map_repository_error)
    }
    pub(crate) fn set_project_tags(
        &self,
        root: &Path,
        id: i64,
        ids: &[i64],
    ) -> Result<ProjectDetail, LibraryServiceError> {
        validate_relation_ids(id, ids)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::set_project_tags(&mut connection, id, ids, now()?)
            .map_err(map_repository_error)?;
        LibraryRepository::get_project(&connection, id).map_err(map_repository_error)
    }
    pub(crate) fn set_asset_categories(
        &self,
        root: &Path,
        id: i64,
        ids: &[i64],
    ) -> Result<AssetDetail, LibraryServiceError> {
        validate_relation_ids(id, ids)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::set_asset_categories(&mut connection, id, ids, now()?)
            .map_err(map_repository_error)?;
        LibraryRepository::get_asset(&connection, id).map_err(map_repository_error)
    }
    pub(crate) fn set_asset_tags(
        &self,
        root: &Path,
        id: i64,
        ids: &[i64],
    ) -> Result<AssetDetail, LibraryServiceError> {
        validate_relation_ids(id, ids)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::set_asset_tags(&mut connection, id, ids, now()?)
            .map_err(map_repository_error)?;
        LibraryRepository::get_asset(&connection, id).map_err(map_repository_error)
    }

    pub(crate) fn list_dimensions(
        &self,
        root: &Path,
    ) -> Result<Vec<Dimension>, LibraryServiceError> {
        let connection = self.open(root)?;
        LibraryRepository::list_dimensions(&connection).map_err(map_repository_error)
    }

    pub(crate) fn list_metadata_presets(
        &self,
        root: &Path,
    ) -> Result<MetadataPresets, LibraryServiceError> {
        let connection = self.open(root)?;
        LibraryRepository::list_metadata_presets(&connection).map_err(map_repository_error)
    }

    pub(crate) fn create_metadata_preset(
        &self,
        root: &Path,
        kind: MetadataPresetKind,
        name: &str,
    ) -> Result<i64, LibraryServiceError> {
        validate_name(name)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::create_metadata_preset(&mut connection, kind, name, now()?)
            .map_err(map_repository_error)
    }

    pub(crate) fn update_metadata_preset(
        &self,
        root: &Path,
        kind: MetadataPresetKind,
        id: i64,
        name: &str,
    ) -> Result<(), LibraryServiceError> {
        validate_id(id)?;
        validate_name(name)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::update_metadata_preset(&mut connection, kind, id, name, now()?)
            .map_err(map_repository_error)
    }

    pub(crate) fn delete_metadata_preset(
        &self,
        root: &Path,
        kind: MetadataPresetKind,
        id: i64,
    ) -> Result<(), LibraryServiceError> {
        validate_id(id)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::delete_metadata_preset(&mut connection, kind, id)
            .map_err(map_repository_error)
    }
    pub(crate) fn create_dimension(
        &self,
        root: &Path,
        input: &DimensionInput,
    ) -> Result<i64, LibraryServiceError> {
        validate_name(&input.name)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::create_dimension(&mut connection, input, now()?)
            .map_err(map_repository_error)
    }
    pub(crate) fn update_dimension(
        &self,
        root: &Path,
        id: i64,
        input: &DimensionInput,
    ) -> Result<(), LibraryServiceError> {
        validate_id(id)?;
        validate_name(&input.name)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::update_dimension(&mut connection, id, input, now()?)
            .map_err(map_repository_error)
    }
    pub(crate) fn delete_dimension(&self, root: &Path, id: i64) -> Result<(), LibraryServiceError> {
        validate_id(id)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::delete_dimension(&mut connection, id).map_err(map_repository_error)
    }

    pub(crate) fn list_categories(
        &self,
        root: &Path,
        dimension_id: Option<i64>,
    ) -> Result<Vec<Category>, LibraryServiceError> {
        if let Some(id) = dimension_id {
            validate_id(id)?;
        }
        let connection = self.open(root)?;
        LibraryRepository::list_categories(&connection, dimension_id).map_err(map_repository_error)
    }
    pub(crate) fn create_category(
        &self,
        root: &Path,
        input: &CategoryInput,
    ) -> Result<i64, LibraryServiceError> {
        validate_category(input)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::create_category(&mut connection, input, now()?)
            .map_err(map_repository_error)
    }
    pub(crate) fn update_category(
        &self,
        root: &Path,
        id: i64,
        input: &CategoryInput,
    ) -> Result<(), LibraryServiceError> {
        validate_id(id)?;
        validate_category(input)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::update_category(&mut connection, id, input, now()?)
            .map_err(map_repository_error)
    }
    pub(crate) fn category_impact(
        &self,
        root: &Path,
        id: i64,
    ) -> Result<RelationImpact, LibraryServiceError> {
        validate_id(id)?;
        let connection = self.open(root)?;
        LibraryRepository::category_impact(&connection, id).map_err(map_repository_error)
    }
    pub(crate) fn delete_category(
        &self,
        root: &Path,
        id: i64,
        action: CategoryDeleteAction,
    ) -> Result<RelationImpact, LibraryServiceError> {
        validate_id(id)?;
        if let CategoryDeleteAction::ReplaceWith(value) = action {
            validate_id(value)?;
        }
        let mut connection = self.open_writable(root)?;
        LibraryRepository::delete_category(&mut connection, id, action)
            .map_err(map_repository_error)
    }

    pub(crate) fn list_tags(&self, root: &Path) -> Result<Vec<Tag>, LibraryServiceError> {
        let connection = self.open(root)?;
        LibraryRepository::list_tags(&connection).map_err(map_repository_error)
    }
    pub(crate) fn create_tag(&self, root: &Path, name: &str) -> Result<i64, LibraryServiceError> {
        validate_name(name)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::create_tag(&mut connection, name, now()?).map_err(map_repository_error)
    }
    pub(crate) fn update_tag(
        &self,
        root: &Path,
        id: i64,
        name: &str,
    ) -> Result<(), LibraryServiceError> {
        validate_id(id)?;
        validate_name(name)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::update_tag(&mut connection, id, name, now()?)
            .map_err(map_repository_error)
    }
    pub(crate) fn delete_tag(
        &self,
        root: &Path,
        id: i64,
    ) -> Result<RelationImpact, LibraryServiceError> {
        validate_id(id)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::delete_tag(&mut connection, id).map_err(map_repository_error)
    }

    pub(crate) fn move_project_to_trash(
        &self,
        root: &Path,
        id: i64,
    ) -> Result<i64, LibraryServiceError> {
        self.move_to_trash(root, TrashEntityType::Project, id)
    }
    pub(crate) fn move_asset_to_trash(
        &self,
        root: &Path,
        id: i64,
    ) -> Result<i64, LibraryServiceError> {
        self.move_to_trash(root, TrashEntityType::Asset, id)
    }
    pub(crate) fn move_assets_to_trash(
        &self,
        root: &Path,
        asset_ids: &[i64],
    ) -> Result<Vec<i64>, LibraryServiceError> {
        validate_batch_ids(asset_ids)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::move_assets_to_trash(&mut connection, asset_ids, now()?)
            .map_err(map_repository_error)
    }
    pub(crate) fn list_trash(
        &self,
        root: &Path,
        cursor: Option<PageCursor>,
        limit: u32,
    ) -> Result<Page<TrashEntry>, LibraryServiceError> {
        validate_limit(limit)?;
        let connection = self.open(root)?;
        LibraryRepository::list_trash(&connection, cursor, limit).map_err(map_repository_error)
    }
    pub(crate) fn restore_trash(
        &self,
        root: &Path,
        trash_id: i64,
    ) -> Result<TrashEntry, LibraryServiceError> {
        validate_id(trash_id)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::restore_trash(&mut connection, trash_id).map_err(map_repository_error)
    }
    pub(crate) fn restore_trash_batch(
        &self,
        root: &Path,
        trash_ids: &[i64],
    ) -> Result<(), LibraryServiceError> {
        validate_batch_ids(trash_ids)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::restore_trash_batch(&mut connection, trash_ids)
            .map_err(map_repository_error)
    }
    pub(crate) fn undo_recent_delete(
        &self,
        root: &Path,
    ) -> Result<TrashEntry, LibraryServiceError> {
        let mut connection = self.open_writable(root)?;
        LibraryRepository::undo_recent_delete(&mut connection).map_err(map_repository_error)
    }
    /// 这里只永久删除数据库中的回收站快照；媒体文件始终保留。
    pub(crate) fn purge_trash_record(
        &self,
        root: &Path,
        trash_id: i64,
    ) -> Result<(), LibraryServiceError> {
        validate_id(trash_id)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::purge_trash(&mut connection, trash_id).map_err(map_repository_error)
    }
    /// 批量永久删除的对象仅为回收站快照，原始媒体仍会保留。
    pub(crate) fn purge_trash_records(
        &self,
        root: &Path,
        trash_ids: &[i64],
    ) -> Result<(), LibraryServiceError> {
        validate_batch_ids(trash_ids)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::purge_trash_batch(&mut connection, trash_ids)
            .map_err(map_repository_error)
    }

    fn move_to_trash(
        &self,
        root: &Path,
        kind: TrashEntityType,
        id: i64,
    ) -> Result<i64, LibraryServiceError> {
        validate_id(id)?;
        let mut connection = self.open_writable(root)?;
        LibraryRepository::move_to_trash(&mut connection, kind, id, now()?)
            .map_err(map_repository_error)
    }
    fn open(&self, root: &Path) -> Result<Connection, LibraryServiceError> {
        DatabaseService::new()
            .open_current_workspace_database(root)
            .map_err(map_database_error)
    }

    fn open_writable(&self, root: &Path) -> Result<WritableConnection, LibraryServiceError> {
        let lease = WriteAccessGuard::acquire_writable(root).map_err(map_access_mode_error)?;
        let connection = self.open(root)?;
        Ok(WritableConnection {
            connection,
            _lease: lease,
        })
    }
}

fn validate_project(input: &CreateProject) -> Result<(), LibraryServiceError> {
    validate_name(&input.title)?;
    validate_rating(input.rating)?;
    validate_ids(&input.category_ids)?;
    validate_ids(&input.tag_ids)
}
fn validate_asset(root: &Path, input: &CreateAsset) -> Result<(), LibraryServiceError> {
    if let Some(id) = input.media.project_id {
        validate_id(id)?;
    }
    validate_rating(input.rating)?;
    validate_name(&input.media.file_name)?;
    if input.media.stored_path.trim().is_empty()
        || input.media.file_size.is_some_and(|v| v < 0)
        || input.media.duration_ms.is_some_and(|v| v < 0)
        || input
            .media
            .frame_rate
            .is_some_and(|v| !v.is_finite() || v <= 0.0)
        || !input.generation_params.is_object()
    {
        return Err(LibraryServiceError::InvalidInput);
    }
    validate_ids(&input.category_ids)?;
    validate_ids(&input.tag_ids)?;
    let kind = match input.media.path_kind {
        PathKind::Managed => crate::domain::StoredPathKind::Managed,
        PathKind::External => crate::domain::StoredPathKind::External,
    };
    WorkspaceService::new()
        .check_stored_path(root, kind, &PathBuf::from(&input.media.stored_path))
        .map_err(|_| LibraryServiceError::InvalidInput)?;
    match input.media.media_type {
        MediaType::Image if input.media.duration_ms.is_some() => {
            Err(LibraryServiceError::InvalidInput)
        }
        _ => Ok(()),
    }
}
fn validate_asset_query(query: &AssetListQuery) -> Result<(), LibraryServiceError> {
    if query
        .keyword
        .as_ref()
        .is_some_and(|value| value.trim().chars().count() > 500)
        || query
            .model
            .as_ref()
            .is_some_and(|value| value.trim().chars().count() > MAX_NAME_LENGTH)
        || query
            .platform
            .as_ref()
            .is_some_and(|value| value.trim().chars().count() > MAX_NAME_LENGTH)
        || query.rating.is_some_and(|value| value > 5)
        || query.created_after.is_some_and(|value| value < 0)
        || query.created_before.is_some_and(|value| value < 0)
        || query
            .min_aspect_ratio
            .is_some_and(|value| !value.is_finite() || value <= 0.0)
        || query
            .max_aspect_ratio
            .is_some_and(|value| !value.is_finite() || value <= 0.0)
        || matches!((query.created_after, query.created_before), (Some(start), Some(end)) if start > end)
        || matches!((query.min_aspect_ratio, query.max_aspect_ratio), (Some(min), Some(max)) if min > max)
    {
        return Err(LibraryServiceError::InvalidInput);
    }
    validate_ids(&query.category_ids)
}
fn validate_bulk_asset_edit(input: &BulkAssetEditInput) -> Result<(), LibraryServiceError> {
    validate_batch_ids(&input.asset_ids)?;
    if input.rating.is_some_and(|rating| rating > 5) {
        return Err(LibraryServiceError::InvalidInput);
    }
    for edit in [&input.model, &input.platform] {
        if let BulkNullableTextEdit::Set(value) = edit {
            validate_name(value)?;
        }
    }
    validate_ids(&input.add_category_ids)?;
    validate_ids(&input.remove_category_ids)?;
    validate_ids(&input.add_tag_ids)?;
    validate_ids(&input.remove_tag_ids)
}
fn validate_category(input: &CategoryInput) -> Result<(), LibraryServiceError> {
    validate_id(input.dimension_id)?;
    validate_name(&input.name)?;
    if input.aliases.len() > MAX_RELATIONS_PER_ENTITY {
        return Err(LibraryServiceError::InvalidInput);
    }
    for alias in &input.aliases {
        validate_name(alias)?;
    }
    Ok(())
}
fn validate_relation_ids(owner_id: i64, ids: &[i64]) -> Result<(), LibraryServiceError> {
    validate_id(owner_id)?;
    validate_ids(ids)
}
fn validate_ids(ids: &[i64]) -> Result<(), LibraryServiceError> {
    if ids.len() > MAX_RELATIONS_PER_ENTITY || ids.iter().any(|id| *id <= 0) {
        Err(LibraryServiceError::InvalidInput)
    } else {
        Ok(())
    }
}
fn validate_batch_ids(ids: &[i64]) -> Result<(), LibraryServiceError> {
    if ids.is_empty()
        || ids.len() > MAX_RELATIONS_PER_ENTITY
        || ids.iter().any(|id| *id <= 0)
        || ids.iter().collect::<std::collections::HashSet<_>>().len() != ids.len()
    {
        Err(LibraryServiceError::InvalidInput)
    } else {
        Ok(())
    }
}
fn validate_id(id: i64) -> Result<(), LibraryServiceError> {
    if id > 0 {
        Ok(())
    } else {
        Err(LibraryServiceError::InvalidInput)
    }
}
fn validate_name(value: &str) -> Result<(), LibraryServiceError> {
    let length = value.trim().chars().count();
    if length == 0 || length > MAX_NAME_LENGTH {
        Err(LibraryServiceError::InvalidInput)
    } else {
        Ok(())
    }
}
fn validate_canvas_reference_name(value: Option<&str>) -> Result<(), LibraryServiceError> {
    match value {
        Some(value)
            if (1..=MAX_CANVAS_REFERENCE_NAME_LENGTH).contains(&value.trim().chars().count()) =>
        {
            Ok(())
        }
        Some(_) => Err(LibraryServiceError::InvalidInput),
        None => Ok(()),
    }
}
fn validate_prompt_text(value: &PromptText) -> Result<(), LibraryServiceError> {
    if [
        value.prompt_zh.as_str(),
        value.prompt_en.as_str(),
        value.negative_prompt.as_str(),
    ]
    .into_iter()
    .any(|text| text.chars().count() > MAX_PROMPT_TEXT_LENGTH)
    {
        Err(LibraryServiceError::InvalidInput)
    } else {
        Ok(())
    }
}
fn validate_cursor(cursor: Option<PageCursor>) -> Result<(), LibraryServiceError> {
    if cursor.is_some_and(|cursor| cursor.id <= 0 || cursor.updated_at < 0) {
        Err(LibraryServiceError::InvalidInput)
    } else {
        Ok(())
    }
}
fn validate_rating(value: u8) -> Result<(), LibraryServiceError> {
    if value <= 5 {
        Ok(())
    } else {
        Err(LibraryServiceError::InvalidInput)
    }
}
fn validate_limit(limit: u32) -> Result<(), LibraryServiceError> {
    if (MIN_PAGE_LIMIT..=MAX_PAGE_LIMIT).contains(&limit) {
        Ok(())
    } else {
        Err(LibraryServiceError::InvalidInput)
    }
}
fn now() -> Result<i64, LibraryServiceError> {
    let milliseconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| LibraryServiceError::DatabaseUnavailable)?
        .as_millis();
    i64::try_from(milliseconds).map_err(|_| LibraryServiceError::DatabaseUnavailable)
}
fn map_repository_error(error: LibraryRepositoryError) -> LibraryServiceError {
    match error {
        LibraryRepositoryError::NotFound => LibraryServiceError::NotFound,
        LibraryRepositoryError::Conflict => LibraryServiceError::Conflict,
        LibraryRepositoryError::InvalidData => LibraryServiceError::DataInvalid,
        LibraryRepositoryError::DatabaseFailed => LibraryServiceError::DatabaseUnavailable,
    }
}
fn map_database_error(_: DatabaseServiceError) -> LibraryServiceError {
    LibraryServiceError::DatabaseUnavailable
}
fn map_access_mode_error(error: AccessModeServiceError) -> LibraryServiceError {
    match error {
        AccessModeServiceError::ReadOnly => LibraryServiceError::ReadOnly,
        AccessModeServiceError::Workspace(_)
        | AccessModeServiceError::DatabaseUnavailable
        | AccessModeServiceError::InvalidSetting
        | AccessModeServiceError::ClockUnavailable => LibraryServiceError::DatabaseUnavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LibraryServiceError, MAX_NAME_LENGTH, MAX_PROMPT_TEXT_LENGTH, validate_bulk_asset_edit,
        validate_canvas_reference_name, validate_cursor, validate_limit, validate_prompt_text,
        validate_rating,
    };
    use crate::domain::{BulkAssetEditInput, BulkNullableTextEdit, PageCursor, PromptText};
    #[test]
    fn page_limit_is_bounded() {
        assert_eq!(validate_limit(0), Err(LibraryServiceError::InvalidInput));
        assert!(validate_limit(1).is_ok());
        assert!(validate_limit(100).is_ok());
        assert_eq!(validate_limit(101), Err(LibraryServiceError::InvalidInput));
    }
    #[test]
    fn rating_is_bounded() {
        assert!(validate_rating(5).is_ok());
        assert_eq!(validate_rating(6), Err(LibraryServiceError::InvalidInput));
    }

    #[test]
    fn canvas_inputs_reject_invalid_cursor_and_text_lengths() {
        assert_eq!(
            validate_cursor(Some(PageCursor {
                updated_at: 0,
                id: 0,
            })),
            Err(LibraryServiceError::InvalidInput)
        );
        assert_eq!(
            validate_canvas_reference_name(Some(" ")),
            Err(LibraryServiceError::InvalidInput)
        );
        assert!(validate_canvas_reference_name(None).is_ok());

        let valid = PromptText {
            prompt_zh: "@ref 一幅图".to_owned(),
            prompt_en: String::new(),
            negative_prompt: String::new(),
        };
        assert!(validate_prompt_text(&valid).is_ok());

        let invalid = PromptText {
            prompt_zh: "x".repeat(MAX_PROMPT_TEXT_LENGTH + 1),
            ..PromptText::default()
        };
        assert_eq!(
            validate_prompt_text(&invalid),
            Err(LibraryServiceError::InvalidInput)
        );
    }

    #[test]
    fn bulk_model_and_platform_names_are_bounded() {
        let input = BulkAssetEditInput {
            asset_ids: vec![1],
            model: BulkNullableTextEdit::Set("x".repeat(MAX_NAME_LENGTH + 1)),
            ..BulkAssetEditInput::default()
        };
        assert_eq!(
            validate_bulk_asset_edit(&input),
            Err(LibraryServiceError::InvalidInput)
        );
    }
}
