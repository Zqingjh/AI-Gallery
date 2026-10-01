use std::collections::{HashMap, HashSet};

use rusqlite::{
    Connection, OptionalExtension, Row, Transaction, params, params_from_iter, types::Value,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::domain::{
    AssetDetail, AssetListQuery, AssetMetadata, AssetOrderChangeMode, AssetSearchField,
    AssetSummary, BulkAssetEditInput, BulkAssetEditPreview, BulkNullableTextEdit, CanvasMemberRole,
    CanvasProjectMember, Category, CategoryDeleteAction, CategoryInput, CreateAsset, CreateProject,
    Dimension, DimensionInput, MAX_BULK_ASSET_IDS, MediaType, MetadataPreset, MetadataPresetKind,
    MetadataPresets, ModelComparisonItem, ModelComparisonQuery, ModelComparisonScope,
    NumberedAssetPage, Page, PageCursor, PathKind, ProjectDetail, ProjectKind, ProjectSummary,
    PromptText, RelationImpact, Tag, TrashEntityType, TrashEntry, UpdateAsset, UpdateProject,
};

const MAX_METADATA_NAME_LENGTH: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LibraryRepositoryError {
    NotFound,
    Conflict,
    InvalidData,
    DatabaseFailed,
}

pub(crate) struct LibraryRepository;

impl LibraryRepository {
    pub(crate) fn asset_ids_for_display_numbers(
        connection: &Connection,
        display_numbers: &[i64],
    ) -> Result<Vec<i64>, LibraryRepositoryError> {
        asset_ids_for_display_numbers(connection, display_numbers)
    }

    pub(crate) fn list_projects(
        connection: &Connection,
        cursor: Option<PageCursor>,
        limit: u32,
    ) -> Result<Page<ProjectSummary>, LibraryRepositoryError> {
        let sql = "SELECT pr.id, pr.kind, pr.title, pr.description,
                          coalesce(p.prompt_zh, ''), coalesce(p.prompt_en, ''),
                          coalesce(p.negative_prompt, ''), pr.rating, pr.is_favorite,
                          pr.is_public, pr.notes,
                          (SELECT count(*) FROM assets a3 WHERE a3.project_id = pr.id),
                          pr.created_at, pr.updated_at
                   FROM projects pr
                   LEFT JOIN prompts p ON p.id = (
                       SELECT p2.id FROM prompts p2
                       WHERE p2.project_id = pr.id
                         AND NOT EXISTS (SELECT 1 FROM assets a2 WHERE a2.prompt_id = p2.id)
                       ORDER BY p2.id LIMIT 1
                   )
                   WHERE (?1 IS NULL OR pr.updated_at < ?1 OR (pr.updated_at = ?1 AND pr.id < ?2))
                   ORDER BY pr.updated_at DESC, pr.id DESC LIMIT ?3";
        let (cursor_time, cursor_id) = cursor
            .map(|value| (Some(value.updated_at), Some(value.id)))
            .unwrap_or((None, None));
        let mut statement = connection.prepare(sql).map_err(map_db_error)?;
        let rows = statement
            .query_map(
                params![cursor_time, cursor_id, i64::from(limit) + 1],
                project_from_row,
            )
            .map_err(map_db_error)?;
        let mut items = rows.collect::<Result<Vec<_>, _>>().map_err(map_db_error)?;
        Ok(build_page(&mut items, limit, |item| PageCursor {
            updated_at: item.updated_at,
            id: item.id,
        }))
    }

    pub(crate) fn get_project(
        connection: &Connection,
        id: i64,
    ) -> Result<ProjectDetail, LibraryRepositoryError> {
        let summary = connection
            .query_row(
                "SELECT pr.id, pr.kind, pr.title, pr.description,
                        coalesce(p.prompt_zh, ''), coalesce(p.prompt_en, ''),
                        coalesce(p.negative_prompt, ''), pr.rating, pr.is_favorite,
                        pr.is_public, pr.notes,
                        (SELECT count(*) FROM assets a3 WHERE a3.project_id = pr.id),
                        pr.created_at, pr.updated_at
                 FROM projects pr
                 LEFT JOIN prompts p ON p.id = (
                    SELECT p2.id FROM prompts p2 WHERE p2.project_id = pr.id
                      AND NOT EXISTS (SELECT 1 FROM assets a2 WHERE a2.prompt_id = p2.id)
                    ORDER BY p2.id LIMIT 1
                 ) WHERE pr.id = ?1",
                [id],
                project_from_row,
            )
            .optional()
            .map_err(map_db_error)?
            .ok_or(LibraryRepositoryError::NotFound)?;
        Ok(ProjectDetail {
            summary,
            category_ids: relation_ids(connection, "project_categories", "project_id", id)?,
            tag_ids: relation_ids(connection, "project_tags", "project_id", id)?,
        })
    }

    pub(crate) fn create_project(
        connection: &mut Connection,
        input: &CreateProject,
        now: i64,
    ) -> Result<i64, LibraryRepositoryError> {
        let transaction = connection.transaction().map_err(map_db_error)?;
        transaction
            .execute(
                "INSERT INTO projects
                 (kind, title, description, rating, is_favorite, is_public, notes, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                params![
                    input.kind.as_db_str(),
                    input.title.trim(),
                    input.description,
                    input.rating,
                    input.is_favorite,
                    input.is_public,
                    input.notes,
                    now
                ],
            )
            .map_err(map_db_error)?;
        let id = transaction.last_insert_rowid();
        insert_project_prompt(&transaction, id, &input.prompt, now)?;
        replace_categories(
            &transaction,
            "project_categories",
            "project_id",
            id,
            &input.category_ids,
            now,
        )?;
        replace_tags(
            &transaction,
            "project_tags",
            "project_id",
            id,
            &input.tag_ids,
            now,
        )?;
        transaction.commit().map_err(map_db_error)?;
        Ok(id)
    }

    pub(crate) fn update_project(
        connection: &mut Connection,
        id: i64,
        input: &UpdateProject,
        now: i64,
    ) -> Result<(), LibraryRepositoryError> {
        let transaction = connection.transaction().map_err(map_db_error)?;
        let previous = Self::get_project(&transaction, id)?;
        let mut changed_fields = Vec::new();
        if previous.summary.kind != input.kind {
            changed_fields.push("kind");
        }
        if previous.summary.title != input.title.trim() {
            changed_fields.push("title");
        }
        if previous.summary.description != input.description {
            changed_fields.push("description");
        }
        if previous.summary.rating != input.rating {
            changed_fields.push("rating");
        }
        if previous.summary.is_favorite != input.is_favorite {
            changed_fields.push("is_favorite");
        }
        if previous.summary.is_public != input.is_public {
            changed_fields.push("is_public");
        }
        if previous.summary.notes != input.notes {
            changed_fields.push("notes");
        }
        if previous.summary.prompt.prompt_zh != input.prompt.prompt_zh {
            changed_fields.push("prompt_zh");
        }
        if previous.summary.prompt.prompt_en != input.prompt.prompt_en {
            changed_fields.push("prompt_en");
        }
        if previous.summary.prompt.negative_prompt != input.prompt.negative_prompt {
            changed_fields.push("negative_prompt");
        }
        if !same_ids(&previous.category_ids, &input.category_ids) {
            changed_fields.push("category_ids");
        }
        if !same_ids(&previous.tag_ids, &input.tag_ids) {
            changed_fields.push("tag_ids");
        }
        ensure_changed(
            transaction
                .execute(
                    "UPDATE projects SET kind=?1, title=?2, description=?3, rating=?4, is_favorite=?5,
             is_public=?6, notes=?7, updated_at=max(updated_at, ?8) WHERE id=?9",
                    params![
                        input.kind.as_db_str(),
                        input.title.trim(),
                        input.description,
                        input.rating,
                        input.is_favorite,
                        input.is_public,
                        input.notes,
                        now,
                        id
                    ],
                )
                .map_err(map_db_error)?,
        )?;
        upsert_project_prompt(&transaction, id, &input.prompt, now)?;
        replace_categories(
            &transaction,
            "project_categories",
            "project_id",
            id,
            &input.category_ids,
            now,
        )?;
        replace_tags(
            &transaction,
            "project_tags",
            "project_id",
            id,
            &input.tag_ids,
            now,
        )?;
        if input.kind == ProjectKind::Canvas {
            ensure_canvas_outputs_for_project(&transaction, id, now)?;
        }
        if !changed_fields.is_empty() {
            record_confirmed_edit(&transaction, "project", id, "update", &changed_fields, now)?;
        }
        transaction.commit().map_err(map_db_error)
    }

    pub(crate) fn assign_assets_to_project_by_display_numbers(
        connection: &mut Connection,
        project_id: i64,
        display_numbers: &[i64],
        now: i64,
    ) -> Result<u32, LibraryRepositoryError> {
        if display_numbers.is_empty() || display_numbers.len() > MAX_BULK_ASSET_IDS {
            return Err(LibraryRepositoryError::InvalidData);
        }
        let transaction = connection.transaction().map_err(map_db_error)?;
        let project_exists = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
                [project_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(map_db_error)?;
        if !project_exists {
            return Err(LibraryRepositoryError::NotFound);
        }
        let asset_ids = asset_ids_for_display_numbers(&transaction, display_numbers)?;
        ensure_canvas_output_invariant_after_removing_assets(
            &transaction,
            &asset_ids,
            Some(project_id),
        )?;
        ensure_canvas_references_safe_after_removing_assets(
            &transaction,
            &asset_ids,
            Some(project_id),
        )?;
        let asset_ids_json = ids_json(&asset_ids)?;
        transaction
            .execute(
                "UPDATE assets
                    SET project_id=?1, updated_at=max(updated_at,?2)
                  WHERE id IN (SELECT value FROM json_each(?3))",
                params![project_id, now, asset_ids_json],
            )
            .map_err(map_db_error)?;
        ensure_canvas_outputs_for_project(&transaction, project_id, now)?;
        for asset_id in &asset_ids {
            record_confirmed_edit(
                &transaction,
                "asset",
                *asset_id,
                "bulk_update",
                &["project_id"],
                now,
            )?;
        }
        transaction.commit().map_err(map_db_error)?;
        u32::try_from(asset_ids.len()).map_err(|_| LibraryRepositoryError::InvalidData)
    }

    pub(crate) fn remove_assets_from_project(
        connection: &mut Connection,
        project_id: i64,
        asset_ids: &[i64],
        now: i64,
    ) -> Result<u32, LibraryRepositoryError> {
        if asset_ids.is_empty() || asset_ids.len() > MAX_BULK_ASSET_IDS {
            return Err(LibraryRepositoryError::InvalidData);
        }
        let transaction = connection.transaction().map_err(map_db_error)?;
        let asset_ids_json = ids_json(asset_ids)?;
        let matched: i64 = transaction
            .query_row(
                "SELECT count(*) FROM assets
                  WHERE project_id=?1
                    AND id IN (SELECT value FROM json_each(?2))",
                params![project_id, asset_ids_json],
                |row| row.get(0),
            )
            .map_err(map_db_error)?;
        if matched
            != i64::try_from(asset_ids.len()).map_err(|_| LibraryRepositoryError::InvalidData)?
        {
            return Err(LibraryRepositoryError::NotFound);
        }
        ensure_canvas_output_invariant_after_removing_assets(&transaction, asset_ids, None)?;
        ensure_canvas_references_safe_after_removing_assets(&transaction, asset_ids, None)?;
        transaction
            .execute(
                "UPDATE assets
                    SET project_id=NULL, updated_at=max(updated_at,?1)
                  WHERE project_id=?2
                    AND id IN (SELECT value FROM json_each(?3))",
                params![now, project_id, asset_ids_json],
            )
            .map_err(map_db_error)?;
        for asset_id in asset_ids {
            record_confirmed_edit(
                &transaction,
                "asset",
                *asset_id,
                "bulk_update",
                &["project_id"],
                now,
            )?;
        }
        transaction.commit().map_err(map_db_error)?;
        u32::try_from(asset_ids.len()).map_err(|_| LibraryRepositoryError::InvalidData)
    }

    pub(crate) fn list_canvas_members(
        connection: &Connection,
        project_id: i64,
        cursor: Option<PageCursor>,
        limit: u32,
        role: Option<CanvasMemberRole>,
    ) -> Result<Page<CanvasProjectMember>, LibraryRepositoryError> {
        ensure_canvas_project(connection, project_id)?;
        let (cursor_time, cursor_id) = cursor
            .map(|value| (Some(value.updated_at), Some(value.id)))
            .unwrap_or((None, None));
        let mut statement = connection
            .prepare(
                "SELECT a.id, orders.position, a.file_name, a.media_type,
                        model.name, platform.name, a.width, a.height, a.duration_ms,
                        a.updated_at, member.role, member.reference_name,
                        coalesce(prompt.prompt_zh,''), coalesce(prompt.prompt_en,''),
                        coalesce(prompt.negative_prompt,'')
                   FROM assets a INDEXED BY idx_assets_project_page
                   JOIN canvas_project_members member
                     ON member.project_id=a.project_id AND member.asset_id=a.id
                   JOIN asset_display_order orders ON orders.asset_id=a.id
                   LEFT JOIN models model ON model.id=a.model_id
                   LEFT JOIN platforms platform ON platform.id=a.platform_id
                   LEFT JOIN prompts prompt ON prompt.id=a.prompt_id
                  WHERE a.project_id=?1
                    AND (?2 IS NULL OR a.updated_at<?2 OR (a.updated_at=?2 AND a.id<?3))
                    AND (?4 IS NULL OR member.role=?4)
                  ORDER BY a.updated_at DESC,a.id DESC LIMIT ?5",
            )
            .map_err(map_db_error)?;
        let rows = statement
            .query_map(
                params![
                    project_id,
                    cursor_time,
                    cursor_id,
                    role.map(CanvasMemberRole::as_db_str),
                    i64::from(limit) + 1
                ],
                canvas_member_from_row,
            )
            .map_err(map_db_error)?;
        let mut items = rows.collect::<Result<Vec<_>, _>>().map_err(map_db_error)?;
        Ok(build_page(&mut items, limit, |item| PageCursor {
            updated_at: item.updated_at,
            id: item.asset_id,
        }))
    }

    pub(crate) fn set_canvas_member(
        connection: &mut Connection,
        project_id: i64,
        asset_id: i64,
        role: CanvasMemberRole,
        reference_name: Option<&str>,
        now: i64,
    ) -> Result<CanvasProjectMember, LibraryRepositoryError> {
        let transaction = connection.transaction().map_err(map_db_error)?;
        ensure_canvas_project(&transaction, project_id)?;
        let media_type: String = transaction
            .query_row(
                "SELECT media_type FROM assets WHERE id=?1 AND project_id=?2",
                params![asset_id, project_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(map_db_error)?
            .ok_or(LibraryRepositoryError::NotFound)?;
        let reference_name = match role {
            CanvasMemberRole::Output => None,
            CanvasMemberRole::Reference => {
                if media_type != "image" {
                    return Err(LibraryRepositoryError::InvalidData);
                }
                let name = normalize_reference_name(
                    reference_name.ok_or(LibraryRepositoryError::InvalidData)?,
                )?;
                let other_output_exists: bool = transaction
                    .query_row(
                        "SELECT EXISTS(
                            SELECT 1 FROM canvas_project_members
                             WHERE project_id=?1 AND role='output' AND asset_id<>?2
                        )",
                        params![project_id, asset_id],
                        |row| row.get(0),
                    )
                    .map_err(map_db_error)?;
                if !other_output_exists {
                    return Err(LibraryRepositoryError::Conflict);
                }
                Some(name)
            }
        };
        ensure_canvas_reference_change_safe(
            &transaction,
            project_id,
            asset_id,
            role,
            reference_name.as_deref(),
        )?;
        let position: i64 = transaction
            .query_row(
                "SELECT position FROM canvas_project_members
                  WHERE project_id=?1 AND asset_id=?2",
                params![project_id, asset_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(map_db_error)?
            .map(Ok)
            .unwrap_or_else(|| {
                transaction
                    .query_row(
                        "SELECT coalesce(max(position),-1)+1
                           FROM canvas_project_members WHERE project_id=?1",
                        [project_id],
                        |row| row.get(0),
                    )
                    .map_err(map_db_error)
            })?;
        transaction
            .execute(
                "INSERT INTO canvas_project_members
                 (project_id,asset_id,role,reference_name,position,created_at,updated_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?6)
                 ON CONFLICT(project_id,asset_id) DO UPDATE SET
                    role=excluded.role,reference_name=excluded.reference_name,
                    updated_at=max(canvas_project_members.updated_at,excluded.updated_at)",
                params![
                    project_id,
                    asset_id,
                    role.as_db_str(),
                    reference_name,
                    position,
                    now
                ],
            )
            .map_err(map_db_error)?;
        transaction.commit().map_err(map_db_error)?;
        canvas_member(connection, project_id, asset_id)
    }

    pub(crate) fn update_canvas_output_prompt(
        connection: &mut Connection,
        project_id: i64,
        asset_id: i64,
        prompt: &PromptText,
        now: i64,
    ) -> Result<CanvasProjectMember, LibraryRepositoryError> {
        let transaction = connection.transaction().map_err(map_db_error)?;
        ensure_canvas_project(&transaction, project_id)?;
        let prompt_id: Option<i64> = transaction
            .query_row(
                "SELECT a.prompt_id
                   FROM assets a
                   JOIN canvas_project_members member
                     ON member.project_id=a.project_id AND member.asset_id=a.id
                  WHERE a.id=?1 AND a.project_id=?2 AND member.role='output'",
                params![asset_id, project_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(map_db_error)?
            .ok_or(LibraryRepositoryError::NotFound)?;
        validate_canvas_prompt_references(&transaction, project_id, prompt)?;
        let previous_prompt = prompt_id
            .map(|id| {
                transaction
                    .query_row(
                        "SELECT prompt_zh,prompt_en,negative_prompt FROM prompts WHERE id=?1",
                        [id],
                        |row| {
                            Ok(PromptText {
                                prompt_zh: row.get(0)?,
                                prompt_en: row.get(1)?,
                                negative_prompt: row.get(2)?,
                            })
                        },
                    )
                    .map_err(map_db_error)
            })
            .transpose()?;
        let prompt_id = match prompt_id {
            Some(prompt_id) => {
                let shared: bool = transaction
                    .query_row(
                        "SELECT count(*)>1 FROM assets WHERE prompt_id=?1",
                        [prompt_id],
                        |row| row.get(0),
                    )
                    .map_err(map_db_error)?;
                if shared {
                    insert_asset_prompt(&transaction, prompt, now)?
                } else {
                    transaction
                        .execute(
                            "UPDATE prompts SET prompt_zh=?1,prompt_en=?2,negative_prompt=?3,
                             updated_at=max(updated_at,?4) WHERE id=?5",
                            params![
                                prompt.prompt_zh,
                                prompt.prompt_en,
                                prompt.negative_prompt,
                                now,
                                prompt_id
                            ],
                        )
                        .map_err(map_db_error)?;
                    prompt_id
                }
            }
            None => insert_asset_prompt(&transaction, prompt, now)?,
        };
        transaction
            .execute(
                "UPDATE assets SET prompt_id=?1,updated_at=max(updated_at,?2) WHERE id=?3",
                params![prompt_id, now, asset_id],
            )
            .map_err(map_db_error)?;
        let mut changed_fields = Vec::new();
        if previous_prompt.as_ref().map(|value| &value.prompt_zh) != Some(&prompt.prompt_zh) {
            changed_fields.push("prompt_zh");
        }
        if previous_prompt.as_ref().map(|value| &value.prompt_en) != Some(&prompt.prompt_en) {
            changed_fields.push("prompt_en");
        }
        if previous_prompt.as_ref().map(|value| &value.negative_prompt)
            != Some(&prompt.negative_prompt)
        {
            changed_fields.push("negative_prompt");
        }
        if !changed_fields.is_empty() {
            record_confirmed_edit(
                &transaction,
                "asset",
                asset_id,
                "update",
                &changed_fields,
                now,
            )?;
        }
        transaction.commit().map_err(map_db_error)?;
        canvas_member(connection, project_id, asset_id)
    }

    pub(crate) fn list_assets(
        connection: &Connection,
        query: AssetListQuery,
    ) -> Result<Page<AssetSummary>, LibraryRepositoryError> {
        let (sql, values) = Self::build_asset_list_sql(&query);
        let mut statement = connection.prepare(&sql).map_err(map_db_error)?;
        let rows = statement
            .query_map(params_from_iter(values), asset_from_row)
            .map_err(map_db_error)?;
        let mut items = rows.collect::<Result<Vec<_>, _>>().map_err(map_db_error)?;
        Ok(build_page(&mut items, query.limit, |item| PageCursor {
            updated_at: item.updated_at,
            id: item.id,
        }))
    }

    pub(crate) fn list_assets_numbered(
        connection: &Connection,
        mut query: AssetListQuery,
        page: u32,
        page_size: u32,
    ) -> Result<NumberedAssetPage, LibraryRepositoryError> {
        if page == 0 || page_size == 0 {
            return Err(LibraryRepositoryError::InvalidData);
        }
        query.cursor = None;
        query.limit = page_size;
        let (base_sql, mut filter_values) = Self::build_asset_list_sql(&query);
        filter_values
            .pop()
            .ok_or(LibraryRepositoryError::InvalidData)?;
        let filter_sql = base_sql
            .split_once(" ORDER BY")
            .map(|(sql, _)| sql)
            .ok_or(LibraryRepositoryError::InvalidData)?;
        let count_sql = format!("SELECT count(*) FROM ({filter_sql}) numbered_assets");
        // COUNT 与页片必须来自同一 SQLite 读取快照，避免并发导入造成总数与内容不一致。
        let transaction = connection.unchecked_transaction().map_err(map_db_error)?;
        let total_count_i64: i64 = transaction
            .query_row(&count_sql, params_from_iter(filter_values.clone()), |row| {
                row.get(0)
            })
            .map_err(map_db_error)?;
        let total_count =
            u64::try_from(total_count_i64).map_err(|_| LibraryRepositoryError::InvalidData)?;
        let offset = u64::from(page - 1)
            .checked_mul(u64::from(page_size))
            .and_then(|value| i64::try_from(value).ok())
            .ok_or(LibraryRepositoryError::InvalidData)?;
        filter_values.push(i64::from(page_size).into());
        filter_values.push(offset.into());
        let sql = format!("{filter_sql} ORDER BY ao.position ASC LIMIT ? OFFSET ?");
        let items = {
            let mut statement = transaction.prepare(&sql).map_err(map_db_error)?;
            let rows = statement
                .query_map(params_from_iter(filter_values), asset_from_row)
                .map_err(map_db_error)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(map_db_error)?
        };
        transaction.commit().map_err(map_db_error)?;
        let total_pages_u64 = total_count.div_ceil(u64::from(page_size));
        let total_pages =
            u32::try_from(total_pages_u64).map_err(|_| LibraryRepositoryError::InvalidData)?;
        Ok(NumberedAssetPage {
            items,
            page,
            page_size,
            total_count,
            total_pages,
        })
    }

    fn build_asset_list_sql(query: &AssetListQuery) -> (String, Vec<Value>) {
        let mut sql = String::from(
            "SELECT a.id, a.project_id, a.media_type, a.path_kind, a.stored_path, a.file_name,
                    a.mime_type, a.file_size, a.content_hash, a.width, a.height, a.duration_ms,
                    a.frame_rate, a.has_audio, coalesce(p.prompt_zh, ''), coalesce(p.prompt_en, ''),
                    coalesce(p.negative_prompt, ''), m.name, pl.name, a.generation_params_json,
                    a.rating, a.is_favorite, a.is_public, a.notes, a.created_at, a.updated_at,
                    ao.position
             FROM assets a LEFT JOIN prompts p ON p.id=a.prompt_id
             LEFT JOIN models m ON m.id=a.model_id LEFT JOIN platforms pl ON pl.id=a.platform_id
             JOIN asset_display_order ao ON ao.asset_id=a.id
             WHERE 1=1",
        );
        let mut values = Vec::<Value>::new();
        if let Some(project_id) = query.project_id {
            sql.push_str(" AND a.project_id=?");
            values.push(project_id.into());
        }
        if let Some(media_type) = query.media_type {
            sql.push_str(" AND a.media_type=?");
            values.push(media_type.as_db_str().to_owned().into());
        }
        if let Some(keyword) = query
            .keyword
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let expression = match query.search_field {
                AssetSearchField::Title => "a.file_name",
                AssetSearchField::Prompt => {
                    "coalesce(p.prompt_zh, '') || ' ' || coalesce(p.prompt_en, '') || ' ' || coalesce(p.negative_prompt, '')"
                }
                AssetSearchField::Notes => "a.notes",
            };
            if query.exact_match {
                sql.push_str(&format!(" AND {expression} = ? COLLATE NOCASE"));
                values.push(keyword.to_owned().into());
            } else {
                sql.push_str(&format!(
                    " AND {expression} LIKE ? ESCAPE '\\' COLLATE NOCASE"
                ));
                values.push(format!("%{}%", escape_like(keyword)).into());
            }
        }
        if let Some(model) = query
            .model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            sql.push_str(" AND m.name=?");
            values.push(model.to_owned().into());
        }
        if let Some(platform) = query
            .platform
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            sql.push_str(" AND pl.name=?");
            values.push(platform.to_owned().into());
        }
        for category_id in &query.category_ids {
            sql.push_str(" AND EXISTS(SELECT 1 FROM asset_categories ac WHERE ac.asset_id=a.id AND ac.category_id=?)");
            values.push((*category_id).into());
        }
        if let Some(rating) = query.rating {
            sql.push_str(" AND a.rating=?");
            values.push(i64::from(rating).into());
        }
        if let Some(is_favorite) = query.is_favorite {
            sql.push_str(" AND a.is_favorite=?");
            values.push(is_favorite.into());
        }
        if let Some(is_public) = query.is_public {
            sql.push_str(" AND a.is_public=?");
            values.push(is_public.into());
        }
        if let Some(created_after) = query.created_after {
            sql.push_str(" AND a.updated_at>=?");
            values.push(created_after.into());
        }
        if let Some(created_before) = query.created_before {
            sql.push_str(" AND a.updated_at<=?");
            values.push(created_before.into());
        }
        if let Some(min_ratio) = query.min_aspect_ratio {
            sql.push_str(" AND (CAST(a.width AS REAL) / a.height)>=?");
            values.push(min_ratio.into());
        }
        if let Some(max_ratio) = query.max_aspect_ratio {
            sql.push_str(" AND (CAST(a.width AS REAL) / a.height)<=?");
            values.push(max_ratio.into());
        }
        if let Some(cursor) = query.cursor {
            sql.push_str(" AND (a.updated_at < ? OR (a.updated_at = ? AND a.id < ?))");
            values.push(cursor.updated_at.into());
            values.push(cursor.updated_at.into());
            values.push(cursor.id.into());
        }
        sql.push_str(" ORDER BY a.updated_at DESC, a.id DESC LIMIT ?");
        values.push((i64::from(query.limit) + 1).into());
        (sql, values)
    }

    pub(crate) fn list_duplicate_groups(
        connection: &Connection,
        cursor: Option<crate::domain::DuplicateGroupCursor>,
        limit: u32,
    ) -> Result<crate::domain::DuplicateAssetGroupPage, LibraryRepositoryError> {
        let mut sql = String::from(
            "WITH duplicate_groups AS (
                SELECT content_hash, count(*) AS asset_count, max(updated_at) AS updated_at, min(id) AS representative_asset_id
                  FROM assets
                 WHERE content_hash IS NOT NULL
                 GROUP BY content_hash
                HAVING count(*) > 1
             )
             SELECT dg.content_hash, dg.asset_count, dg.representative_asset_id, a.file_name, dg.updated_at
               FROM duplicate_groups dg
               JOIN assets a ON a.id=dg.representative_asset_id",
        );
        let mut values = Vec::<Value>::new();
        if let Some(cursor) = cursor {
            sql.push_str(" WHERE dg.updated_at < ? OR (dg.updated_at = ? AND dg.content_hash < ?)");
            values.push(cursor.updated_at.into());
            values.push(cursor.updated_at.into());
            values.push(cursor.content_hash.into());
        }
        sql.push_str(" ORDER BY dg.updated_at DESC, dg.content_hash DESC LIMIT ?");
        values.push((i64::from(limit) + 1).into());
        let mut statement = connection.prepare(&sql).map_err(map_db_error)?;
        let rows = statement
            .query_map(params_from_iter(values), |row| {
                Ok(crate::domain::DuplicateAssetGroup {
                    content_hash: row.get(0)?,
                    asset_count: row.get(1)?,
                    representative_asset_id: row.get(2)?,
                    representative_file_name: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            })
            .map_err(map_db_error)?;
        let mut items = rows.collect::<Result<Vec<_>, _>>().map_err(map_db_error)?;
        let has_more = items.len() > limit as usize;
        if has_more {
            items.pop();
        }
        let next_cursor = has_more.then(|| {
            let item = items.last().expect("超过页大小时必有最后一项");
            crate::domain::DuplicateGroupCursor {
                updated_at: item.updated_at,
                content_hash: item.content_hash.clone(),
            }
        });
        Ok(crate::domain::DuplicateAssetGroupPage { items, next_cursor })
    }

    pub(crate) fn get_asset(
        connection: &Connection,
        id: i64,
    ) -> Result<AssetDetail, LibraryRepositoryError> {
        let summary = connection
            .query_row(
                "SELECT a.id, a.project_id, a.media_type, a.path_kind, a.stored_path, a.file_name,
                    a.mime_type, a.file_size, a.content_hash, a.width, a.height, a.duration_ms,
                    a.frame_rate, a.has_audio, coalesce(p.prompt_zh, ''), coalesce(p.prompt_en, ''),
                    coalesce(p.negative_prompt, ''), m.name, pl.name, a.generation_params_json,
                    a.rating, a.is_favorite, a.is_public, a.notes, a.created_at, a.updated_at,
                    ao.position
             FROM assets a LEFT JOIN prompts p ON p.id=a.prompt_id
             LEFT JOIN models m ON m.id=a.model_id LEFT JOIN platforms pl ON pl.id=a.platform_id
             JOIN asset_display_order ao ON ao.asset_id=a.id
             WHERE a.id=?1",
                [id],
                asset_from_row,
            )
            .optional()
            .map_err(map_db_error)?
            .ok_or(LibraryRepositoryError::NotFound)?;
        Ok(AssetDetail {
            summary,
            category_ids: relation_ids(connection, "asset_categories", "asset_id", id)?,
            tag_ids: relation_ids(connection, "asset_tags", "asset_id", id)?,
        })
    }

    pub(crate) fn create_asset(
        connection: &mut Connection,
        input: &CreateAsset,
        now: i64,
    ) -> Result<i64, LibraryRepositoryError> {
        let transaction = connection.transaction().map_err(map_db_error)?;
        let id = insert_asset(&transaction, input, now)?;
        transaction.commit().map_err(map_db_error)?;
        Ok(id)
    }

    /// 同一事务内写入导入记录，并尽可能保留原始文件的修改时间。
    pub(crate) fn create_imported_assets_batch(
        connection: &mut Connection,
        inputs: &[(CreateAsset, Option<i64>)],
        now: i64,
    ) -> Result<Vec<i64>, LibraryRepositoryError> {
        let transaction = connection.transaction().map_err(map_db_error)?;
        let ids = inputs
            .iter()
            .map(|(input, source_modified_at)| {
                insert_asset(&transaction, input, source_modified_at.unwrap_or(now))
            })
            .collect::<Result<Vec<_>, _>>()?;
        transaction.commit().map_err(map_db_error)?;
        Ok(ids)
    }

    pub(crate) fn update_asset(
        connection: &mut Connection,
        id: i64,
        input: &UpdateAsset,
        now: i64,
    ) -> Result<(), LibraryRepositoryError> {
        let transaction = connection.transaction().map_err(map_db_error)?;
        let previous = Self::get_asset(&transaction, id)?;
        if previous.summary.media.project_id != input.media.project_id {
            ensure_canvas_output_invariant_after_removing_assets(&transaction, &[id], None)?;
            ensure_canvas_references_safe_after_removing_assets(&transaction, &[id], None)?;
        }
        let mut changed_fields = Vec::new();
        if previous.summary.media != input.media {
            changed_fields.push("media");
        }
        if previous.summary.prompt.prompt_zh != input.prompt.prompt_zh {
            changed_fields.push("prompt_zh");
        }
        if previous.summary.prompt.prompt_en != input.prompt.prompt_en {
            changed_fields.push("prompt_en");
        }
        if previous.summary.prompt.negative_prompt != input.prompt.negative_prompt {
            changed_fields.push("negative_prompt");
        }
        if normalized_optional_text(previous.summary.model.as_deref())
            != normalized_optional_text(input.model.as_deref())
        {
            changed_fields.push("model");
        }
        if normalized_optional_text(previous.summary.platform.as_deref())
            != normalized_optional_text(input.platform.as_deref())
        {
            changed_fields.push("platform");
        }
        if previous.summary.generation_params != input.generation_params {
            changed_fields.push("generation_params");
        }
        if previous.summary.rating != input.rating {
            changed_fields.push("rating");
        }
        if previous.summary.is_favorite != input.is_favorite {
            changed_fields.push("is_favorite");
        }
        if previous.summary.is_public != input.is_public {
            changed_fields.push("is_public");
        }
        if previous.summary.notes != input.notes {
            changed_fields.push("notes");
        }
        if !same_ids(&previous.category_ids, &input.category_ids) {
            changed_fields.push("category_ids");
        }
        if !same_ids(&previous.tag_ids, &input.tag_ids) {
            changed_fields.push("tag_ids");
        }
        let prompt_id: Option<i64> = transaction
            .query_row("SELECT prompt_id FROM assets WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .optional()
            .map_err(map_db_error)?
            .ok_or(LibraryRepositoryError::NotFound)?;
        let prompt_id = match prompt_id {
            Some(prompt_id) => {
                transaction.execute("UPDATE prompts SET prompt_zh=?1,prompt_en=?2,negative_prompt=?3,updated_at=max(updated_at,?4) WHERE id=?5",
                    params![input.prompt.prompt_zh, input.prompt.prompt_en, input.prompt.negative_prompt, now, prompt_id]).map_err(map_db_error)?;
                prompt_id
            }
            None => insert_asset_prompt(&transaction, &input.prompt, now)?,
        };
        let model_id = get_or_create_named(&transaction, "models", input.model.as_deref(), now)?;
        let platform_id =
            get_or_create_named(&transaction, "platforms", input.platform.as_deref(), now)?;
        let params_json = serde_json::to_string(&input.generation_params)
            .map_err(|_| LibraryRepositoryError::InvalidData)?;
        ensure_changed(transaction.execute(
            "UPDATE assets SET project_id=?1,prompt_id=?2,model_id=?3,platform_id=?4,media_type=?5,
             path_kind=?6,stored_path=?7,file_name=?8,mime_type=?9,file_size=?10,content_hash=?11,
             width=?12,height=?13,duration_ms=?14,frame_rate=?15,has_audio=?16,
             generation_params_json=?17,rating=?18,is_favorite=?19,is_public=?20,notes=?21,
             updated_at=max(updated_at,?22) WHERE id=?23",
            params![input.media.project_id,prompt_id,model_id,platform_id,input.media.media_type.as_db_str(),
                input.media.path_kind.as_db_str(),input.media.stored_path,input.media.file_name,
                input.media.mime_type,input.media.file_size,input.media.content_hash,input.media.width,
                input.media.height,input.media.duration_ms,input.media.frame_rate,input.media.has_audio,
                params_json,input.rating,input.is_favorite,input.is_public,input.notes,now,id],
        ).map_err(map_db_error)?)?;
        replace_categories(
            &transaction,
            "asset_categories",
            "asset_id",
            id,
            &input.category_ids,
            now,
        )?;
        replace_tags(
            &transaction,
            "asset_tags",
            "asset_id",
            id,
            &input.tag_ids,
            now,
        )?;
        if let Some(project_id) = input.media.project_id {
            ensure_canvas_output_for_asset(&transaction, project_id, id, now)?;
        }
        if !changed_fields.is_empty() {
            record_confirmed_edit(&transaction, "asset", id, "update", &changed_fields, now)?;
        }
        transaction.commit().map_err(map_db_error)
    }

    pub(crate) fn update_asset_display_order(
        connection: &mut Connection,
        id: i64,
        target_position: i64,
        mode: AssetOrderChangeMode,
    ) -> Result<(), LibraryRepositoryError> {
        let transaction = connection.transaction().map_err(map_db_error)?;
        let current_position: i64 = transaction
            .query_row(
                "SELECT position FROM asset_display_order WHERE asset_id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()
            .map_err(map_db_error)?
            .ok_or(LibraryRepositoryError::NotFound)?;
        let max_position: i64 = transaction
            .query_row(
                "SELECT COALESCE(MAX(position), 0) FROM asset_display_order",
                [],
                |row| row.get(0),
            )
            .map_err(map_db_error)?;
        if target_position < 1 || target_position > max_position {
            return Err(LibraryRepositoryError::InvalidData);
        }
        if current_position == target_position {
            return Ok(());
        }
        match mode {
            AssetOrderChangeMode::Swap => {
                transaction
                    .execute(
                        "UPDATE asset_display_order SET position=0 WHERE asset_id=?1",
                        [id],
                    )
                    .map_err(map_db_error)?;
                transaction
                    .execute(
                        "UPDATE asset_display_order SET position=?1 WHERE position=?2",
                        params![current_position, target_position],
                    )
                    .map_err(map_db_error)?;
                transaction
                    .execute(
                        "UPDATE asset_display_order SET position=?1 WHERE asset_id=?2",
                        params![target_position, id],
                    )
                    .map_err(map_db_error)?;
            }
            AssetOrderChangeMode::ShiftFollowing => {
                transaction
                    .execute(
                        "UPDATE asset_display_order SET position=0 WHERE asset_id=?1",
                        [id],
                    )
                    .map_err(map_db_error)?;
                if current_position > target_position {
                    transaction
                        .execute(
                            "UPDATE asset_display_order SET position=-position
                         WHERE position>=?1 AND position<?2",
                            params![target_position, current_position],
                        )
                        .map_err(map_db_error)?;
                    transaction
                        .execute(
                            "UPDATE asset_display_order SET position=-position+1 WHERE position<0",
                            [],
                        )
                        .map_err(map_db_error)?;
                } else {
                    transaction
                        .execute(
                            "UPDATE asset_display_order SET position=-position
                         WHERE position>?1 AND position<=?2",
                            params![current_position, target_position],
                        )
                        .map_err(map_db_error)?;
                    transaction
                        .execute(
                            "UPDATE asset_display_order SET position=-position-1 WHERE position<0",
                            [],
                        )
                        .map_err(map_db_error)?;
                }
                transaction
                    .execute(
                        "UPDATE asset_display_order SET position=?1 WHERE asset_id=?2",
                        params![target_position, id],
                    )
                    .map_err(map_db_error)?;
            }
        }
        transaction.commit().map_err(map_db_error)
    }

    /// 只读取目标与关系数量，不加载媒体，也不修改任何正式字段。
    pub(crate) fn preview_bulk_asset_edit(
        connection: &Connection,
        input: &BulkAssetEditInput,
    ) -> Result<BulkAssetEditPreview, LibraryRepositoryError> {
        let normalized = normalize_bulk_asset_edit(connection, input)?;
        validate_bulk_asset_edit(connection, &normalized)?;
        bulk_asset_edit_preview(connection, &normalized)
    }

    /// 在单一事务内应用整批变更；任何一个目标或关系失败都会回滚全批。
    pub(crate) fn bulk_edit_assets(
        connection: &mut Connection,
        input: &BulkAssetEditInput,
        now: i64,
    ) -> Result<BulkAssetEditPreview, LibraryRepositoryError> {
        let normalized = normalize_bulk_asset_edit(connection, input)?;
        let transaction = connection.transaction().map_err(map_db_error)?;
        validate_bulk_asset_edit(&transaction, &normalized)?;
        let preview = bulk_asset_edit_preview(&transaction, &normalized)?;

        let (change_model, model_id) =
            resolve_bulk_named(&transaction, "models", &normalized.model, now)?;
        let (change_platform, platform_id) =
            resolve_bulk_named(&transaction, "platforms", &normalized.platform, now)?;
        transaction
            .execute(
                "UPDATE assets
                    SET rating=coalesce(?1,rating),
                        is_favorite=coalesce(?2,is_favorite),
                        is_public=coalesce(?3,is_public),
                        model_id=CASE WHEN ?4 THEN ?5 ELSE model_id END,
                        platform_id=CASE WHEN ?6 THEN ?7 ELSE platform_id END,
                        updated_at=max(updated_at,?8)
                  WHERE id IN (SELECT value FROM json_each(?9))",
                params![
                    normalized.rating,
                    normalized.is_favorite,
                    normalized.is_public,
                    change_model,
                    model_id,
                    change_platform,
                    platform_id,
                    now,
                    normalized.asset_ids_json,
                ],
            )
            .map_err(map_db_error)?;

        apply_bulk_relation_changes(
            &transaction,
            "asset_categories",
            "category_id",
            &normalized.asset_ids_json,
            &normalized.add_category_ids_json,
            &normalized.remove_category_ids_json,
            now,
        )?;
        apply_bulk_relation_changes(
            &transaction,
            "asset_tags",
            "tag_id",
            &normalized.asset_ids_json,
            &normalized.add_tag_ids_json,
            &normalized.remove_tag_ids_json,
            now,
        )?;
        let mut changed_fields = Vec::with_capacity(7);
        if normalized.rating.is_some() {
            changed_fields.push("rating");
        }
        if normalized.is_favorite.is_some() {
            changed_fields.push("is_favorite");
        }
        if normalized.is_public.is_some() {
            changed_fields.push("is_public");
        }
        if normalized.model != BulkNullableTextEdit::Keep {
            changed_fields.push("model");
        }
        if normalized.platform != BulkNullableTextEdit::Keep {
            changed_fields.push("platform");
        }
        if normalized.add_category_count + normalized.remove_category_count > 0 {
            changed_fields.push("category_ids");
        }
        if normalized.add_tag_count + normalized.remove_tag_count > 0 {
            changed_fields.push("tag_ids");
        }
        let fields_json = serde_json::to_string(&changed_fields)
            .map_err(|_| LibraryRepositoryError::InvalidData)?;
        transaction.execute(
            "INSERT INTO edit_history(target_type,target_id,action,changed_fields_json,source,status,created_at,updated_at)
             SELECT 'asset',CAST(value AS INTEGER),'bulk_update',?1,'manual','confirmed',?2,?2
             FROM json_each(?3)",
            params![fields_json, now, normalized.asset_ids_json],
        ).map_err(map_db_error)?;
        transaction.commit().map_err(map_db_error)?;
        Ok(preview)
    }

    /// 按项目或基准作品的三类提示词精确匹配，使用稳定游标返回轻量摘要。
    pub(crate) fn list_model_comparison(
        connection: &Connection,
        query: ModelComparisonQuery,
    ) -> Result<Page<ModelComparisonItem>, LibraryRepositoryError> {
        if !(crate::domain::MIN_PAGE_LIMIT..=crate::domain::MAX_PAGE_LIMIT).contains(&query.limit) {
            return Err(LibraryRepositoryError::InvalidData);
        }
        let filter = match query.scope {
            ModelComparisonScope::Project { project_id } => {
                if project_id <= 0 {
                    return Err(LibraryRepositoryError::InvalidData);
                }
                let exists: bool = connection
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
                        [project_id],
                        |row| row.get(0),
                    )
                    .map_err(map_db_error)?;
                if !exists {
                    return Err(LibraryRepositoryError::NotFound);
                }
                ModelComparisonFilter::Project(project_id)
            }
            ModelComparisonScope::MatchingPrompt { baseline_asset_id } => {
                if baseline_asset_id <= 0 {
                    return Err(LibraryRepositoryError::InvalidData);
                }
                let prompt = connection
                    .query_row(
                        "SELECT coalesce(p.prompt_zh,''),coalesce(p.prompt_en,''),
                                coalesce(p.negative_prompt,'')
                           FROM assets a LEFT JOIN prompts p ON p.id=a.prompt_id
                          WHERE a.id=?1",
                        [baseline_asset_id],
                        |row| {
                            Ok(PromptText {
                                prompt_zh: row.get(0)?,
                                prompt_en: row.get(1)?,
                                negative_prompt: row.get(2)?,
                            })
                        },
                    )
                    .optional()
                    .map_err(map_db_error)?
                    .ok_or(LibraryRepositoryError::NotFound)?;
                ModelComparisonFilter::MatchingPrompt(prompt)
            }
        };
        let (sql, values) = Self::build_model_comparison_sql(&filter, query.cursor, query.limit);
        let mut statement = connection.prepare(&sql).map_err(map_db_error)?;
        let rows = statement
            .query_map(params_from_iter(values), |row| {
                Ok(ModelComparisonItem {
                    id: row.get(0)?,
                    file_name: row.get(1)?,
                    model: row.get(2)?,
                    platform: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            })
            .map_err(map_db_error)?;
        let mut items = rows.collect::<Result<Vec<_>, _>>().map_err(map_db_error)?;
        Ok(build_page(&mut items, query.limit, |item| PageCursor {
            updated_at: item.updated_at,
            id: item.id,
        }))
    }

    fn build_model_comparison_sql(
        filter: &ModelComparisonFilter,
        cursor: Option<PageCursor>,
        limit: u32,
    ) -> (String, Vec<Value>) {
        let mut sql = String::from(
            "SELECT a.id,a.file_name,coalesce(m.name,''),coalesce(pl.name,''),a.updated_at
               FROM assets a LEFT JOIN prompts p ON p.id=a.prompt_id
               LEFT JOIN models m ON m.id=a.model_id LEFT JOIN platforms pl ON pl.id=a.platform_id
              WHERE ",
        );
        let mut values = Vec::<Value>::new();
        match filter {
            ModelComparisonFilter::Project(project_id) => {
                sql.push_str("a.project_id=?");
                values.push((*project_id).into());
            }
            ModelComparisonFilter::MatchingPrompt(prompt) => {
                sql.push_str(
                    "coalesce(p.prompt_zh,'')=? AND coalesce(p.prompt_en,'')=?
                     AND coalesce(p.negative_prompt,'')=?",
                );
                values.push(prompt.prompt_zh.clone().into());
                values.push(prompt.prompt_en.clone().into());
                values.push(prompt.negative_prompt.clone().into());
            }
        }
        if let Some(cursor) = cursor {
            sql.push_str(" AND (a.updated_at < ? OR (a.updated_at = ? AND a.id < ?))");
            values.push(cursor.updated_at.into());
            values.push(cursor.updated_at.into());
            values.push(cursor.id.into());
        }
        sql.push_str(" ORDER BY a.updated_at DESC,a.id DESC LIMIT ?");
        values.push((i64::from(limit) + 1).into());
        (sql, values)
    }

    pub(crate) fn set_project_categories(
        connection: &mut Connection,
        id: i64,
        ids: &[i64],
        now: i64,
    ) -> Result<(), LibraryRepositoryError> {
        set_category_relations(
            connection,
            "projects",
            "project_categories",
            "project_id",
            id,
            ids,
            now,
        )
    }
    pub(crate) fn set_asset_categories(
        connection: &mut Connection,
        id: i64,
        ids: &[i64],
        now: i64,
    ) -> Result<(), LibraryRepositoryError> {
        set_category_relations(
            connection,
            "assets",
            "asset_categories",
            "asset_id",
            id,
            ids,
            now,
        )
    }
    pub(crate) fn set_project_tags(
        connection: &mut Connection,
        id: i64,
        ids: &[i64],
        now: i64,
    ) -> Result<(), LibraryRepositoryError> {
        set_tag_relations(
            connection,
            "projects",
            "project_tags",
            "project_id",
            id,
            ids,
            now,
        )
    }
    pub(crate) fn set_asset_tags(
        connection: &mut Connection,
        id: i64,
        ids: &[i64],
        now: i64,
    ) -> Result<(), LibraryRepositoryError> {
        set_tag_relations(connection, "assets", "asset_tags", "asset_id", id, ids, now)
    }

    pub(crate) fn list_metadata_presets(
        connection: &Connection,
    ) -> Result<MetadataPresets, LibraryRepositoryError> {
        fn list(
            connection: &Connection,
            kind: MetadataPresetKind,
        ) -> Result<Vec<MetadataPreset>, LibraryRepositoryError> {
            let sql = match kind {
                MetadataPresetKind::Model => {
                    "SELECT m.id,m.name,count(a.id)
                       FROM models m LEFT JOIN assets a ON a.model_id=m.id
                      GROUP BY m.id,m.name ORDER BY m.name COLLATE NOCASE,m.id"
                }
                MetadataPresetKind::Platform => {
                    "SELECT p.id,p.name,count(a.id)
                       FROM platforms p LEFT JOIN assets a ON a.platform_id=p.id
                      GROUP BY p.id,p.name ORDER BY p.name COLLATE NOCASE,p.id"
                }
            };
            let mut statement = connection.prepare(sql).map_err(map_db_error)?;
            let rows = statement
                .query_map([], |row| {
                    Ok(MetadataPreset {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        asset_count: row.get(2)?,
                    })
                })
                .map_err(map_db_error)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(map_db_error)
        }
        Ok(MetadataPresets {
            models: list(connection, MetadataPresetKind::Model)?,
            platforms: list(connection, MetadataPresetKind::Platform)?,
        })
    }

    pub(crate) fn create_metadata_preset(
        connection: &mut Connection,
        kind: MetadataPresetKind,
        name: &str,
        now: i64,
    ) -> Result<i64, LibraryRepositoryError> {
        let sql = match kind {
            MetadataPresetKind::Model => {
                "INSERT INTO models(name,provider,created_at,updated_at) VALUES(?1,'',?2,?2)"
            }
            MetadataPresetKind::Platform => {
                "INSERT INTO platforms(name,created_at,updated_at) VALUES(?1,?2,?2)"
            }
        };
        connection
            .execute(sql, params![name.trim(), now])
            .map_err(map_db_error)?;
        Ok(connection.last_insert_rowid())
    }

    pub(crate) fn update_metadata_preset(
        connection: &mut Connection,
        kind: MetadataPresetKind,
        id: i64,
        name: &str,
        now: i64,
    ) -> Result<(), LibraryRepositoryError> {
        let sql = match kind {
            MetadataPresetKind::Model => {
                "UPDATE models SET name=?1,updated_at=max(updated_at,?2) WHERE id=?3"
            }
            MetadataPresetKind::Platform => {
                "UPDATE platforms SET name=?1,updated_at=max(updated_at,?2) WHERE id=?3"
            }
        };
        ensure_changed(
            connection
                .execute(sql, params![name.trim(), now, id])
                .map_err(map_db_error)?,
        )
    }

    pub(crate) fn delete_metadata_preset(
        connection: &mut Connection,
        kind: MetadataPresetKind,
        id: i64,
    ) -> Result<(), LibraryRepositoryError> {
        let sql = match kind {
            MetadataPresetKind::Model => "DELETE FROM models WHERE id=?1",
            MetadataPresetKind::Platform => "DELETE FROM platforms WHERE id=?1",
        };
        ensure_changed(connection.execute(sql, [id]).map_err(map_db_error)?)
    }

    pub(crate) fn list_dimensions(
        connection: &Connection,
    ) -> Result<Vec<Dimension>, LibraryRepositoryError> {
        let mut statement = connection.prepare("SELECT id,name,allows_multiple,ai_can_suggest_new,is_enabled,created_at,updated_at FROM dimensions ORDER BY id").map_err(map_db_error)?;
        statement
            .query_map([], |r| {
                Ok(Dimension {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    allows_multiple: r.get(2)?,
                    ai_can_suggest_new: r.get(3)?,
                    is_enabled: r.get(4)?,
                    created_at: r.get(5)?,
                    updated_at: r.get(6)?,
                })
            })
            .map_err(map_db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_db_error)
    }

    pub(crate) fn create_dimension(
        connection: &mut Connection,
        input: &DimensionInput,
        now: i64,
    ) -> Result<i64, LibraryRepositoryError> {
        let tx = connection.transaction().map_err(map_db_error)?;
        tx.execute("INSERT INTO dimensions(name,allows_multiple,ai_can_suggest_new,is_enabled,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?5)", params![input.name.trim(),input.allows_multiple,input.ai_can_suggest_new,input.is_enabled,now]).map_err(map_db_error)?;
        let id = tx.last_insert_rowid();
        tx.commit().map_err(map_db_error)?;
        Ok(id)
    }

    pub(crate) fn update_dimension(
        connection: &mut Connection,
        id: i64,
        input: &DimensionInput,
        now: i64,
    ) -> Result<(), LibraryRepositoryError> {
        let tx = connection.transaction().map_err(map_db_error)?;
        ensure_changed(tx.execute("UPDATE dimensions SET name=?1,allows_multiple=?2,ai_can_suggest_new=?3,is_enabled=?4,updated_at=max(updated_at,?5) WHERE id=?6",params![input.name.trim(),input.allows_multiple,input.ai_can_suggest_new,input.is_enabled,now,id]).map_err(map_db_error)?)?;
        if !input.allows_multiple {
            let invalid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM (SELECT project_id FROM project_categories pc JOIN categories c ON c.id=pc.category_id WHERE c.dimension_id=?1 GROUP BY project_id HAVING count(*)>1 UNION ALL SELECT asset_id FROM asset_categories ac JOIN categories c ON c.id=ac.category_id WHERE c.dimension_id=?1 GROUP BY asset_id HAVING count(*)>1))",[id],|r|r.get(0)).map_err(map_db_error)?;
            if invalid {
                return Err(LibraryRepositoryError::Conflict);
            }
        }
        tx.commit().map_err(map_db_error)
    }

    pub(crate) fn delete_dimension(
        connection: &mut Connection,
        id: i64,
    ) -> Result<(), LibraryRepositoryError> {
        let tx = connection.transaction().map_err(map_db_error)?;
        let has_categories: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM categories WHERE dimension_id=?1)",
                [id],
                |r| r.get(0),
            )
            .map_err(map_db_error)?;
        if has_categories {
            return Err(LibraryRepositoryError::Conflict);
        }
        ensure_changed(
            tx.execute("DELETE FROM dimensions WHERE id=?1", [id])
                .map_err(map_db_error)?,
        )?;
        tx.commit().map_err(map_db_error)
    }

    pub(crate) fn list_categories(
        connection: &Connection,
        dimension_id: Option<i64>,
    ) -> Result<Vec<Category>, LibraryRepositoryError> {
        let mut statement=connection.prepare("SELECT c.id,c.dimension_id,c.name,c.aliases_json,c.description,c.color,c.icon,c.is_enabled,(SELECT count(*) FROM asset_categories ac WHERE ac.category_id=c.id),c.created_at,c.updated_at FROM categories c WHERE (?1 IS NULL OR c.dimension_id=?1) ORDER BY c.dimension_id,c.id").map_err(map_db_error)?;
        let rows = statement
            .query_map([dimension_id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, bool>(7)?,
                    r.get::<_, i64>(8)?,
                    r.get::<_, i64>(9)?,
                    r.get::<_, i64>(10)?,
                ))
            })
            .map_err(map_db_error)?;
        rows.map(|row| {
            let (
                id,
                dimension_id,
                name,
                aliases,
                description,
                color,
                icon,
                is_enabled,
                asset_count,
                created_at,
                updated_at,
            ) = row.map_err(map_db_error)?;
            Ok(Category {
                id,
                dimension_id,
                name,
                aliases: parse_json(&aliases)?,
                description,
                color,
                icon,
                is_enabled,
                asset_count,
                created_at,
                updated_at,
            })
        })
        .collect()
    }

    pub(crate) fn create_category(
        connection: &mut Connection,
        input: &CategoryInput,
        now: i64,
    ) -> Result<i64, LibraryRepositoryError> {
        let aliases = serde_json::to_string(&input.aliases)
            .map_err(|_| LibraryRepositoryError::InvalidData)?;
        let tx = connection.transaction().map_err(map_db_error)?;
        tx.execute("INSERT INTO categories(dimension_id,name,aliases_json,description,color,icon,is_enabled,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?8)",params![input.dimension_id,input.name.trim(),aliases,input.description,input.color,input.icon,input.is_enabled,now]).map_err(map_db_error)?;
        let id = tx.last_insert_rowid();
        tx.commit().map_err(map_db_error)?;
        Ok(id)
    }

    pub(crate) fn update_category(
        connection: &mut Connection,
        id: i64,
        input: &CategoryInput,
        now: i64,
    ) -> Result<(), LibraryRepositoryError> {
        let aliases = serde_json::to_string(&input.aliases)
            .map_err(|_| LibraryRepositoryError::InvalidData)?;
        let tx = connection.transaction().map_err(map_db_error)?;
        ensure_changed(tx.execute("UPDATE categories SET dimension_id=?1,name=?2,aliases_json=?3,description=?4,color=?5,icon=?6,is_enabled=?7,updated_at=max(updated_at,?8) WHERE id=?9",params![input.dimension_id,input.name.trim(),aliases,input.description,input.color,input.icon,input.is_enabled,now,id]).map_err(map_db_error)?)?;
        validate_existing_relations_for_category(&tx, id, input.dimension_id)?;
        tx.commit().map_err(map_db_error)
    }

    pub(crate) fn category_impact(
        connection: &Connection,
        id: i64,
    ) -> Result<RelationImpact, LibraryRepositoryError> {
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM categories WHERE id=?1)",
                [id],
                |row| row.get(0),
            )
            .map_err(map_db_error)?;
        if !exists {
            return Err(LibraryRepositoryError::NotFound);
        }
        connection.query_row("SELECT (SELECT count(*) FROM project_categories WHERE category_id=?1),(SELECT count(*) FROM asset_categories WHERE category_id=?1),(SELECT count(*) FROM ai_suggestions WHERE category_id=?1)",[id],|r|Ok(RelationImpact{project_count:r.get(0)?,asset_count:r.get(1)?,suggestion_count:r.get(2)?})).optional().map_err(map_db_error)?.ok_or(LibraryRepositoryError::NotFound)
    }

    pub(crate) fn delete_category(
        connection: &mut Connection,
        id: i64,
        action: CategoryDeleteAction,
    ) -> Result<RelationImpact, LibraryRepositoryError> {
        let tx = connection.transaction().map_err(map_db_error)?;
        let (dimension_id, name): (i64, String) = tx
            .query_row(
                "SELECT dimension_id,name FROM categories WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(map_db_error)?
            .ok_or(LibraryRepositoryError::NotFound)?;
        let impact = Self::category_impact(&tx, id)?;
        match action {
            CategoryDeleteAction::Remove => {
                tx.execute("DELETE FROM project_categories WHERE category_id=?1", [id])
                    .map_err(map_db_error)?;
                tx.execute("DELETE FROM asset_categories WHERE category_id=?1", [id])
                    .map_err(map_db_error)?;
                tx.execute("UPDATE ai_suggestions SET category_id=NULL,suggested_category_name=coalesce(suggested_category_name,?2) WHERE category_id=?1",params![id,name]).map_err(map_db_error)?;
            }
            CategoryDeleteAction::ReplaceWith(replacement) => {
                let replacement_dimension: i64 = tx
                    .query_row(
                        "SELECT dimension_id FROM categories WHERE id=?1",
                        [replacement],
                        |r| r.get(0),
                    )
                    .optional()
                    .map_err(map_db_error)?
                    .ok_or(LibraryRepositoryError::NotFound)?;
                if replacement == id || replacement_dimension != dimension_id {
                    return Err(LibraryRepositoryError::Conflict);
                }
                tx.execute("INSERT OR IGNORE INTO project_categories(project_id,category_id,created_at) SELECT project_id,?2,created_at FROM project_categories WHERE category_id=?1",params![id,replacement]).map_err(map_db_error)?;
                tx.execute("INSERT OR IGNORE INTO asset_categories(asset_id,category_id,created_at) SELECT asset_id,?2,created_at FROM asset_categories WHERE category_id=?1",params![id,replacement]).map_err(map_db_error)?;
                tx.execute("DELETE FROM project_categories WHERE category_id=?1", [id])
                    .map_err(map_db_error)?;
                tx.execute("DELETE FROM asset_categories WHERE category_id=?1", [id])
                    .map_err(map_db_error)?;
                tx.execute(
                    "UPDATE ai_suggestions SET category_id=?2 WHERE category_id=?1",
                    params![id, replacement],
                )
                .map_err(map_db_error)?;
            }
        }
        tx.execute("DELETE FROM categories WHERE id=?1", [id])
            .map_err(map_db_error)?;
        tx.commit().map_err(map_db_error)?;
        Ok(impact)
    }

    pub(crate) fn list_tags(connection: &Connection) -> Result<Vec<Tag>, LibraryRepositoryError> {
        let mut statement = connection
            .prepare("SELECT t.id,t.name,(SELECT count(*) FROM asset_tags at WHERE at.tag_id=t.id),t.created_at,t.updated_at FROM tags t ORDER BY t.id")
            .map_err(map_db_error)?;
        statement
            .query_map([], |r| {
                Ok(Tag {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    asset_count: r.get(2)?,
                    created_at: r.get(3)?,
                    updated_at: r.get(4)?,
                })
            })
            .map_err(map_db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_db_error)
    }

    pub(crate) fn create_tag(
        connection: &mut Connection,
        name: &str,
        now: i64,
    ) -> Result<i64, LibraryRepositoryError> {
        let tx = connection.transaction().map_err(map_db_error)?;
        tx.execute(
            "INSERT INTO tags(name,created_at,updated_at) VALUES(?1,?2,?2)",
            params![name.trim(), now],
        )
        .map_err(map_db_error)?;
        let id = tx.last_insert_rowid();
        tx.commit().map_err(map_db_error)?;
        Ok(id)
    }
    pub(crate) fn update_tag(
        connection: &mut Connection,
        id: i64,
        name: &str,
        now: i64,
    ) -> Result<(), LibraryRepositoryError> {
        let tx = connection.transaction().map_err(map_db_error)?;
        ensure_changed(
            tx.execute(
                "UPDATE tags SET name=?1,updated_at=max(updated_at,?2) WHERE id=?3",
                params![name.trim(), now, id],
            )
            .map_err(map_db_error)?,
        )?;
        tx.commit().map_err(map_db_error)
    }
    pub(crate) fn delete_tag(
        connection: &mut Connection,
        id: i64,
    ) -> Result<RelationImpact, LibraryRepositoryError> {
        let tx = connection.transaction().map_err(map_db_error)?;
        let impact=tx.query_row("SELECT (SELECT count(*) FROM project_tags WHERE tag_id=?1),(SELECT count(*) FROM asset_tags WHERE tag_id=?1)",[id],|r|Ok(RelationImpact{project_count:r.get(0)?,asset_count:r.get(1)?,suggestion_count:0})).map_err(map_db_error)?;
        tx.execute("DELETE FROM project_tags WHERE tag_id=?1", [id])
            .map_err(map_db_error)?;
        tx.execute("DELETE FROM asset_tags WHERE tag_id=?1", [id])
            .map_err(map_db_error)?;
        ensure_changed(
            tx.execute("DELETE FROM tags WHERE id=?1", [id])
                .map_err(map_db_error)?,
        )?;
        tx.commit().map_err(map_db_error)?;
        Ok(impact)
    }

    pub(crate) fn move_to_trash(
        connection: &mut Connection,
        entity_type: TrashEntityType,
        id: i64,
        now: i64,
    ) -> Result<i64, LibraryRepositoryError> {
        let tx = connection.transaction().map_err(map_db_error)?;
        let trash_id = move_to_trash_in_transaction(&tx, entity_type, id, now)?;
        tx.commit().map_err(map_db_error)?;
        Ok(trash_id)
    }

    pub(crate) fn move_assets_to_trash(
        connection: &mut Connection,
        asset_ids: &[i64],
        now: i64,
    ) -> Result<Vec<i64>, LibraryRepositoryError> {
        let tx = connection.transaction().map_err(map_db_error)?;
        let mut trash_ids = Vec::with_capacity(asset_ids.len());
        for asset_id in asset_ids {
            trash_ids.push(move_to_trash_in_transaction(
                &tx,
                TrashEntityType::Asset,
                *asset_id,
                now,
            )?);
        }
        tx.commit().map_err(map_db_error)?;
        Ok(trash_ids)
    }

    pub(crate) fn restore_trash_batch(
        connection: &mut Connection,
        trash_ids: &[i64],
    ) -> Result<(), LibraryRepositoryError> {
        let tx = connection.transaction().map_err(map_db_error)?;
        for trash_id in trash_ids {
            restore_trash_in_transaction(&tx, *trash_id)?;
        }
        tx.commit().map_err(map_db_error)
    }

    pub(crate) fn purge_trash_batch(
        connection: &mut Connection,
        trash_ids: &[i64],
    ) -> Result<(), LibraryRepositoryError> {
        let tx = connection.transaction().map_err(map_db_error)?;
        for trash_id in trash_ids {
            ensure_changed(
                tx.execute("DELETE FROM trash_entries WHERE id=?1", [trash_id])
                    .map_err(map_db_error)?,
            )?;
        }
        tx.commit().map_err(map_db_error)
    }

    pub(crate) fn list_trash(
        connection: &Connection,
        cursor: Option<PageCursor>,
        limit: u32,
    ) -> Result<Page<TrashEntry>, LibraryRepositoryError> {
        let (cursor_time, cursor_id) = cursor
            .map(|value| (Some(value.updated_at), Some(value.id)))
            .unwrap_or((None, None));
        let mut statement = connection
            .prepare(
                "SELECT id,entity_type,entity_id,snapshot_json,deleted_at FROM trash_entries
                 WHERE (?1 IS NULL OR deleted_at < ?1 OR (deleted_at = ?1 AND id < ?2))
                 ORDER BY deleted_at DESC,id DESC LIMIT ?3",
            )
            .map_err(map_db_error)?;
        let rows = statement
            .query_map(
                params![cursor_time, cursor_id, i64::from(limit) + 1],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .map_err(map_db_error)?;
        let mut items = rows
            .map(|row| {
                let (id, kind, entity_id, snapshot, deleted_at) = row.map_err(map_db_error)?;
                trash_entry_from_snapshot(id, &kind, entity_id, &snapshot, deleted_at)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(build_page(&mut items, limit, |item| PageCursor {
            updated_at: item.deleted_at,
            id: item.id,
        }))
    }

    pub(crate) fn restore_trash(
        connection: &mut Connection,
        trash_id: i64,
    ) -> Result<TrashEntry, LibraryRepositoryError> {
        let tx = connection.transaction().map_err(map_db_error)?;
        let entry = restore_trash_in_transaction(&tx, trash_id)?;
        tx.commit().map_err(map_db_error)?;
        Ok(entry)
    }

    pub(crate) fn undo_recent_delete(
        connection: &mut Connection,
    ) -> Result<TrashEntry, LibraryRepositoryError> {
        let tx = connection.transaction().map_err(map_db_error)?;
        let id: i64 = tx
            .query_row(
                "SELECT id FROM trash_entries ORDER BY deleted_at DESC,id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(map_db_error)?
            .ok_or(LibraryRepositoryError::NotFound)?;
        let entry = restore_trash_in_transaction(&tx, id)?;
        tx.commit().map_err(map_db_error)?;
        Ok(entry)
    }
    pub(crate) fn purge_trash(
        connection: &mut Connection,
        trash_id: i64,
    ) -> Result<(), LibraryRepositoryError> {
        Self::purge_trash_batch(connection, &[trash_id])
    }
}

fn move_to_trash_in_transaction(
    tx: &Transaction<'_>,
    entity_type: TrashEntityType,
    id: i64,
    now: i64,
) -> Result<i64, LibraryRepositoryError> {
    if entity_type == TrashEntityType::Asset {
        ensure_canvas_output_invariant_after_removing_assets(tx, &[id], None)?;
        ensure_canvas_references_safe_after_removing_assets(tx, &[id], None)?;
    }
    let snapshot = match entity_type {
        TrashEntityType::Project => serde_json::to_string(&project_snapshot(tx, id)?),
        TrashEntityType::Asset => serde_json::to_string(&asset_snapshot(tx, id)?),
    }
    .map_err(|_| LibraryRepositoryError::InvalidData)?;
    tx.execute("INSERT INTO trash_entries(entity_type,entity_id,snapshot_json,media_action,deleted_at) VALUES(?1,?2,?3,'keep',?4)",params![entity_type.as_db_str(),id,snapshot,now]).map_err(map_db_error)?;
    let trash_id = tx.last_insert_rowid();
    // 建议目标采用多态引用；移入回收站会删除实体，因此必须在同一事务中关闭
    // 尚未审核的建议。恢复实体不会复活旧建议，避免用户误应用过期结果。
    tx.execute(
        "UPDATE ai_suggestions SET status='rejected',updated_at=max(updated_at,?1)
             WHERE target_type=?2 AND target_id=?3 AND status='pending'",
        params![now, entity_type.as_db_str(), id],
    )
    .map_err(map_db_error)?;
    match entity_type {
        TrashEntityType::Project => {
            remove_p1_entity_state(tx, "project", id)?;
            tx.execute(
                "DELETE FROM edit_history WHERE target_type='prompt' AND target_id IN (
                        SELECT p.id FROM prompts p WHERE p.project_id=?1
                        AND NOT EXISTS(SELECT 1 FROM assets a WHERE a.prompt_id=p.id)
                    )",
                [id],
            )
            .map_err(map_db_error)?;
            tx.execute("DELETE FROM prompts WHERE project_id=?1 AND NOT EXISTS(SELECT 1 FROM assets WHERE prompt_id=prompts.id)",[id]).map_err(map_db_error)?;
            tx.execute("DELETE FROM projects WHERE id=?1", [id])
                .map_err(map_db_error)?;
        }
        TrashEntityType::Asset => {
            let prompt_id: Option<i64> = tx
                .query_row("SELECT prompt_id FROM assets WHERE id=?1", [id], |r| {
                    r.get(0)
                })
                .map_err(map_db_error)?;
            remove_p1_entity_state(tx, "asset", id)?;
            tx.execute("DELETE FROM assets WHERE id=?1", [id])
                .map_err(map_db_error)?;
            if let Some(prompt_id) = prompt_id {
                let still_referenced: bool = tx
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM assets WHERE prompt_id=?1)",
                        [prompt_id],
                        |row| row.get(0),
                    )
                    .map_err(map_db_error)?;
                if !still_referenced {
                    tx.execute(
                        "DELETE FROM edit_history WHERE target_type='prompt' AND target_id=?1",
                        [prompt_id],
                    )
                    .map_err(map_db_error)?;
                    tx.execute("DELETE FROM prompts WHERE id=?1", [prompt_id])
                        .map_err(map_db_error)?;
                }
            }
        }
    }
    Ok(trash_id)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ModelComparisonFilter {
    Project(i64),
    MatchingPrompt(PromptText),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedBulkAssetEdit {
    asset_ids: Vec<i64>,
    asset_ids_json: String,
    rating: Option<u8>,
    is_favorite: Option<bool>,
    is_public: Option<bool>,
    model: BulkNullableTextEdit,
    platform: BulkNullableTextEdit,
    add_category_ids_json: String,
    remove_category_ids_json: String,
    add_tag_ids_json: String,
    remove_tag_ids_json: String,
    add_category_count: usize,
    remove_category_count: usize,
    add_tag_count: usize,
    remove_tag_count: usize,
}

fn normalize_bulk_asset_edit(
    connection: &Connection,
    input: &BulkAssetEditInput,
) -> Result<NormalizedBulkAssetEdit, LibraryRepositoryError> {
    if input.asset_ids.is_empty()
        || input.asset_ids.len() > MAX_BULK_ASSET_IDS
        || input.rating.is_some_and(|rating| rating > 5)
    {
        return Err(LibraryRepositoryError::InvalidData);
    }
    let asset_ids = asset_ids_for_display_numbers(connection, &input.asset_ids)?;
    let add_category_ids = deduplicate_positive_ids(&input.add_category_ids)?;
    let remove_category_ids = deduplicate_positive_ids(&input.remove_category_ids)?;
    let add_tag_ids = deduplicate_positive_ids(&input.add_tag_ids)?;
    let remove_tag_ids = deduplicate_positive_ids(&input.remove_tag_ids)?;
    reject_overlapping_ids(&add_category_ids, &remove_category_ids)?;
    reject_overlapping_ids(&add_tag_ids, &remove_tag_ids)?;
    let model = normalize_bulk_text_edit(&input.model)?;
    let platform = normalize_bulk_text_edit(&input.platform)?;
    if input.rating.is_none()
        && input.is_favorite.is_none()
        && input.is_public.is_none()
        && model == BulkNullableTextEdit::Keep
        && platform == BulkNullableTextEdit::Keep
        && add_category_ids.is_empty()
        && remove_category_ids.is_empty()
        && add_tag_ids.is_empty()
        && remove_tag_ids.is_empty()
    {
        return Err(LibraryRepositoryError::InvalidData);
    }
    Ok(NormalizedBulkAssetEdit {
        asset_ids_json: ids_json(&asset_ids)?,
        asset_ids,
        rating: input.rating,
        is_favorite: input.is_favorite,
        is_public: input.is_public,
        model,
        platform,
        add_category_ids_json: ids_json(&add_category_ids)?,
        remove_category_ids_json: ids_json(&remove_category_ids)?,
        add_tag_ids_json: ids_json(&add_tag_ids)?,
        remove_tag_ids_json: ids_json(&remove_tag_ids)?,
        add_category_count: add_category_ids.len(),
        remove_category_count: remove_category_ids.len(),
        add_tag_count: add_tag_ids.len(),
        remove_tag_count: remove_tag_ids.len(),
    })
}

fn asset_ids_for_display_numbers(
    connection: &Connection,
    display_numbers: &[i64],
) -> Result<Vec<i64>, LibraryRepositoryError> {
    let display_numbers = deduplicate_positive_ids(display_numbers)?;
    let display_numbers_json = ids_json(&display_numbers)?;
    let mut statement = connection
        .prepare(
            "SELECT asset_id FROM asset_display_order
              WHERE position IN (SELECT value FROM json_each(?1))
              ORDER BY position",
        )
        .map_err(map_db_error)?;
    let rows = statement
        .query_map([&display_numbers_json], |row| row.get(0))
        .map_err(map_db_error)?;
    let asset_ids = rows
        .collect::<Result<Vec<i64>, _>>()
        .map_err(map_db_error)?;
    if asset_ids.len() != display_numbers.len() {
        return Err(LibraryRepositoryError::NotFound);
    }
    Ok(asset_ids)
}

fn deduplicate_positive_ids(ids: &[i64]) -> Result<Vec<i64>, LibraryRepositoryError> {
    let mut seen = HashSet::with_capacity(ids.len());
    let mut unique = Vec::with_capacity(ids.len());
    for id in ids {
        if *id <= 0 {
            return Err(LibraryRepositoryError::InvalidData);
        }
        if seen.insert(*id) {
            unique.push(*id);
        }
    }
    Ok(unique)
}

fn reject_overlapping_ids(left: &[i64], right: &[i64]) -> Result<(), LibraryRepositoryError> {
    let left = left.iter().copied().collect::<HashSet<_>>();
    if right.iter().any(|id| left.contains(id)) {
        Err(LibraryRepositoryError::InvalidData)
    } else {
        Ok(())
    }
}

fn normalize_bulk_text_edit(
    edit: &BulkNullableTextEdit,
) -> Result<BulkNullableTextEdit, LibraryRepositoryError> {
    match edit {
        BulkNullableTextEdit::Set(value) => {
            let value = value.trim();
            if value.is_empty() || value.chars().count() > MAX_METADATA_NAME_LENGTH {
                Err(LibraryRepositoryError::InvalidData)
            } else {
                Ok(BulkNullableTextEdit::Set(value.to_owned()))
            }
        }
        other => Ok(other.clone()),
    }
}

fn ids_json(ids: &[i64]) -> Result<String, LibraryRepositoryError> {
    serde_json::to_string(ids).map_err(|_| LibraryRepositoryError::InvalidData)
}

fn record_confirmed_edit(
    connection: &Connection,
    target_type: &str,
    target_id: i64,
    action: &str,
    changed_fields: &[&str],
    now: i64,
) -> Result<(), LibraryRepositoryError> {
    let fields =
        serde_json::to_string(changed_fields).map_err(|_| LibraryRepositoryError::InvalidData)?;
    connection.execute(
        "INSERT INTO edit_history(target_type,target_id,action,changed_fields_json,source,status,created_at,updated_at)
         VALUES(?1,?2,?3,?4,'manual','confirmed',?5,?5)",
        params![target_type, target_id, action, fields, now],
    ).map_err(map_db_error)?;
    Ok(())
}

fn same_ids(left: &[i64], right: &[i64]) -> bool {
    left.len() == right.len()
        && left.iter().copied().collect::<HashSet<_>>()
            == right.iter().copied().collect::<HashSet<_>>()
}

fn normalized_optional_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn validate_bulk_asset_edit(
    connection: &Connection,
    edit: &NormalizedBulkAssetEdit,
) -> Result<(), LibraryRepositoryError> {
    let target_count: i64 = connection
        .query_row(
            "SELECT count(*) FROM assets WHERE id IN (SELECT value FROM json_each(?1))",
            [&edit.asset_ids_json],
            |row| row.get(0),
        )
        .map_err(map_db_error)?;
    if target_count != edit.asset_ids.len() as i64 {
        // 回收站中的作品已从 assets 移除，因此也会在这里被整批拒绝。
        return Err(LibraryRepositoryError::NotFound);
    }

    validate_bulk_relation_targets(
        connection,
        "categories",
        &edit.add_category_ids_json,
        &edit.remove_category_ids_json,
        edit.add_category_count + edit.remove_category_count,
    )?;
    validate_bulk_relation_targets(
        connection,
        "tags",
        &edit.add_tag_ids_json,
        &edit.remove_tag_ids_json,
        edit.add_tag_count + edit.remove_tag_count,
    )?;
    let disabled_addition: bool = connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM categories
                 WHERE id IN (SELECT value FROM json_each(?1)) AND is_enabled=0
             )",
            [&edit.add_category_ids_json],
            |row| row.get(0),
        )
        .map_err(map_db_error)?;
    if disabled_addition {
        return Err(LibraryRepositoryError::Conflict);
    }

    let violates_single_category: bool = connection
        .query_row(
            "WITH target_assets(asset_id) AS (
                 SELECT CAST(value AS INTEGER) FROM json_each(?1)
             ), resulting(asset_id,category_id) AS (
                 SELECT ac.asset_id,ac.category_id
                   FROM asset_categories ac JOIN target_assets t ON t.asset_id=ac.asset_id
                  WHERE ac.category_id NOT IN (SELECT value FROM json_each(?2))
                 UNION
                 SELECT t.asset_id,CAST(c.value AS INTEGER)
                   FROM target_assets t CROSS JOIN json_each(?3) c
             )
             SELECT EXISTS(
                 SELECT 1 FROM resulting r
                 JOIN categories c ON c.id=r.category_id
                 JOIN dimensions d ON d.id=c.dimension_id
                 WHERE d.allows_multiple=0
                 GROUP BY r.asset_id,c.dimension_id HAVING count(*)>1
             )",
            params![
                edit.asset_ids_json,
                edit.remove_category_ids_json,
                edit.add_category_ids_json,
            ],
            |row| row.get(0),
        )
        .map_err(map_db_error)?;
    if violates_single_category {
        return Err(LibraryRepositoryError::Conflict);
    }
    Ok(())
}

fn validate_bulk_relation_targets(
    connection: &Connection,
    table: &str,
    additions_json: &str,
    removals_json: &str,
    expected: usize,
) -> Result<(), LibraryRepositoryError> {
    let sql = match table {
        "categories" => {
            "SELECT count(*) FROM categories WHERE id IN (
                SELECT value FROM json_each(?1) UNION SELECT value FROM json_each(?2)
             )"
        }
        "tags" => {
            "SELECT count(*) FROM tags WHERE id IN (
                SELECT value FROM json_each(?1) UNION SELECT value FROM json_each(?2)
             )"
        }
        _ => return Err(LibraryRepositoryError::InvalidData),
    };
    let found: i64 = connection
        .query_row(sql, params![additions_json, removals_json], |row| {
            row.get(0)
        })
        .map_err(map_db_error)?;
    if found != expected as i64 {
        Err(LibraryRepositoryError::NotFound)
    } else {
        Ok(())
    }
}

fn bulk_asset_edit_preview(
    connection: &Connection,
    edit: &NormalizedBulkAssetEdit,
) -> Result<BulkAssetEditPreview, LibraryRepositoryError> {
    let (category_add, category_remove, tag_add, tag_remove): (i64, i64, i64, i64) = connection
        .query_row(
            "WITH target_assets(asset_id) AS (
                 SELECT CAST(value AS INTEGER) FROM json_each(?1)
             )
             SELECT
               (SELECT count(*) FROM target_assets t CROSS JOIN json_each(?2) c
                 WHERE NOT EXISTS(SELECT 1 FROM asset_categories ac
                                   WHERE ac.asset_id=t.asset_id AND ac.category_id=c.value)),
               (SELECT count(*) FROM asset_categories ac JOIN target_assets t
                    ON t.asset_id=ac.asset_id
                 WHERE ac.category_id IN (SELECT value FROM json_each(?3))),
               (SELECT count(*) FROM target_assets t CROSS JOIN json_each(?4) tag
                 WHERE NOT EXISTS(SELECT 1 FROM asset_tags at
                                   WHERE at.asset_id=t.asset_id AND at.tag_id=tag.value)),
               (SELECT count(*) FROM asset_tags at JOIN target_assets t
                    ON t.asset_id=at.asset_id
                 WHERE at.tag_id IN (SELECT value FROM json_each(?5)))",
            params![
                edit.asset_ids_json,
                edit.add_category_ids_json,
                edit.remove_category_ids_json,
                edit.add_tag_ids_json,
                edit.remove_tag_ids_json,
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(map_db_error)?;
    Ok(BulkAssetEditPreview {
        target_count: u32::try_from(edit.asset_ids.len())
            .map_err(|_| LibraryRepositoryError::InvalidData)?,
        category_relations_to_add: relation_count(category_add)?,
        category_relations_to_remove: relation_count(category_remove)?,
        tag_relations_to_add: relation_count(tag_add)?,
        tag_relations_to_remove: relation_count(tag_remove)?,
    })
}

fn relation_count(value: i64) -> Result<u32, LibraryRepositoryError> {
    u32::try_from(value).map_err(|_| LibraryRepositoryError::InvalidData)
}

fn resolve_bulk_named(
    transaction: &Transaction<'_>,
    table: &str,
    edit: &BulkNullableTextEdit,
    now: i64,
) -> Result<(bool, Option<i64>), LibraryRepositoryError> {
    match edit {
        BulkNullableTextEdit::Keep => Ok((false, None)),
        BulkNullableTextEdit::Clear => Ok((true, None)),
        BulkNullableTextEdit::Set(value) => Ok((
            true,
            get_or_create_named(transaction, table, Some(value), now)?,
        )),
    }
}

fn apply_bulk_relation_changes(
    transaction: &Transaction<'_>,
    table: &str,
    target_column: &str,
    asset_ids_json: &str,
    additions_json: &str,
    removals_json: &str,
    now: i64,
) -> Result<(), LibraryRepositoryError> {
    let (delete_sql, insert_sql) = match (table, target_column) {
        ("asset_categories", "category_id") => (
            "DELETE FROM asset_categories
              WHERE asset_id IN (SELECT value FROM json_each(?1))
                AND category_id IN (SELECT value FROM json_each(?2))",
            "INSERT INTO asset_categories(asset_id,category_id,created_at)
             SELECT CAST(a.value AS INTEGER),CAST(c.value AS INTEGER),?3
               FROM json_each(?1) a CROSS JOIN json_each(?2) c
              WHERE NOT EXISTS(
                    SELECT 1 FROM asset_categories existing
                     WHERE existing.asset_id=a.value AND existing.category_id=c.value
              )",
        ),
        ("asset_tags", "tag_id") => (
            "DELETE FROM asset_tags
              WHERE asset_id IN (SELECT value FROM json_each(?1))
                AND tag_id IN (SELECT value FROM json_each(?2))",
            "INSERT INTO asset_tags(asset_id,tag_id,created_at)
             SELECT CAST(a.value AS INTEGER),CAST(t.value AS INTEGER),?3
               FROM json_each(?1) a CROSS JOIN json_each(?2) t
              WHERE NOT EXISTS(
                    SELECT 1 FROM asset_tags existing
                     WHERE existing.asset_id=a.value AND existing.tag_id=t.value
              )",
        ),
        _ => return Err(LibraryRepositoryError::InvalidData),
    };
    transaction
        .execute(delete_sql, params![asset_ids_json, removals_json])
        .map_err(map_db_error)?;
    transaction
        .execute(insert_sql, params![asset_ids_json, additions_json, now])
        .map_err(map_db_error)?;
    Ok(())
}

fn project_from_row(row: &Row<'_>) -> rusqlite::Result<ProjectSummary> {
    let kind: String = row.get(1)?;
    let kind = ProjectKind::from_db_str(&kind).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid project kind",
            )),
        )
    })?;
    Ok(ProjectSummary {
        id: row.get(0)?,
        kind,
        title: row.get(2)?,
        description: row.get(3)?,
        prompt: PromptText {
            prompt_zh: row.get(4)?,
            prompt_en: row.get(5)?,
            negative_prompt: row.get(6)?,
        },
        rating: row.get(7)?,
        is_favorite: row.get(8)?,
        is_public: row.get(9)?,
        notes: row.get(10)?,
        asset_count: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn canvas_member_from_row(row: &Row<'_>) -> rusqlite::Result<CanvasProjectMember> {
    let media_type: String = row.get(3)?;
    let role: String = row.get(10)?;
    let media_type = match media_type.as_str() {
        "image" => MediaType::Image,
        "video" => MediaType::Video,
        _ => {
            return Err(rusqlite::Error::InvalidColumnType(
                3,
                "media_type".into(),
                rusqlite::types::Type::Text,
            ));
        }
    };
    let role = CanvasMemberRole::from_db_str(&role).ok_or_else(|| {
        rusqlite::Error::InvalidColumnType(10, "role".into(), rusqlite::types::Type::Text)
    })?;
    Ok(CanvasProjectMember {
        asset_id: row.get(0)?,
        display_order: row.get(1)?,
        file_name: row.get(2)?,
        media_type,
        model_name: row.get(4)?,
        platform_name: row.get(5)?,
        width: row.get(6)?,
        height: row.get(7)?,
        duration_ms: row.get(8)?,
        updated_at: row.get(9)?,
        role,
        reference_name: row.get(11)?,
        prompt_zh: row.get(12)?,
        prompt_en: row.get(13)?,
        negative_prompt: row.get(14)?,
    })
}

fn canvas_member(
    connection: &Connection,
    project_id: i64,
    asset_id: i64,
) -> Result<CanvasProjectMember, LibraryRepositoryError> {
    connection
        .query_row(
            "SELECT a.id, orders.position, a.file_name, a.media_type,
                    model.name, platform.name, a.width, a.height, a.duration_ms,
                    a.updated_at, member.role, member.reference_name,
                    coalesce(prompt.prompt_zh,''), coalesce(prompt.prompt_en,''),
                    coalesce(prompt.negative_prompt,'')
               FROM canvas_project_members member
               JOIN assets a ON a.id=member.asset_id AND a.project_id=member.project_id
               JOIN asset_display_order orders ON orders.asset_id=a.id
               LEFT JOIN models model ON model.id=a.model_id
               LEFT JOIN platforms platform ON platform.id=a.platform_id
               LEFT JOIN prompts prompt ON prompt.id=a.prompt_id
              WHERE member.project_id=?1 AND member.asset_id=?2",
            params![project_id, asset_id],
            canvas_member_from_row,
        )
        .optional()
        .map_err(map_db_error)?
        .ok_or(LibraryRepositoryError::NotFound)
}

fn ensure_canvas_project(
    connection: &Connection,
    project_id: i64,
) -> Result<(), LibraryRepositoryError> {
    let kind: Option<String> = connection
        .query_row(
            "SELECT kind FROM projects WHERE id=?1",
            [project_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_db_error)?;
    match kind.as_deref() {
        Some("canvas") => Ok(()),
        Some(_) => Err(LibraryRepositoryError::Conflict),
        None => Err(LibraryRepositoryError::NotFound),
    }
}

fn ensure_canvas_outputs_for_project(
    transaction: &Transaction<'_>,
    project_id: i64,
    now: i64,
) -> Result<(), LibraryRepositoryError> {
    let is_canvas: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1 AND kind='canvas')",
            [project_id],
            |row| row.get(0),
        )
        .map_err(map_db_error)?;
    if !is_canvas {
        return Ok(());
    }
    transaction
        .execute(
            "INSERT INTO canvas_project_members
             (project_id,asset_id,role,reference_name,position,created_at,updated_at)
             SELECT ?1,a.id,'output',NULL,orders.position,?2,?2
               FROM assets a
               JOIN asset_display_order orders ON orders.asset_id=a.id
              WHERE a.project_id=?1
                AND NOT EXISTS(
                    SELECT 1 FROM canvas_project_members member WHERE member.asset_id=a.id
                )",
            params![project_id, now],
        )
        .map_err(map_db_error)?;
    Ok(())
}

fn ensure_canvas_output_for_asset(
    transaction: &Transaction<'_>,
    project_id: i64,
    asset_id: i64,
    now: i64,
) -> Result<(), LibraryRepositoryError> {
    transaction
        .execute(
            "INSERT INTO canvas_project_members
             (project_id,asset_id,role,reference_name,position,created_at,updated_at)
             SELECT ?1,a.id,'output',NULL,orders.position,?3,?3
               FROM assets a
               JOIN projects project ON project.id=a.project_id AND project.kind='canvas'
               JOIN asset_display_order orders ON orders.asset_id=a.id
              WHERE a.project_id=?1 AND a.id=?2
             ON CONFLICT(project_id,asset_id) DO NOTHING",
            params![project_id, asset_id, now],
        )
        .map_err(map_db_error)?;
    Ok(())
}

fn normalize_reference_name(value: &str) -> Result<String, LibraryRepositoryError> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > 50
        || value.chars().any(is_reference_name_delimiter)
    {
        return Err(LibraryRepositoryError::InvalidData);
    }
    Ok(value.to_owned())
}

fn is_reference_name_delimiter(character: char) -> bool {
    character.is_whitespace()
        || character == '@'
        || matches!(
            character,
            ',' | '.'
                | ';'
                | ':'
                | '!'
                | '?'
                | '，'
                | '。'
                | '；'
                | '：'
                | '！'
                | '？'
                | '、'
                | '/'
                | '\\'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '<'
                | '>'
                | '《'
                | '》'
                | '"'
                | '\''
                | '`'
                | '~'
                | '#'
                | '$'
                | '%'
                | '^'
                | '&'
                | '*'
                | '+'
                | '='
                | '|'
                | '-'
                | '_'
        )
}

fn validate_canvas_prompt_references(
    connection: &Connection,
    project_id: i64,
    prompt: &PromptText,
) -> Result<(), LibraryRepositoryError> {
    let mut statement = connection
        .prepare(
            "SELECT reference_name FROM canvas_project_members
              WHERE project_id=?1 AND role='reference'",
        )
        .map_err(map_db_error)?;
    let references = statement
        .query_map([project_id], |row| row.get::<_, String>(0))
        .map_err(map_db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_db_error)?
        .into_iter()
        .map(|value| value.to_lowercase())
        .collect::<HashSet<_>>();
    for text in [
        &prompt.prompt_zh,
        &prompt.prompt_en,
        &prompt.negative_prompt,
    ] {
        let characters = text.chars().collect::<Vec<_>>();
        let mut index = 0;
        while index < characters.len() {
            if characters[index] != '@' {
                index += 1;
                continue;
            }
            index += 1;
            let start = index;
            while index < characters.len() && !is_reference_name_delimiter(characters[index]) {
                index += 1;
            }
            if start == index {
                return Err(LibraryRepositoryError::InvalidData);
            }
            let token = characters[start..index]
                .iter()
                .collect::<String>()
                .to_lowercase();
            if !references.contains(&token) {
                return Err(LibraryRepositoryError::InvalidData);
            }
        }
    }
    Ok(())
}

fn ensure_canvas_output_invariant_after_removing_assets(
    connection: &Connection,
    asset_ids: &[i64],
    ignored_project_id: Option<i64>,
) -> Result<(), LibraryRepositoryError> {
    if asset_ids.is_empty() {
        return Ok(());
    }
    let asset_ids_json = ids_json(asset_ids)?;
    let invalid: bool = connection
        .query_row(
            "WITH selected(asset_id) AS (SELECT value FROM json_each(?1)),
                  affected(project_id) AS (
                    SELECT DISTINCT member.project_id
                      FROM canvas_project_members member
                      JOIN selected ON selected.asset_id=member.asset_id
                     WHERE (?2 IS NULL OR member.project_id<>?2)
                  )
             SELECT EXISTS(
                SELECT 1 FROM affected
                 WHERE EXISTS(
                    SELECT 1 FROM canvas_project_members remaining
                     WHERE remaining.project_id=affected.project_id
                       AND NOT EXISTS(
                            SELECT 1 FROM selected WHERE selected.asset_id=remaining.asset_id
                       )
                 )
                   AND NOT EXISTS(
                    SELECT 1 FROM canvas_project_members output
                     WHERE output.project_id=affected.project_id AND output.role='output'
                       AND NOT EXISTS(
                            SELECT 1 FROM selected WHERE selected.asset_id=output.asset_id
                       )
                 )
             )",
            params![asset_ids_json, ignored_project_id],
            |row| row.get(0),
        )
        .map_err(map_db_error)?;
    if invalid {
        return Err(LibraryRepositoryError::Conflict);
    }
    Ok(())
}

fn ensure_canvas_reference_change_safe(
    connection: &Connection,
    project_id: i64,
    asset_id: i64,
    next_role: CanvasMemberRole,
    next_reference_name: Option<&str>,
) -> Result<(), LibraryRepositoryError> {
    let current_reference_name: Option<String> = connection
        .query_row(
            "SELECT reference_name FROM canvas_project_members
              WHERE project_id=?1 AND asset_id=?2 AND role='reference'",
            params![project_id, asset_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_db_error)?;
    let Some(current_reference_name) = current_reference_name else {
        return Ok(());
    };
    let remains_same_reference = next_role == CanvasMemberRole::Reference
        && next_reference_name
            .is_some_and(|name| name.to_lowercase() == current_reference_name.to_lowercase());
    if remains_same_reference {
        return Ok(());
    }

    let mut statement = connection
        .prepare(
            "SELECT coalesce(prompt.prompt_zh,''),coalesce(prompt.prompt_en,''),
                    coalesce(prompt.negative_prompt,'')
               FROM canvas_project_members member
               JOIN assets asset ON asset.id=member.asset_id
               LEFT JOIN prompts prompt ON prompt.id=asset.prompt_id
              WHERE member.project_id=?1 AND member.role='output'",
        )
        .map_err(map_db_error)?;
    let output_prompts = statement
        .query_map([project_id], |row| {
            Ok(PromptText {
                prompt_zh: row.get(0)?,
                prompt_en: row.get(1)?,
                negative_prompt: row.get(2)?,
            })
        })
        .map_err(map_db_error)?;
    for prompt in output_prompts {
        let prompt = prompt.map_err(map_db_error)?;
        if [
            &prompt.prompt_zh,
            &prompt.prompt_en,
            &prompt.negative_prompt,
        ]
        .into_iter()
        .any(|text| prompt_uses_reference_name(text, &current_reference_name))
        {
            return Err(LibraryRepositoryError::Conflict);
        }
    }
    Ok(())
}

fn ensure_canvas_references_safe_after_removing_assets(
    connection: &Connection,
    asset_ids: &[i64],
    ignored_project_id: Option<i64>,
) -> Result<(), LibraryRepositoryError> {
    if asset_ids.is_empty() {
        return Ok(());
    }
    let asset_ids_json = ids_json(asset_ids)?;
    let mut references = connection
        .prepare(
            "SELECT member.project_id,member.reference_name
               FROM canvas_project_members member
              WHERE member.role='reference'
                AND member.asset_id IN (SELECT value FROM json_each(?1))
                AND (?2 IS NULL OR member.project_id<>?2)",
        )
        .map_err(map_db_error)?;
    let references = references
        .query_map(params![asset_ids_json, ignored_project_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(map_db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_db_error)?;
    let mut references_by_project = HashMap::<i64, HashSet<String>>::new();
    for (project_id, reference_name) in references {
        references_by_project
            .entry(project_id)
            .or_default()
            .insert(reference_name.to_lowercase());
    }
    for (project_id, reference_names) in references_by_project {
        let mut outputs = connection
            .prepare(
                "SELECT coalesce(prompt.prompt_zh,''),coalesce(prompt.prompt_en,''),
                        coalesce(prompt.negative_prompt,'')
                   FROM canvas_project_members member
                   JOIN assets asset ON asset.id=member.asset_id
                   LEFT JOIN prompts prompt ON prompt.id=asset.prompt_id
                  WHERE member.project_id=?1 AND member.role='output'
                    AND member.asset_id NOT IN (SELECT value FROM json_each(?2))",
            )
            .map_err(map_db_error)?;
        let output_prompts = outputs
            .query_map(params![project_id, asset_ids_json], |row| {
                Ok(PromptText {
                    prompt_zh: row.get(0)?,
                    prompt_en: row.get(1)?,
                    negative_prompt: row.get(2)?,
                })
            })
            .map_err(map_db_error)?;
        for prompt in output_prompts {
            let prompt = prompt.map_err(map_db_error)?;
            if [
                &prompt.prompt_zh,
                &prompt.prompt_en,
                &prompt.negative_prompt,
            ]
            .into_iter()
            .flat_map(|text| prompt_reference_tokens(text).into_iter())
            .any(|token| reference_names.contains(&token))
            {
                return Err(LibraryRepositoryError::Conflict);
            }
        }
    }
    Ok(())
}

fn prompt_uses_reference_name(text: &str, reference_name: &str) -> bool {
    prompt_reference_tokens(text).contains(&reference_name.to_lowercase())
}

fn prompt_reference_tokens(text: &str) -> HashSet<String> {
    let characters = text.chars().collect::<Vec<_>>();
    let mut tokens = HashSet::new();
    let mut index = 0;
    while index < characters.len() {
        if characters[index] != '@' {
            index += 1;
            continue;
        }
        index += 1;
        let start = index;
        while index < characters.len() && !is_reference_name_delimiter(characters[index]) {
            index += 1;
        }
        if start < index {
            tokens.insert(
                characters[start..index]
                    .iter()
                    .collect::<String>()
                    .to_lowercase(),
            );
        }
    }
    tokens
}
fn asset_from_row(row: &Row<'_>) -> rusqlite::Result<AssetSummary> {
    let media_type: String = row.get(2)?;
    let path_kind: String = row.get(3)?;
    let json: String = row.get(19)?;
    let generation_params = serde_json::from_str(&json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(19, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(AssetSummary {
        id: row.get(0)?,
        media: AssetMetadata {
            project_id: row.get(1)?,
            media_type: if media_type == "image" {
                MediaType::Image
            } else {
                MediaType::Video
            },
            path_kind: if path_kind == "managed" {
                PathKind::Managed
            } else {
                PathKind::External
            },
            stored_path: row.get(4)?,
            file_name: row.get(5)?,
            mime_type: row.get(6)?,
            file_size: row.get(7)?,
            content_hash: row.get(8)?,
            width: row.get(9)?,
            height: row.get(10)?,
            duration_ms: row.get(11)?,
            frame_rate: row.get(12)?,
            has_audio: row.get(13)?,
        },
        prompt: PromptText {
            prompt_zh: row.get(14)?,
            prompt_en: row.get(15)?,
            negative_prompt: row.get(16)?,
        },
        model: row.get(17)?,
        platform: row.get(18)?,
        generation_params,
        rating: row.get(20)?,
        is_favorite: row.get(21)?,
        is_public: row.get(22)?,
        notes: row.get(23)?,
        created_at: row.get(24)?,
        updated_at: row.get(25)?,
        display_order: row.get(26)?,
    })
}
fn build_page<T>(items: &mut Vec<T>, limit: u32, cursor: impl Fn(&T) -> PageCursor) -> Page<T> {
    let has_more = items.len() > limit as usize;
    if has_more {
        items.pop();
    }
    let next_cursor = if has_more {
        items.last().map(cursor)
    } else {
        None
    };
    Page {
        items: std::mem::take(items),
        next_cursor,
    }
}
fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}
fn ensure_changed(changed: usize) -> Result<(), LibraryRepositoryError> {
    if changed == 0 {
        Err(LibraryRepositoryError::NotFound)
    } else {
        Ok(())
    }
}
fn map_db_error(error: rusqlite::Error) -> LibraryRepositoryError {
    match error {
        rusqlite::Error::SqliteFailure(code, _)
            if code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
                || code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY =>
        {
            LibraryRepositoryError::Conflict
        }
        rusqlite::Error::SqliteFailure(code, _)
            if code.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            LibraryRepositoryError::Conflict
        }
        rusqlite::Error::FromSqlConversionFailure(..)
        | rusqlite::Error::IntegralValueOutOfRange(..) => LibraryRepositoryError::InvalidData,
        _ => LibraryRepositoryError::DatabaseFailed,
    }
}
fn parse_json<T: DeserializeOwned>(value: &str) -> Result<T, LibraryRepositoryError> {
    serde_json::from_str(value).map_err(|_| LibraryRepositoryError::InvalidData)
}
fn insert_project_prompt(
    tx: &Transaction<'_>,
    project_id: i64,
    p: &PromptText,
    now: i64,
) -> Result<i64, LibraryRepositoryError> {
    tx.execute("INSERT INTO prompts(project_id,prompt_zh,prompt_en,negative_prompt,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?5)",params![project_id,p.prompt_zh,p.prompt_en,p.negative_prompt,now]).map_err(map_db_error)?;
    Ok(tx.last_insert_rowid())
}
fn upsert_project_prompt(
    tx: &Transaction<'_>,
    project_id: i64,
    p: &PromptText,
    now: i64,
) -> Result<(), LibraryRepositoryError> {
    let id:Option<i64>=tx.query_row("SELECT p.id FROM prompts p WHERE p.project_id=?1 AND NOT EXISTS(SELECT 1 FROM assets a WHERE a.prompt_id=p.id) ORDER BY p.id LIMIT 1",[project_id],|r|r.get(0)).optional().map_err(map_db_error)?;
    if let Some(id) = id {
        tx.execute("UPDATE prompts SET prompt_zh=?1,prompt_en=?2,negative_prompt=?3,updated_at=max(updated_at,?4) WHERE id=?5",params![p.prompt_zh,p.prompt_en,p.negative_prompt,now,id]).map_err(map_db_error)?;
    } else {
        insert_project_prompt(tx, project_id, p, now)?;
    }
    Ok(())
}
fn insert_asset(
    transaction: &Transaction<'_>,
    input: &CreateAsset,
    now: i64,
) -> Result<i64, LibraryRepositoryError> {
    let prompt_id = insert_asset_prompt(transaction, &input.prompt, now)?;
    let model_id = get_or_create_named(transaction, "models", input.model.as_deref(), now)?;
    let platform_id =
        get_or_create_named(transaction, "platforms", input.platform.as_deref(), now)?;
    let params_json = serde_json::to_string(&input.generation_params)
        .map_err(|_| LibraryRepositoryError::InvalidData)?;
    transaction
        .execute(
            "INSERT INTO assets (project_id, prompt_id, model_id, platform_id, media_type, path_kind,
             stored_path, file_name, mime_type, file_size, content_hash, width, height, duration_ms,
             frame_rate, has_audio, generation_params_json, rating, is_favorite, is_public, notes,
             created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,
             ?15,?16,?17,?18,?19,?20,?21,?22,?22)",
            params![
                input.media.project_id,
                prompt_id,
                model_id,
                platform_id,
                input.media.media_type.as_db_str(),
                input.media.path_kind.as_db_str(),
                input.media.stored_path,
                input.media.file_name,
                input.media.mime_type,
                input.media.file_size,
                input.media.content_hash,
                input.media.width,
                input.media.height,
                input.media.duration_ms,
                input.media.frame_rate,
                input.media.has_audio,
                params_json,
                input.rating,
                input.is_favorite,
                input.is_public,
                input.notes,
                now
            ],
        )
        .map_err(map_db_error)?;
    let id = transaction.last_insert_rowid();
    replace_categories(
        transaction,
        "asset_categories",
        "asset_id",
        id,
        &input.category_ids,
        now,
    )?;
    replace_tags(
        transaction,
        "asset_tags",
        "asset_id",
        id,
        &input.tag_ids,
        now,
    )?;
    if let Some(project_id) = input.media.project_id {
        ensure_canvas_output_for_asset(transaction, project_id, id, now)?;
    }
    Ok(id)
}

fn insert_asset_prompt(
    tx: &Transaction<'_>,
    p: &PromptText,
    now: i64,
) -> Result<i64, LibraryRepositoryError> {
    tx.execute("INSERT INTO prompts(prompt_zh,prompt_en,negative_prompt,created_at,updated_at) VALUES(?1,?2,?3,?4,?4)",params![p.prompt_zh,p.prompt_en,p.negative_prompt,now]).map_err(map_db_error)?;
    Ok(tx.last_insert_rowid())
}
fn get_or_create_named(
    tx: &Transaction<'_>,
    table: &str,
    name: Option<&str>,
    now: i64,
) -> Result<Option<i64>, LibraryRepositoryError> {
    let Some(name) = name.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let insert = match table {
        "models" => "INSERT OR IGNORE INTO models(name,created_at,updated_at) VALUES(?1,?2,?2)",
        "platforms" => {
            "INSERT OR IGNORE INTO platforms(name,created_at,updated_at) VALUES(?1,?2,?2)"
        }
        _ => return Err(LibraryRepositoryError::InvalidData),
    };
    tx.execute(insert, params![name, now])
        .map_err(map_db_error)?;
    let select = if table == "models" {
        "SELECT id FROM models WHERE name=?1"
    } else {
        "SELECT id FROM platforms WHERE name=?1"
    };
    tx.query_row(select, [name], |r| r.get(0))
        .map(Some)
        .map_err(map_db_error)
}
fn relation_ids(
    connection: &Connection,
    table: &str,
    owner: &str,
    id: i64,
) -> Result<Vec<i64>, LibraryRepositoryError> {
    let target = if table.ends_with("categories") {
        "category_id"
    } else {
        "tag_id"
    };
    let sql = format!("SELECT {target} FROM {table} WHERE {owner}=?1 ORDER BY {target}");
    let mut statement = connection.prepare(&sql).map_err(map_db_error)?;
    statement
        .query_map([id], |r| r.get(0))
        .map_err(map_db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_db_error)
}
fn replace_categories(
    tx: &Transaction<'_>,
    table: &str,
    owner: &str,
    id: i64,
    ids: &[i64],
    now: i64,
) -> Result<(), LibraryRepositoryError> {
    validate_category_set(tx, ids)?;
    let delete = format!("DELETE FROM {table} WHERE {owner}=?1");
    tx.execute(&delete, [id]).map_err(map_db_error)?;
    let insert = format!("INSERT INTO {table}({owner},category_id,created_at) VALUES(?1,?2,?3)");
    let mut statement = tx.prepare(&insert).map_err(map_db_error)?;
    for category_id in ids {
        statement
            .execute(params![id, category_id, now])
            .map_err(map_db_error)?;
    }
    Ok(())
}
fn replace_tags(
    tx: &Transaction<'_>,
    table: &str,
    owner: &str,
    id: i64,
    ids: &[i64],
    now: i64,
) -> Result<(), LibraryRepositoryError> {
    let unique = ids.iter().copied().collect::<HashSet<_>>();
    if unique.len() != ids.len() {
        return Err(LibraryRepositoryError::InvalidData);
    }
    let ids_json = serde_json::to_string(ids).map_err(|_| LibraryRepositoryError::InvalidData)?;
    let found: i64 = tx
        .query_row(
            "SELECT count(*) FROM tags WHERE id IN (SELECT value FROM json_each(?1))",
            [ids_json],
            |row| row.get(0),
        )
        .map_err(map_db_error)?;
    if found != ids.len() as i64 {
        return Err(LibraryRepositoryError::NotFound);
    }
    let delete = format!("DELETE FROM {table} WHERE {owner}=?1");
    tx.execute(&delete, [id]).map_err(map_db_error)?;
    let insert = format!("INSERT INTO {table}({owner},tag_id,created_at) VALUES(?1,?2,?3)");
    let mut statement = tx.prepare(&insert).map_err(map_db_error)?;
    for tag_id in ids {
        statement
            .execute(params![id, tag_id, now])
            .map_err(map_db_error)?;
    }
    Ok(())
}
fn set_category_relations(
    connection: &mut Connection,
    entity_table: &str,
    relation_table: &str,
    owner: &str,
    id: i64,
    ids: &[i64],
    now: i64,
) -> Result<(), LibraryRepositoryError> {
    let tx = connection.transaction().map_err(map_db_error)?;
    let sql = format!("SELECT EXISTS(SELECT 1 FROM {entity_table} WHERE id=?1)");
    let exists: bool = tx
        .query_row(&sql, [id], |r| r.get(0))
        .map_err(map_db_error)?;
    if !exists {
        return Err(LibraryRepositoryError::NotFound);
    }
    replace_categories(&tx, relation_table, owner, id, ids, now)?;
    let touch = format!("UPDATE {entity_table} SET updated_at=max(updated_at,?1) WHERE id=?2");
    tx.execute(&touch, params![now, id]).map_err(map_db_error)?;
    tx.commit().map_err(map_db_error)
}
fn set_tag_relations(
    connection: &mut Connection,
    entity_table: &str,
    relation_table: &str,
    owner: &str,
    id: i64,
    ids: &[i64],
    now: i64,
) -> Result<(), LibraryRepositoryError> {
    let tx = connection.transaction().map_err(map_db_error)?;
    let sql = format!("SELECT EXISTS(SELECT 1 FROM {entity_table} WHERE id=?1)");
    let exists: bool = tx
        .query_row(&sql, [id], |r| r.get(0))
        .map_err(map_db_error)?;
    if !exists {
        return Err(LibraryRepositoryError::NotFound);
    }
    replace_tags(&tx, relation_table, owner, id, ids, now)?;
    let touch = format!("UPDATE {entity_table} SET updated_at=max(updated_at,?1) WHERE id=?2");
    tx.execute(&touch, params![now, id]).map_err(map_db_error)?;
    tx.commit().map_err(map_db_error)
}
fn validate_category_set(tx: &Transaction<'_>, ids: &[i64]) -> Result<(), LibraryRepositoryError> {
    let unique = ids.iter().copied().collect::<HashSet<_>>();
    if unique.len() != ids.len() {
        return Err(LibraryRepositoryError::InvalidData);
    }
    let ids_json = serde_json::to_string(ids).map_err(|_| LibraryRepositoryError::InvalidData)?;
    let mut statement = tx
        .prepare(
            "SELECT c.dimension_id,c.is_enabled,d.allows_multiple
             FROM categories c JOIN dimensions d ON d.id=c.dimension_id
             WHERE c.id IN (SELECT value FROM json_each(?1))",
        )
        .map_err(map_db_error)?;
    let rows = statement
        .query_map([ids_json], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, bool>(1)?,
                row.get::<_, bool>(2)?,
            ))
        })
        .map_err(map_db_error)?;
    let mut dimensions = HashMap::<i64, (bool, u32)>::new();
    let mut found = 0_usize;
    for row in rows {
        let (dimension, enabled, allows_multiple) = row.map_err(map_db_error)?;
        found += 1;
        if !enabled {
            return Err(LibraryRepositoryError::Conflict);
        }
        let item = dimensions.entry(dimension).or_insert((allows_multiple, 0));
        item.1 += 1;
        if !item.0 && item.1 > 1 {
            return Err(LibraryRepositoryError::Conflict);
        }
    }
    if found != ids.len() {
        return Err(LibraryRepositoryError::NotFound);
    }
    Ok(())
}
fn validate_existing_relations_for_category(
    tx: &Transaction<'_>,
    category_id: i64,
    dimension_id: i64,
) -> Result<(), LibraryRepositoryError> {
    let allows: bool = tx
        .query_row(
            "SELECT allows_multiple FROM dimensions WHERE id=?1",
            [dimension_id],
            |r| r.get(0),
        )
        .map_err(map_db_error)?;
    if allows {
        return Ok(());
    }
    let conflict:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM project_categories pc JOIN project_categories other ON other.project_id=pc.project_id JOIN categories c ON c.id=other.category_id WHERE pc.category_id=?1 AND other.category_id<>?1 AND c.dimension_id=?2 UNION ALL SELECT 1 FROM asset_categories ac JOIN asset_categories other ON other.asset_id=ac.asset_id JOIN categories c ON c.id=other.category_id WHERE ac.category_id=?1 AND other.category_id<>?1 AND c.dimension_id=?2)",params![category_id,dimension_id],|r|r.get(0)).map_err(map_db_error)?;
    if conflict {
        Err(LibraryRepositoryError::Conflict)
    } else {
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct PromptSnapshot {
    id: i64,
    project_id: Option<i64>,
    prompt_zh: String,
    prompt_en: String,
    negative_prompt: String,
    created_at: i64,
    updated_at: i64,
    #[serde(default)]
    versions: Vec<PromptVersionSnapshot>,
    #[serde(default)]
    history: Vec<EditHistorySnapshot>,
}
#[derive(Serialize, Deserialize)]
struct PromptVersionSnapshot {
    id: i64,
    prompt_id: i64,
    version: i64,
    prompt_zh: String,
    prompt_en: String,
    negative_prompt: String,
    created_at: i64,
}
#[derive(Serialize, Deserialize)]
struct CustomFieldValueSnapshot {
    id: i64,
    field_id: i64,
    target_type: String,
    target_id: i64,
    value_json: String,
    source: String,
    status: String,
    created_at: i64,
    updated_at: i64,
}
#[derive(Serialize, Deserialize)]
struct EditHistorySnapshot {
    id: i64,
    target_type: String,
    target_id: i64,
    action: String,
    changed_fields_json: String,
    source: String,
    status: String,
    created_at: i64,
    updated_at: i64,
}
#[derive(Serialize, Deserialize)]
struct CanvasMemberSnapshot {
    project_id: i64,
    asset_id: i64,
    role: CanvasMemberRole,
    reference_name: Option<String>,
    position: i64,
    created_at: i64,
    updated_at: i64,
}
#[derive(Serialize, Deserialize)]
struct ProjectSnapshot {
    project: ProjectSummary,
    prompt: Option<PromptSnapshot>,
    category_ids: Vec<i64>,
    tag_ids: Vec<i64>,
    asset_ids: Vec<i64>,
    #[serde(default)]
    canvas_members: Vec<CanvasMemberSnapshot>,
    #[serde(default)]
    custom_field_values: Vec<CustomFieldValueSnapshot>,
    #[serde(default)]
    history: Vec<EditHistorySnapshot>,
}
#[derive(Serialize, Deserialize)]
struct AssetSnapshot {
    asset: AssetSummary,
    prompt: Option<PromptSnapshot>,
    model_id: Option<i64>,
    platform_id: Option<i64>,
    category_ids: Vec<i64>,
    tag_ids: Vec<i64>,
    #[serde(default)]
    canvas_member: Option<CanvasMemberSnapshot>,
    #[serde(default)]
    custom_field_values: Vec<CustomFieldValueSnapshot>,
    #[serde(default)]
    history: Vec<EditHistorySnapshot>,
    #[serde(default)]
    cover: Option<AssetCoverSnapshot>,
}
#[derive(Serialize, Deserialize)]
struct AssetCoverSnapshot {
    source_type: String,
    stored_path: String,
    frame_timestamp_ms: Option<i64>,
    created_at: i64,
    updated_at: i64,
}
fn prompt_snapshot(
    connection: &Connection,
    id: i64,
) -> Result<Option<PromptSnapshot>, LibraryRepositoryError> {
    let prompt = connection.query_row("SELECT id,project_id,prompt_zh,prompt_en,negative_prompt,created_at,updated_at FROM prompts WHERE id=?1",[id],|r|Ok(PromptSnapshot{id:r.get(0)?,project_id:r.get(1)?,prompt_zh:r.get(2)?,prompt_en:r.get(3)?,negative_prompt:r.get(4)?,created_at:r.get(5)?,updated_at:r.get(6)?,versions:Vec::new(),history:Vec::new()})).optional().map_err(map_db_error)?;
    prompt
        .map(|mut prompt| {
            let mut versions = connection.prepare("SELECT id,prompt_id,version,prompt_zh,prompt_en,negative_prompt,created_at FROM prompt_versions WHERE prompt_id=?1 ORDER BY version,id").map_err(map_db_error)?;
            prompt.versions = versions.query_map([id], |row| Ok(PromptVersionSnapshot { id: row.get(0)?, prompt_id: row.get(1)?, version: row.get(2)?, prompt_zh: row.get(3)?, prompt_en: row.get(4)?, negative_prompt: row.get(5)?, created_at: row.get(6)? })).map_err(map_db_error)?.collect::<Result<Vec<_>, _>>().map_err(map_db_error)?;
            prompt.history = edit_history_snapshot(connection, "prompt", id, false)?;
            Ok(prompt)
        })
        .transpose()
}
fn project_snapshot(
    connection: &Connection,
    id: i64,
) -> Result<ProjectSnapshot, LibraryRepositoryError> {
    let detail = LibraryRepository::get_project(connection, id)?;
    let prompt_id:Option<i64>=connection.query_row("SELECT p.id FROM prompts p WHERE p.project_id=?1 AND NOT EXISTS(SELECT 1 FROM assets a WHERE a.prompt_id=p.id) ORDER BY p.id LIMIT 1",[id],|r|r.get(0)).optional().map_err(map_db_error)?;
    let mut s = connection
        .prepare("SELECT id FROM assets WHERE project_id=?1 ORDER BY id")
        .map_err(map_db_error)?;
    let asset_ids = s
        .query_map([id], |r| r.get(0))
        .map_err(map_db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_db_error)?;
    let mut canvas_members = connection
        .prepare(
            "SELECT project_id,asset_id,role,reference_name,position,created_at,updated_at
               FROM canvas_project_members WHERE project_id=?1 ORDER BY asset_id",
        )
        .map_err(map_db_error)?;
    let canvas_members = canvas_members
        .query_map([id], canvas_member_snapshot_from_row)
        .map_err(map_db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_db_error)?;
    Ok(ProjectSnapshot {
        project: detail.summary,
        prompt: prompt_id
            .map(|v| prompt_snapshot(connection, v))
            .transpose()?
            .flatten(),
        category_ids: detail.category_ids,
        tag_ids: detail.tag_ids,
        asset_ids,
        canvas_members,
        custom_field_values: custom_field_value_snapshot(connection, "project", id)?,
        history: edit_history_snapshot(connection, "project", id, true)?,
    })
}
fn asset_snapshot(
    connection: &Connection,
    id: i64,
) -> Result<AssetSnapshot, LibraryRepositoryError> {
    let detail = LibraryRepository::get_asset(connection, id)?;
    let (prompt_id, model_id, platform_id): (Option<i64>, Option<i64>, Option<i64>) = connection
        .query_row(
            "SELECT prompt_id,model_id,platform_id FROM assets WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(map_db_error)?;
    Ok(AssetSnapshot {
        asset: detail.summary,
        prompt: prompt_id
            .map(|v| prompt_snapshot(connection, v))
            .transpose()?
            .flatten(),
        model_id,
        platform_id,
        category_ids: detail.category_ids,
        tag_ids: detail.tag_ids,
        canvas_member: connection
            .query_row(
                "SELECT project_id,asset_id,role,reference_name,position,created_at,updated_at
                   FROM canvas_project_members WHERE asset_id=?1",
                [id],
                canvas_member_snapshot_from_row,
            )
            .optional()
            .map_err(map_db_error)?,
        custom_field_values: custom_field_value_snapshot(connection, "asset", id)?,
        history: edit_history_snapshot(connection, "asset", id, true)?,
        cover: connection
            .query_row(
                "SELECT source_type,stored_path,frame_timestamp_ms,created_at,updated_at
                 FROM asset_covers WHERE asset_id=?1",
                [id],
                |row| {
                    Ok(AssetCoverSnapshot {
                        source_type: row.get(0)?,
                        stored_path: row.get(1)?,
                        frame_timestamp_ms: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(map_db_error)?,
    })
}

fn canvas_member_snapshot_from_row(row: &Row<'_>) -> rusqlite::Result<CanvasMemberSnapshot> {
    let role: String = row.get(2)?;
    let role = CanvasMemberRole::from_db_str(&role).ok_or_else(|| {
        rusqlite::Error::InvalidColumnType(2, "role".into(), rusqlite::types::Type::Text)
    })?;
    Ok(CanvasMemberSnapshot {
        project_id: row.get(0)?,
        asset_id: row.get(1)?,
        role,
        reference_name: row.get(3)?,
        position: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn custom_field_value_snapshot(
    connection: &Connection,
    target_type: &str,
    target_id: i64,
) -> Result<Vec<CustomFieldValueSnapshot>, LibraryRepositoryError> {
    let mut statement = connection.prepare(
        "SELECT id,field_id,target_type,target_id,value_json,source,status,created_at,updated_at
         FROM custom_field_values WHERE target_type=?1 AND target_id=?2 ORDER BY id",
    ).map_err(map_db_error)?;
    statement
        .query_map(params![target_type, target_id], |row| {
            Ok(CustomFieldValueSnapshot {
                id: row.get(0)?,
                field_id: row.get(1)?,
                target_type: row.get(2)?,
                target_id: row.get(3)?,
                value_json: row.get(4)?,
                source: row.get(5)?,
                status: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })
        .map_err(map_db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_db_error)
}

fn edit_history_snapshot(
    connection: &Connection,
    target_type: &str,
    target_id: i64,
    include_custom_field_values: bool,
) -> Result<Vec<EditHistorySnapshot>, LibraryRepositoryError> {
    let sql = if include_custom_field_values {
        "SELECT id,target_type,target_id,action,changed_fields_json,source,status,created_at,updated_at
         FROM edit_history WHERE (target_type=?1 AND target_id=?2) OR (
           target_type='custom_field_value' AND target_id IN (
             SELECT id FROM custom_field_values WHERE target_type=?1 AND target_id=?2
           )
         ) ORDER BY id"
    } else {
        "SELECT id,target_type,target_id,action,changed_fields_json,source,status,created_at,updated_at
         FROM edit_history WHERE target_type=?1 AND target_id=?2 ORDER BY id"
    };
    let mut statement = connection.prepare(sql).map_err(map_db_error)?;
    statement
        .query_map(params![target_type, target_id], |row| {
            Ok(EditHistorySnapshot {
                id: row.get(0)?,
                target_type: row.get(1)?,
                target_id: row.get(2)?,
                action: row.get(3)?,
                changed_fields_json: row.get(4)?,
                source: row.get(5)?,
                status: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })
        .map_err(map_db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_db_error)
}

fn remove_p1_entity_state(
    tx: &Transaction<'_>,
    target_type: &str,
    target_id: i64,
) -> Result<(), LibraryRepositoryError> {
    tx.execute(
        "DELETE FROM edit_history WHERE target_type='custom_field_value' AND target_id IN (
           SELECT id FROM custom_field_values WHERE target_type=?1 AND target_id=?2
         )",
        params![target_type, target_id],
    )
    .map_err(map_db_error)?;
    tx.execute(
        "DELETE FROM edit_history WHERE target_type=?1 AND target_id=?2",
        params![target_type, target_id],
    )
    .map_err(map_db_error)?;
    tx.execute(
        "DELETE FROM custom_field_values WHERE target_type=?1 AND target_id=?2",
        params![target_type, target_id],
    )
    .map_err(map_db_error)?;
    Ok(())
}

fn restore_custom_field_values(
    tx: &Transaction<'_>,
    values: &[CustomFieldValueSnapshot],
    target_id: i64,
) -> Result<HashMap<i64, i64>, LibraryRepositoryError> {
    let mut id_map = HashMap::new();
    for value in values {
        tx.execute(
            "INSERT INTO custom_field_values(field_id,target_type,target_id,value_json,source,status,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![value.field_id,value.target_type,target_id,value.value_json,value.source,value.status,value.created_at,value.updated_at],
        ).map_err(map_db_error)?;
        id_map.insert(value.id, tx.last_insert_rowid());
    }
    Ok(id_map)
}

fn restore_edit_history(
    tx: &Transaction<'_>,
    history: &[EditHistorySnapshot],
    target_type: &str,
    old_target_id: i64,
    new_target_id: i64,
    custom_field_value_ids: &HashMap<i64, i64>,
) -> Result<(), LibraryRepositoryError> {
    for entry in history {
        let target_id = if entry.target_type == target_type && entry.target_id == old_target_id {
            new_target_id
        } else if entry.target_type == "custom_field_value" {
            custom_field_value_ids
                .get(&entry.target_id)
                .copied()
                .unwrap_or(entry.target_id)
        } else {
            entry.target_id
        };
        tx.execute(
            "INSERT INTO edit_history(target_type,target_id,action,changed_fields_json,source,status,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![entry.target_type,target_id,entry.action,entry.changed_fields_json,entry.source,entry.status,entry.created_at,entry.updated_at],
        ).map_err(map_db_error)?;
    }
    Ok(())
}

fn id_exists(tx: &Transaction<'_>, table: &str, id: i64) -> Result<bool, LibraryRepositoryError> {
    tx.query_row(
        &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id=?1)"),
        [id],
        |row| row.get(0),
    )
    .map_err(map_db_error)
}

fn restore_prompt(
    tx: &Transaction<'_>,
    p: &PromptSnapshot,
    project_id: Option<i64>,
) -> Result<i64, LibraryRepositoryError> {
    // 即使快照创建时提示词由多个作品共享，也不能只凭可复用的整数 ID
    // 认定当前行仍是同一提示词。ID 已占用时恢复独立副本，优先保证内容不丢失、
    // 不静默关联到后来创建的提示词。
    let prompt_id = if id_exists(tx, "prompts", p.id)? {
        tx.execute("INSERT INTO prompts(project_id,prompt_zh,prompt_en,negative_prompt,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6)",params![project_id,p.prompt_zh,p.prompt_en,p.negative_prompt,p.created_at,p.updated_at]).map_err(map_db_error)?;
        tx.last_insert_rowid()
    } else {
        tx.execute("INSERT INTO prompts(id,project_id,prompt_zh,prompt_en,negative_prompt,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![p.id,project_id,p.prompt_zh,p.prompt_en,p.negative_prompt,p.created_at,p.updated_at]).map_err(map_db_error)?;
        p.id
    };
    for version in &p.versions {
        tx.execute("INSERT INTO prompt_versions(prompt_id,version,prompt_zh,prompt_en,negative_prompt,created_at) VALUES(?1,?2,?3,?4,?5,?6)", params![prompt_id,version.version,version.prompt_zh,version.prompt_en,version.negative_prompt,version.created_at]).map_err(map_db_error)?;
    }
    restore_edit_history(tx, &p.history, "prompt", p.id, prompt_id, &HashMap::new())?;
    Ok(prompt_id)
}
fn restore_project(
    tx: &Transaction<'_>,
    s: &ProjectSnapshot,
) -> Result<i64, LibraryRepositoryError> {
    let p = &s.project;
    let project_id = if id_exists(tx, "projects", p.id)? {
        tx.execute("INSERT INTO projects(kind,title,description,rating,is_favorite,is_public,notes,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![p.kind.as_db_str(),p.title,p.description,p.rating,p.is_favorite,p.is_public,p.notes,p.created_at,p.updated_at]).map_err(map_db_error)?;
        tx.last_insert_rowid()
    } else {
        tx.execute("INSERT INTO projects(id,kind,title,description,rating,is_favorite,is_public,notes,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![p.id,p.kind.as_db_str(),p.title,p.description,p.rating,p.is_favorite,p.is_public,p.notes,p.created_at,p.updated_at]).map_err(map_db_error)?;
        p.id
    };
    if let Some(prompt) = &s.prompt {
        restore_prompt(tx, prompt, Some(project_id))?;
    }
    replace_categories(
        tx,
        "project_categories",
        "project_id",
        project_id,
        &s.category_ids,
        p.created_at,
    )?;
    replace_tags(
        tx,
        "project_tags",
        "project_id",
        project_id,
        &s.tag_ids,
        p.created_at,
    )?;
    let ids =
        serde_json::to_string(&s.asset_ids).map_err(|_| LibraryRepositoryError::InvalidData)?;
    tx.execute("UPDATE assets SET project_id=?1 WHERE project_id IS NULL AND id IN (SELECT value FROM json_each(?2))",params![project_id,ids]).map_err(map_db_error)?;
    for member in &s.canvas_members {
        let asset_belongs_to_restored_project: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM assets WHERE id=?1 AND project_id=?2)",
                params![member.asset_id, project_id],
                |row| row.get(0),
            )
            .map_err(map_db_error)?;
        if !asset_belongs_to_restored_project {
            return Err(LibraryRepositoryError::Conflict);
        }
        restore_canvas_member(tx, member, project_id, member.asset_id)?;
    }
    if p.kind == ProjectKind::Canvas {
        ensure_canvas_outputs_for_project(tx, project_id, p.updated_at)?;
    }
    let value_ids = restore_custom_field_values(tx, &s.custom_field_values, project_id)?;
    restore_edit_history(tx, &s.history, "project", p.id, project_id, &value_ids)?;
    Ok(project_id)
}
fn restore_asset(tx: &Transaction<'_>, s: &AssetSnapshot) -> Result<i64, LibraryRepositoryError> {
    let a = &s.asset;
    let prompt_id = s
        .prompt
        .as_ref()
        .map(|prompt| restore_prompt(tx, prompt, prompt.project_id))
        .transpose()?;
    let params_json = serde_json::to_string(&a.generation_params)
        .map_err(|_| LibraryRepositoryError::InvalidData)?;
    let asset_id = if id_exists(tx, "assets", a.id)? {
        tx.execute("INSERT INTO assets(project_id,prompt_id,model_id,platform_id,media_type,path_kind,stored_path,file_name,mime_type,file_size,content_hash,width,height,duration_ms,frame_rate,has_audio,generation_params_json,rating,is_favorite,is_public,notes,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23)",params![a.media.project_id,prompt_id,s.model_id,s.platform_id,a.media.media_type.as_db_str(),a.media.path_kind.as_db_str(),a.media.stored_path,a.media.file_name,a.media.mime_type,a.media.file_size,a.media.content_hash,a.media.width,a.media.height,a.media.duration_ms,a.media.frame_rate,a.media.has_audio,params_json,a.rating,a.is_favorite,a.is_public,a.notes,a.created_at,a.updated_at]).map_err(map_db_error)?;
        tx.last_insert_rowid()
    } else {
        tx.execute("INSERT INTO assets(id,project_id,prompt_id,model_id,platform_id,media_type,path_kind,stored_path,file_name,mime_type,file_size,content_hash,width,height,duration_ms,frame_rate,has_audio,generation_params_json,rating,is_favorite,is_public,notes,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24)",params![a.id,a.media.project_id,prompt_id,s.model_id,s.platform_id,a.media.media_type.as_db_str(),a.media.path_kind.as_db_str(),a.media.stored_path,a.media.file_name,a.media.mime_type,a.media.file_size,a.media.content_hash,a.media.width,a.media.height,a.media.duration_ms,a.media.frame_rate,a.media.has_audio,params_json,a.rating,a.is_favorite,a.is_public,a.notes,a.created_at,a.updated_at]).map_err(map_db_error)?;
        a.id
    };
    replace_categories(
        tx,
        "asset_categories",
        "asset_id",
        asset_id,
        &s.category_ids,
        a.created_at,
    )?;
    replace_tags(
        tx,
        "asset_tags",
        "asset_id",
        asset_id,
        &s.tag_ids,
        a.created_at,
    )?;
    let value_ids = restore_custom_field_values(tx, &s.custom_field_values, asset_id)?;
    restore_edit_history(tx, &s.history, "asset", a.id, asset_id, &value_ids)?;
    if let Some(cover) = &s.cover {
        tx.execute(
            "INSERT INTO asset_covers(asset_id,source_type,stored_path,frame_timestamp_ms,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                asset_id,
                cover.source_type,
                cover.stored_path,
                cover.frame_timestamp_ms,
                cover.created_at,
                cover.updated_at,
            ],
        )
        .map_err(map_db_error)?;
    }
    match (a.media.project_id, &s.canvas_member) {
        (Some(project_id), Some(member)) => {
            if member.project_id != project_id || member.asset_id != a.id {
                return Err(LibraryRepositoryError::InvalidData);
            }
            let relation_target_is_valid: bool = tx
                .query_row(
                    "SELECT EXISTS(
                        SELECT 1 FROM projects project
                        JOIN assets asset ON asset.project_id=project.id
                        WHERE project.id=?1 AND asset.id=?2
                    )",
                    params![project_id, asset_id],
                    |row| row.get(0),
                )
                .map_err(map_db_error)?;
            if !relation_target_is_valid {
                return Err(LibraryRepositoryError::Conflict);
            }
            // simple 项目保留隐藏画布配置，切回 canvas 时可以原样继续使用。
            restore_canvas_member(tx, member, project_id, asset_id)?;
        }
        (None, Some(_)) => return Err(LibraryRepositoryError::InvalidData),
        (_, None) => {}
    }
    Ok(asset_id)
}

fn restore_canvas_member(
    tx: &Transaction<'_>,
    member: &CanvasMemberSnapshot,
    project_id: i64,
    asset_id: i64,
) -> Result<(), LibraryRepositoryError> {
    tx.execute(
        "INSERT INTO canvas_project_members
         (project_id,asset_id,role,reference_name,position,created_at,updated_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            project_id,
            asset_id,
            member.role.as_db_str(),
            member.reference_name,
            member.position,
            member.created_at,
            member.updated_at
        ],
    )
    .map_err(map_db_error)?;
    Ok(())
}

fn restore_trash_in_transaction(
    tx: &Transaction<'_>,
    trash_id: i64,
) -> Result<TrashEntry, LibraryRepositoryError> {
    let (kind, _entity_id, snapshot, deleted_at): (String, i64, String, i64) = tx
        .query_row(
            "SELECT entity_type,entity_id,snapshot_json,deleted_at FROM trash_entries WHERE id=?1",
            [trash_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(map_db_error)?
        .ok_or(LibraryRepositoryError::NotFound)?;
    let (entity_type, restored_entity_id, display_name, media_type) = if kind == "project" {
        let value: ProjectSnapshot = parse_json(&snapshot)?;
        let display_name = value.project.title.clone();
        let restored_entity_id = restore_project(tx, &value)?;
        (
            TrashEntityType::Project,
            restored_entity_id,
            display_name,
            None,
        )
    } else if kind == "asset" {
        let value: AssetSnapshot = parse_json(&snapshot)?;
        let display_name = value.asset.media.file_name.clone();
        let media_type = value.asset.media.media_type;
        let restored_entity_id = restore_asset(tx, &value)?;
        (
            TrashEntityType::Asset,
            restored_entity_id,
            display_name,
            Some(media_type),
        )
    } else {
        return Err(LibraryRepositoryError::InvalidData);
    };
    tx.execute("DELETE FROM trash_entries WHERE id=?1", [trash_id])
        .map_err(map_db_error)?;
    Ok(TrashEntry {
        id: trash_id,
        entity_type,
        entity_id: restored_entity_id,
        display_name,
        media_type,
        deleted_at,
    })
}

fn trash_entry_from_snapshot(
    id: i64,
    kind: &str,
    entity_id: i64,
    snapshot: &str,
    deleted_at: i64,
) -> Result<TrashEntry, LibraryRepositoryError> {
    let (entity_type, display_name, media_type) = match kind {
        "project" => {
            let value: ProjectSnapshot = parse_json(snapshot)?;
            (TrashEntityType::Project, value.project.title, None)
        }
        "asset" => {
            let value: AssetSnapshot = parse_json(snapshot)?;
            (
                TrashEntityType::Asset,
                value.asset.media.file_name,
                Some(value.asset.media.media_type),
            )
        }
        _ => return Err(LibraryRepositoryError::InvalidData),
    };
    Ok(TrashEntry {
        id,
        entity_type,
        entity_id,
        display_name,
        media_type,
        deleted_at,
    })
}

#[cfg(test)]
mod tests {
    use rusqlite::{Connection, params};

    use super::{LibraryRepository, LibraryRepositoryError, ModelComparisonFilter, canvas_member};
    use crate::domain::{
        AssetListQuery, AssetMetadata, AssetOrderChangeMode, AssetSearchField, BulkAssetEditInput,
        BulkNullableTextEdit, CanvasMemberRole, CategoryDeleteAction, CategoryInput, CreateAsset,
        CreateProject, DimensionInput, MediaType, MetadataPresetKind, ModelComparisonQuery,
        ModelComparisonScope, PathKind, ProjectKind, PromptText, TrashEntityType,
    };

    fn database() -> Connection {
        let connection = Connection::open_in_memory().expect("应能打开内存数据库");
        connection
            .pragma_update(None, "foreign_keys", true)
            .expect("应能启用外键");
        connection
            .execute_batch(include_str!("../migrations/0001_initial.sql"))
            .expect("应能建立 v1 schema");
        connection
            .execute_batch(include_str!("../migrations/0002_search.sql"))
            .expect("应能建立 v2 搜索 schema");
        connection
            .execute_batch(include_str!("../migrations/0003_ai.sql"))
            .expect("应能建立 v3 AI schema");
        connection
            .execute_batch(include_str!("../migrations/0004_p1_library.sql"))
            .expect("应能建立 v4 P1 schema");
        connection
            .execute_batch(include_str!("../migrations/0005_media_integrity.sql"))
            .expect("应能建立 v5 媒体完整性 schema");
        connection
            .execute_batch(include_str!(
                "../migrations/0006_asset_order_and_taxonomy.sql"
            ))
            .expect("应能建立 v6 作品编号 schema");
        connection
            .execute_batch(include_str!("../migrations/0007_common_taxonomy.sql"))
            .expect("应能建立 v7 常用分类 schema");
        connection
            .execute_batch(include_str!(
                "../migrations/0008_metadata_presets_and_import_order.sql"
            ))
            .expect("应能建立 v8 元数据预设与导入排序 schema");
        connection
            .execute_batch(include_str!(
                "../migrations/0009_media_metadata_and_order_repair.sql"
            ))
            .expect("应能建立 v9 媒体元数据与编号修复 schema");
        connection
            .execute_batch(include_str!("../migrations/0010_canvas_projects.sql"))
            .expect("应能建立 v10 画布项目 schema");
        connection
            .pragma_update(None, "user_version", 10_u32)
            .expect("应能标记 v10 schema");
        crate::repositories::DatabaseRepository::validate_v10(&connection)
            .expect("v10 schema 应通过完整性校验");
        connection
    }

    fn fts_asset_ids(connection: &Connection, keyword: &str) -> Vec<i64> {
        let query = format!("\"{}\"", keyword.replace('"', "\"\""));
        let mut statement = connection
            .prepare(
                "SELECT CAST(asset_id AS INTEGER)
                   FROM asset_search
                  WHERE asset_search MATCH ?1
                  ORDER BY CAST(asset_id AS INTEGER)",
            )
            .expect("应能准备 FTS 验证查询");
        statement
            .query_map([query], |row| row.get(0))
            .expect("应能执行 FTS 验证查询")
            .collect::<Result<Vec<_>, _>>()
            .expect("应能读取 FTS 验证结果")
    }

    fn project(title: &str) -> CreateProject {
        CreateProject {
            kind: ProjectKind::Simple,
            title: title.to_owned(),
            description: String::new(),
            prompt: PromptText {
                prompt_zh: format!("{title} 中文提示词"),
                prompt_en: format!("{title} prompt"),
                negative_prompt: "模糊".to_owned(),
            },
            rating: 4,
            is_favorite: true,
            is_public: false,
            notes: "私密备注".to_owned(),
            category_ids: Vec::new(),
            tag_ids: Vec::new(),
        }
    }

    fn asset(project_id: Option<i64>) -> CreateAsset {
        CreateAsset {
            media: AssetMetadata {
                project_id,
                media_type: MediaType::Image,
                path_kind: PathKind::Managed,
                stored_path: "media/images/work.png".to_owned(),
                file_name: "work.png".to_owned(),
                mime_type: Some("image/png".to_owned()),
                file_size: Some(128),
                content_hash: Some("hash".to_owned()),
                width: Some(64),
                height: Some(64),
                duration_ms: None,
                frame_rate: None,
                has_audio: None,
            },
            prompt: PromptText::default(),
            model: Some("Model A".to_owned()),
            platform: Some("Platform A".to_owned()),
            generation_params: serde_json::json!({"seed": 7}),
            rating: 5,
            is_favorite: true,
            is_public: true,
            notes: String::new(),
            category_ids: Vec::new(),
            tag_ids: Vec::new(),
        }
    }

    #[test]
    fn project_crud_uses_stable_keyset_and_rolls_back_invalid_relations() {
        let mut connection = database();
        let first = LibraryRepository::create_project(&mut connection, &project("第一项"), 10)
            .expect("应能创建项目");
        let second = LibraryRepository::create_project(&mut connection, &project("第二项"), 20)
            .expect("应能创建项目");

        let page = LibraryRepository::list_projects(&connection, None, 1).expect("应能分页");
        assert_eq!(page.items[0].id, second);
        let next = LibraryRepository::list_projects(&connection, page.next_cursor, 1)
            .expect("游标应能读取下一页");
        assert_eq!(next.items[0].id, first);

        let mut invalid = project("应回滚");
        invalid.category_ids.push(999);
        assert_eq!(
            LibraryRepository::create_project(&mut connection, &invalid, 30),
            Err(LibraryRepositoryError::NotFound)
        );
        let count: i64 = connection
            .query_row("SELECT count(*) FROM projects", [], |row| row.get(0))
            .expect("应能读取项目数量");
        assert_eq!(count, 2, "关系写入失败必须回滚主记录");
    }

    #[test]
    fn category_replacement_is_atomic_and_preserves_relation() {
        let mut connection = database();
        let dimension = LibraryRepository::create_dimension(
            &mut connection,
            &DimensionInput {
                name: "测试视觉风格".to_owned(),
                allows_multiple: true,
                ai_can_suggest_new: false,
                is_enabled: true,
            },
            1,
        )
        .expect("应能创建维度");
        let old = LibraryRepository::create_category(
            &mut connection,
            &CategoryInput {
                dimension_id: dimension,
                name: "旧分类".to_owned(),
                aliases: Vec::new(),
                description: String::new(),
                color: None,
                icon: None,
                is_enabled: true,
            },
            2,
        )
        .expect("应能创建分类");
        let replacement = LibraryRepository::create_category(
            &mut connection,
            &CategoryInput {
                dimension_id: dimension,
                name: "新分类".to_owned(),
                aliases: Vec::new(),
                description: String::new(),
                color: None,
                icon: None,
                is_enabled: true,
            },
            3,
        )
        .expect("应能创建替代分类");
        let mut input = project("带分类项目");
        input.category_ids.push(old);
        let project_id =
            LibraryRepository::create_project(&mut connection, &input, 4).expect("应能创建项目");

        let impact = LibraryRepository::delete_category(
            &mut connection,
            old,
            CategoryDeleteAction::ReplaceWith(replacement),
        )
        .expect("应能事务替换分类");
        assert_eq!(impact.project_count, 1);
        assert_eq!(
            LibraryRepository::get_project(&connection, project_id)
                .expect("项目应存在")
                .category_ids,
            vec![replacement]
        );
    }

    #[test]
    fn asset_trash_restore_and_purge_never_operate_on_media() {
        let mut connection = database();
        let mut project_input = project("项目");
        project_input.kind = ProjectKind::Canvas;
        let project_id = LibraryRepository::create_project(&mut connection, &project_input, 1)
            .expect("应能创建画布项目");
        let asset_id =
            LibraryRepository::create_asset(&mut connection, &asset(Some(project_id)), 2)
                .expect("应能创建作品");
        let mut companion = asset(Some(project_id));
        companion.media.stored_path = "media/images/companion.png".to_owned();
        companion.media.file_name = "companion.png".to_owned();
        LibraryRepository::create_asset(&mut connection, &companion, 2).expect("应能创建输出作品");
        LibraryRepository::set_canvas_member(
            &mut connection,
            project_id,
            asset_id,
            CanvasMemberRole::Reference,
            Some("subject"),
            2,
        )
        .expect("应能设置参考图");
        connection
            .execute(
                "UPDATE canvas_project_members SET position=17 WHERE asset_id=?1",
                [asset_id],
            )
            .expect("应能设置画布位置");
        let trash_id =
            LibraryRepository::move_to_trash(&mut connection, TrashEntityType::Asset, asset_id, 3)
                .expect("应能移入回收站");
        let trash = LibraryRepository::list_trash(&connection, None, 10).expect("应能读取回收站");
        assert_eq!(trash.items[0].display_name, "work.png");
        assert_eq!(trash.items[0].media_type, Some(MediaType::Image));
        assert_eq!(
            LibraryRepository::get_asset(&connection, asset_id),
            Err(LibraryRepositoryError::NotFound)
        );
        let restored =
            LibraryRepository::restore_trash(&mut connection, trash_id).expect("应能恢复作品");
        assert_eq!(restored.display_name, "work.png");
        assert_eq!(restored.media_type, Some(MediaType::Image));
        let restored_member =
            canvas_member(&connection, project_id, asset_id).expect("应能恢复画布成员");
        assert_eq!(restored_member.role, CanvasMemberRole::Reference);
        assert_eq!(restored_member.reference_name.as_deref(), Some("subject"));
        let restored_position: i64 = connection
            .query_row(
                "SELECT position FROM canvas_project_members WHERE asset_id=?1",
                [asset_id],
                |row| row.get(0),
            )
            .expect("应能读取恢复后位置");
        assert_eq!(restored_position, 17);
        assert_eq!(
            LibraryRepository::get_asset(&connection, asset_id)
                .expect("恢复后作品应存在")
                .summary
                .generation_params,
            serde_json::json!({"seed": 7})
        );

        let trash_id =
            LibraryRepository::move_to_trash(&mut connection, TrashEntityType::Asset, asset_id, 4)
                .expect("应能再次移入回收站");
        LibraryRepository::purge_trash(&mut connection, trash_id).expect("应能永久删除记录");
        assert_eq!(
            LibraryRepository::get_asset(&connection, asset_id),
            Err(LibraryRepositoryError::NotFound)
        );
    }

    #[test]
    fn asset_canvas_member_restore_failure_rolls_back_asset_and_keeps_trash() {
        let mut connection = database();
        let mut project_input = project("画布项目");
        project_input.kind = ProjectKind::Canvas;
        let project_id = LibraryRepository::create_project(&mut connection, &project_input, 1)
            .expect("应能创建画布项目");
        let asset_id =
            LibraryRepository::create_asset(&mut connection, &asset(Some(project_id)), 2)
                .expect("应能创建待回收作品");
        let mut second = asset(Some(project_id));
        second.media.stored_path = "media/images/second.png".to_owned();
        second.media.file_name = "second.png".to_owned();
        let second_id =
            LibraryRepository::create_asset(&mut connection, &second, 2).expect("应能创建第二作品");
        let mut output = asset(Some(project_id));
        output.media.stored_path = "media/images/final.png".to_owned();
        output.media.file_name = "final.png".to_owned();
        LibraryRepository::create_asset(&mut connection, &output, 2).expect("应能创建保留输出");
        LibraryRepository::set_canvas_member(
            &mut connection,
            project_id,
            asset_id,
            CanvasMemberRole::Reference,
            Some("subject"),
            3,
        )
        .expect("应能设置待回收参考图");
        let trash_id =
            LibraryRepository::move_to_trash(&mut connection, TrashEntityType::Asset, asset_id, 4)
                .expect("应能移入回收站");
        LibraryRepository::set_canvas_member(
            &mut connection,
            project_id,
            second_id,
            CanvasMemberRole::Reference,
            Some("subject"),
            5,
        )
        .expect("应能占用原参考名");

        assert_eq!(
            LibraryRepository::restore_trash(&mut connection, trash_id),
            Err(LibraryRepositoryError::Conflict)
        );
        assert_eq!(
            LibraryRepository::get_asset(&connection, asset_id),
            Err(LibraryRepositoryError::NotFound),
            "成员关系恢复失败时不得留下半恢复作品"
        );
        assert_eq!(
            LibraryRepository::list_trash(&connection, None, 10)
                .expect("应能读取回收站")
                .items[0]
                .id,
            trash_id
        );
    }

    #[test]
    fn moving_assets_to_trash_is_atomic_when_one_target_is_missing() {
        let mut connection = database();
        let asset_id = LibraryRepository::create_asset(&mut connection, &asset(None), 1)
            .expect("应能创建作品");

        assert_eq!(
            LibraryRepository::move_assets_to_trash(&mut connection, &[asset_id, 999], 2),
            Err(LibraryRepositoryError::NotFound)
        );
        assert!(
            LibraryRepository::get_asset(&connection, asset_id).is_ok(),
            "任一目标无效时，不得移入任何作品"
        );
        let trash_count: i64 = connection
            .query_row("SELECT count(*) FROM trash_entries", [], |row| row.get(0))
            .expect("应能读取回收站数量");
        assert_eq!(trash_count, 0);
    }

    #[test]
    fn video_cover_metadata_follows_asset_trash_restore_lifecycle() {
        let mut connection = database();
        let mut video = asset(None);
        video.media.media_type = MediaType::Video;
        video.media.stored_path = "media/videos/work.mp4".to_owned();
        video.media.file_name = "work.mp4".to_owned();
        video.media.mime_type = Some("video/mp4".to_owned());
        let asset_id =
            LibraryRepository::create_asset(&mut connection, &video, 1).expect("应能创建视频作品");
        connection
            .execute(
                "INSERT INTO asset_covers(asset_id,source_type,stored_path,frame_timestamp_ms,created_at,updated_at)
                 VALUES(?1,'key_frame','media/covers/work.png',1000,2,2)",
                [asset_id],
            )
            .expect("应能创建视频封面元数据");

        let trash_id =
            LibraryRepository::move_to_trash(&mut connection, TrashEntityType::Asset, asset_id, 3)
                .expect("应能把视频移入回收站");
        let cover_count: i64 = connection
            .query_row("SELECT count(*) FROM asset_covers", [], |row| row.get(0))
            .expect("应能检查封面记录");
        assert_eq!(cover_count, 0, "回收站期间封面元数据只存在快照中");

        LibraryRepository::restore_trash(&mut connection, trash_id)
            .expect("应能连同封面元数据恢复视频");
        let restored: (String, i64) = connection
            .query_row(
                "SELECT source_type,frame_timestamp_ms FROM asset_covers WHERE asset_id=?1",
                [asset_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("恢复后封面元数据应存在");
        assert_eq!(restored, ("key_frame".to_owned(), 1000));
    }

    #[test]
    fn asset_trash_restores_and_purge_cleans_all_p1_state() {
        let mut connection = database();
        let asset_id = LibraryRepository::create_asset(&mut connection, &asset(None), 1)
            .expect("应能创建作品");
        let prompt_id: i64 = connection
            .query_row(
                "SELECT prompt_id FROM assets WHERE id=?1",
                [asset_id],
                |row| row.get(0),
            )
            .expect("作品应有关联提示词");
        connection.execute(
            "INSERT INTO prompt_versions(prompt_id,version,prompt_zh,prompt_en,negative_prompt,created_at) VALUES(?1,1,'版本一','version one','blur',2)",
            [prompt_id],
        ).expect("应能创建提示词版本");
        connection.execute(
            "INSERT INTO custom_fields(name,target_type,value_type,options_json,is_enabled,created_at,updated_at) VALUES('用途','asset','text','{}',1,2,2)",
            [],
        ).expect("应能创建自定义字段");
        let field_id = connection.last_insert_rowid();
        connection.execute(
            "INSERT INTO custom_field_values(field_id,target_type,target_id,value_json,source,status,created_at,updated_at) VALUES(?1,'asset',?2,'\"封面\"','manual','confirmed',2,2)",
            params![field_id, asset_id],
        ).expect("应能创建字段值");
        let field_value_id = connection.last_insert_rowid();
        for (target_type, target_id, field) in [
            ("asset", asset_id, "rating"),
            ("prompt", prompt_id, "prompt_zh"),
            ("custom_field_value", field_value_id, "value"),
        ] {
            connection.execute(
                "INSERT INTO edit_history(target_type,target_id,action,changed_fields_json,source,status,created_at,updated_at) VALUES(?1,?2,'update',json_array(?3),'manual','confirmed',2,2)",
                params![target_type, target_id, field],
            ).expect("应能创建编辑历史");
        }

        let trash_id =
            LibraryRepository::move_to_trash(&mut connection, TrashEntityType::Asset, asset_id, 3)
                .expect("应能连同 P1 状态移入回收站");
        for table in ["prompt_versions", "custom_field_values", "edit_history"] {
            let count: i64 = connection
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .expect("应能读取状态表");
            assert_eq!(count, 0, "软删除期间 P1 状态只能存在回收站快照中");
        }

        LibraryRepository::restore_trash(&mut connection, trash_id)
            .expect("应能原子恢复作品和 P1 状态");
        let version_count: i64 = connection
            .query_row(
                "SELECT count(*) FROM prompt_versions WHERE prompt_id=?1",
                [prompt_id],
                |row| row.get(0),
            )
            .expect("应能读取恢复版本");
        let value_count: i64 = connection.query_row(
            "SELECT count(*) FROM custom_field_values WHERE target_type='asset' AND target_id=?1",
            [asset_id],
            |row| row.get(0),
        ).expect("应能读取恢复字段值");
        let history_count: i64 = connection
            .query_row("SELECT count(*) FROM edit_history", [], |row| row.get(0))
            .expect("应能读取恢复历史");
        assert_eq!((version_count, value_count, history_count), (1, 1, 3));

        let trash_id =
            LibraryRepository::move_to_trash(&mut connection, TrashEntityType::Asset, asset_id, 4)
                .expect("应能再次移入回收站");
        LibraryRepository::purge_trash(&mut connection, trash_id).expect("应能永久删除回收站快照");
        assert_eq!(
            connection
                .query_row("SELECT count(*) FROM trash_entries", [], |row| row
                    .get::<_, i64>(0))
                .expect("应能读取回收站"),
            0
        );
        for table in ["prompt_versions", "custom_field_values", "edit_history"] {
            let count: i64 = connection
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .expect("应能读取状态表");
            assert_eq!(count, 0, "永久删除后不得遗留 P1 多态数据");
        }
    }

    #[test]
    fn trashing_one_asset_preserves_a_prompt_shared_with_another_asset() {
        let mut connection = database();
        let first = LibraryRepository::create_asset(&mut connection, &asset(None), 1)
            .expect("应能创建首个作品");
        let mut second_input = asset(None);
        second_input.media.stored_path = "media/images/second.png".to_owned();
        second_input.media.file_name = "second.png".to_owned();
        second_input.media.content_hash = Some("second-hash".to_owned());
        let second = LibraryRepository::create_asset(&mut connection, &second_input, 2)
            .expect("应能创建第二个作品");
        let (shared_prompt, second_prompt): (i64, i64) = connection
            .query_row(
                "SELECT (SELECT prompt_id FROM assets WHERE id=?1),(SELECT prompt_id FROM assets WHERE id=?2)",
                params![first, second],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("两个作品都应有提示词");
        connection
            .execute(
                "UPDATE assets SET prompt_id=?1 WHERE id=?2",
                params![shared_prompt, second],
            )
            .expect("应能构造合法的共享提示词关系");
        connection
            .execute("DELETE FROM prompts WHERE id=?1", [second_prompt])
            .expect("应能清理第二个孤立提示词");
        connection
            .execute(
                "INSERT INTO prompt_versions(prompt_id,version,prompt_zh,prompt_en,negative_prompt,created_at) VALUES(?1,1,'共享版本','','',2)",
                [shared_prompt],
            )
            .expect("应能创建共享提示词版本");

        let trash_id =
            LibraryRepository::move_to_trash(&mut connection, TrashEntityType::Asset, first, 3)
                .expect("移除一个共享作品不得影响另一个");
        let remaining_prompt: i64 = connection
            .query_row(
                "SELECT prompt_id FROM assets WHERE id=?1",
                [second],
                |row| row.get(0),
            )
            .expect("另一个作品必须保留提示词");
        let version_count: i64 = connection
            .query_row(
                "SELECT count(*) FROM prompt_versions WHERE prompt_id=?1",
                [shared_prompt],
                |row| row.get(0),
            )
            .expect("共享版本必须保留");
        assert_eq!(remaining_prompt, shared_prompt);
        assert_eq!(version_count, 1);

        let restored = LibraryRepository::restore_trash(&mut connection, trash_id)
            .expect("应能恢复共享提示词快照");
        let restored_prompt: i64 = connection
            .query_row(
                "SELECT prompt_id FROM assets WHERE id=?1",
                [restored.entity_id],
                |row| row.get(0),
            )
            .expect("恢复作品应重新关联共享提示词");
        assert_ne!(
            restored_prompt, shared_prompt,
            "恢复应使用独立快照副本，避免整数 ID 被误认作稳定身份"
        );
        let restored_version_count: i64 = connection
            .query_row(
                "SELECT count(*) FROM prompt_versions WHERE prompt_id=?1",
                [restored_prompt],
                |row| row.get(0),
            )
            .expect("独立副本应恢复版本");
        assert_eq!(restored_version_count, 1);
    }

    #[test]
    fn shared_prompt_restore_does_not_bind_to_a_reused_prompt_id() {
        let mut connection = database();
        let mut first_input = asset(None);
        first_input.prompt.prompt_zh = "原共享提示词".to_owned();
        let first = LibraryRepository::create_asset(&mut connection, &first_input, 1)
            .expect("应能创建首个作品");
        let mut second_input = asset(None);
        second_input.media.stored_path = "media/images/second-shared.png".to_owned();
        second_input.media.file_name = "second-shared.png".to_owned();
        second_input.media.content_hash = Some("second-shared-hash".to_owned());
        let second = LibraryRepository::create_asset(&mut connection, &second_input, 2)
            .expect("应能创建第二个作品");
        let (original_prompt, unused_prompt): (i64, i64) = connection.query_row(
            "SELECT (SELECT prompt_id FROM assets WHERE id=?1),(SELECT prompt_id FROM assets WHERE id=?2)",
            params![first, second],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).expect("两个作品都应有提示词");
        connection
            .execute(
                "UPDATE assets SET prompt_id=?1 WHERE id=?2",
                params![original_prompt, second],
            )
            .expect("应能共享提示词");
        connection
            .execute("DELETE FROM prompts WHERE id=?1", [unused_prompt])
            .expect("应能清理孤立提示词");
        connection.execute(
            "INSERT INTO prompt_versions(prompt_id,version,prompt_zh,prompt_en,negative_prompt,created_at) VALUES(?1,1,'原版本','','',2)",
            [original_prompt],
        ).expect("应能创建原版本");

        let first_trash =
            LibraryRepository::move_to_trash(&mut connection, TrashEntityType::Asset, first, 3)
                .expect("应能先回收首个共享作品");
        LibraryRepository::move_to_trash(&mut connection, TrashEntityType::Asset, second, 4)
            .expect("回收最后引用后应删除原提示词");
        let original_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM prompts WHERE id=?1)",
                [original_prompt],
                |row| row.get(0),
            )
            .expect("应能确认原提示词已删除");
        assert!(!original_exists);

        let mut replacement_input = asset(None);
        replacement_input.prompt.prompt_zh = "后来创建的提示词".to_owned();
        replacement_input.media.stored_path = "media/images/later.png".to_owned();
        replacement_input.media.file_name = "later.png".to_owned();
        replacement_input.media.content_hash = Some("later-hash".to_owned());
        let replacement = LibraryRepository::create_asset(&mut connection, &replacement_input, 5)
            .expect("应能创建后来作品并复用提示词 ID");
        let reused_prompt: i64 = connection
            .query_row(
                "SELECT prompt_id FROM assets WHERE id=?1",
                [replacement],
                |row| row.get(0),
            )
            .expect("后来作品应有提示词");
        assert_eq!(
            reused_prompt, original_prompt,
            "测试必须实际复用原提示词 ID"
        );

        let restored = LibraryRepository::restore_trash(&mut connection, first_trash)
            .expect("共享快照遇到复用 ID 时必须恢复独立提示词");
        let (restored_prompt, restored_text): (i64, String) = connection.query_row(
            "SELECT a.prompt_id,p.prompt_zh FROM assets a JOIN prompts p ON p.id=a.prompt_id WHERE a.id=?1",
            [restored.entity_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).expect("恢复作品应有关联提示词");
        assert_ne!(restored_prompt, reused_prompt);
        assert_eq!(restored_text, "原共享提示词");
        let original_version_count: i64 = connection
            .query_row(
                "SELECT count(*) FROM prompt_versions WHERE prompt_id=?1 AND prompt_zh='原版本'",
                [restored_prompt],
                |row| row.get(0),
            )
            .expect("原版本应恢复到独立提示词");
        assert_eq!(original_version_count, 1);
        let replacement_text: String = connection
            .query_row(
                "SELECT prompt_zh FROM prompts WHERE id=?1",
                [reused_prompt],
                |row| row.get(0),
            )
            .expect("后来提示词不得被覆盖");
        assert_eq!(replacement_text, "后来创建的提示词");
    }

    #[test]
    fn restore_remaps_reused_asset_prompt_value_and_history_ids() {
        let mut connection = database();
        let original = LibraryRepository::create_asset(&mut connection, &asset(None), 1)
            .expect("应能创建原作品");
        connection.execute(
            "INSERT INTO custom_fields(name,target_type,value_type,options_json,is_enabled,created_at,updated_at) VALUES('用途','asset','text','{}',1,1,1)",
            [],
        ).expect("应能创建字段");
        let field_id = connection.last_insert_rowid();
        connection.execute(
            "INSERT INTO custom_field_values(field_id,target_type,target_id,value_json,source,status,created_at,updated_at) VALUES(?1,'asset',?2,'\"原值\"','manual','confirmed',1,1)",
            params![field_id, original],
        ).expect("应能创建原字段值");
        let original_value_id = connection.last_insert_rowid();
        connection.execute(
            "INSERT INTO edit_history(target_type,target_id,action,changed_fields_json,source,status,created_at,updated_at) VALUES('custom_field_value',?1,'update','[\"value\"]','manual','confirmed',1,1)",
            [original_value_id],
        ).expect("应能创建原历史");

        let trash_id =
            LibraryRepository::move_to_trash(&mut connection, TrashEntityType::Asset, original, 2)
                .expect("应能移入回收站");
        let mut replacement_input = asset(None);
        replacement_input.media.stored_path = "media/images/replacement.png".to_owned();
        replacement_input.media.file_name = "replacement.png".to_owned();
        replacement_input.media.content_hash = Some("replacement-hash".to_owned());
        let replacement = LibraryRepository::create_asset(&mut connection, &replacement_input, 3)
            .expect("应能创建并复用最大 rowid 的新作品");
        assert_eq!(replacement, original, "测试必须实际复用作品主键");
        connection.execute(
            "INSERT INTO custom_field_values(field_id,target_type,target_id,value_json,source,status,created_at,updated_at) VALUES(?1,'asset',?2,'\"新值\"','manual','confirmed',3,3)",
            params![field_id, replacement],
        ).expect("应能复用字段值主键");
        let replacement_value_id = connection.last_insert_rowid();
        assert_eq!(replacement_value_id, original_value_id);
        connection.execute(
            "INSERT INTO edit_history(target_type,target_id,action,changed_fields_json,source,status,created_at,updated_at) VALUES('custom_field_value',?1,'update','[\"value\"]','manual','confirmed',3,3)",
            [replacement_value_id],
        ).expect("应能复用历史主键");

        let restored = LibraryRepository::restore_trash(&mut connection, trash_id)
            .expect("主键已复用时仍必须恢复");
        assert_ne!(restored.entity_id, replacement);
        assert!(LibraryRepository::get_asset(&connection, replacement).is_ok());
        assert!(LibraryRepository::get_asset(&connection, restored.entity_id).is_ok());
        let restored_value: i64 = connection
            .query_row(
                "SELECT id FROM custom_field_values WHERE target_type='asset' AND target_id=?1",
                [restored.entity_id],
                |row| row.get(0),
            )
            .expect("恢复字段值应重映射到新作品 ID");
        assert_ne!(restored_value, replacement_value_id);
        let remapped_history_count: i64 = connection.query_row(
            "SELECT count(*) FROM edit_history WHERE target_type='custom_field_value' AND target_id=?1",
            [restored_value],
            |row| row.get(0),
        ).expect("字段值历史应重映射到新字段值 ID");
        assert_eq!(remapped_history_count, 1);
    }

    #[test]
    fn update_history_records_only_actual_fields_and_skips_noop() {
        let mut connection = database();
        let project_id = LibraryRepository::create_project(&mut connection, &project("项目"), 1)
            .expect("应能创建项目");
        let unchanged_project = project("项目");
        LibraryRepository::update_project(&mut connection, project_id, &unchanged_project, 2)
            .expect("无变化更新仍可幂等完成");
        let project_history: i64 = connection
            .query_row(
                "SELECT count(*) FROM edit_history WHERE target_type='project' AND target_id=?1",
                [project_id],
                |row| row.get(0),
            )
            .expect("应能读取项目历史");
        assert_eq!(project_history, 0);
        let mut renamed_project = unchanged_project;
        renamed_project.title = "新项目名".to_owned();
        LibraryRepository::update_project(&mut connection, project_id, &renamed_project, 3)
            .expect("应能更新项目标题");
        let project_fields: String = connection.query_row(
            "SELECT changed_fields_json FROM edit_history WHERE target_type='project' AND target_id=?1",
            [project_id],
            |row| row.get(0),
        ).expect("应能读取项目变更字段");
        assert_eq!(project_fields, "[\"title\"]");

        let asset_id = LibraryRepository::create_asset(&mut connection, &asset(None), 4)
            .expect("应能创建作品");
        let unchanged_asset = asset(None);
        LibraryRepository::update_asset(&mut connection, asset_id, &unchanged_asset, 5)
            .expect("无变化作品更新仍可幂等完成");
        let asset_history: i64 = connection
            .query_row(
                "SELECT count(*) FROM edit_history WHERE target_type='asset' AND target_id=?1",
                [asset_id],
                |row| row.get(0),
            )
            .expect("应能读取作品历史");
        assert_eq!(asset_history, 0);
        let mut rated_asset = unchanged_asset;
        rated_asset.rating = 3;
        LibraryRepository::update_asset(&mut connection, asset_id, &rated_asset, 6)
            .expect("应能只更新评分");
        let asset_fields: String = connection.query_row(
            "SELECT changed_fields_json FROM edit_history WHERE target_type='asset' AND target_id=?1",
            [asset_id],
            |row| row.get(0),
        ).expect("应能读取作品变更字段");
        assert_eq!(asset_fields, "[\"rating\"]");
    }

    #[test]
    fn moving_to_trash_rejects_pending_ai_suggestions_without_reviving_them() {
        let mut connection = database();
        let asset_id = LibraryRepository::create_asset(&mut connection, &asset(None), 1)
            .expect("应能创建作品");
        connection
            .execute(
                "INSERT INTO dimensions(name,allows_multiple,ai_can_suggest_new,is_enabled,created_at,updated_at) VALUES('风格',1,1,1,1,1)",
                [],
            )
            .expect("应能创建维度");
        let dimension_id = connection.last_insert_rowid();
        connection
            .execute(
                "INSERT INTO ai_suggestions(target_type,target_id,dimension_id,suggested_category_name,status,created_at,updated_at) VALUES('asset',?1,?2,'电影感','pending',1,1)",
                params![asset_id, dimension_id],
            )
            .expect("应能创建待审核建议");

        let trash_id =
            LibraryRepository::move_to_trash(&mut connection, TrashEntityType::Asset, asset_id, 2)
                .expect("应能移入回收站");
        let status: String = connection
            .query_row(
                "SELECT status FROM ai_suggestions WHERE target_type='asset' AND target_id=?1",
                [asset_id],
                |row| row.get(0),
            )
            .expect("应能读取建议状态");
        assert_eq!(status, "rejected");
        LibraryRepository::restore_trash(&mut connection, trash_id).expect("应能恢复作品");
        let status_after_restore: String = connection
            .query_row(
                "SELECT status FROM ai_suggestions WHERE target_type='asset' AND target_id=?1",
                [asset_id],
                |row| row.get(0),
            )
            .expect("恢复后应能读取建议状态");
        assert_eq!(status_after_restore, "rejected");
    }

    #[test]
    fn batch_asset_create_rolls_back_every_related_record_on_conflict() {
        let mut connection = database();
        let preset_counts_before: (i64, i64) = connection
            .query_row(
                "SELECT (SELECT count(*) FROM models),(SELECT count(*) FROM platforms)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("应能读取默认预设数量");
        let first = asset(None);
        let mut duplicate = asset(None);
        duplicate.media.file_name = "duplicate.png".to_owned();

        assert_eq!(
            LibraryRepository::create_imported_assets_batch(
                &mut connection,
                &[(first, Some(10)), (duplicate, Some(10))],
                10,
            ),
            Err(LibraryRepositoryError::Conflict)
        );
        for table in ["assets", "prompts"] {
            let count: i64 = connection
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .expect("应能检查批量回滚结果");
            assert_eq!(count, 0, "{table} 不得残留半批记录");
        }
        let preset_counts_after: (i64, i64) = connection
            .query_row(
                "SELECT (SELECT count(*) FROM models),(SELECT count(*) FROM platforms)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("应能再次读取预设数量");
        assert_eq!(
            preset_counts_after, preset_counts_before,
            "预设表不得残留半批记录"
        );
    }

    #[test]
    fn trash_list_uses_stable_deleted_at_cursor() {
        let mut connection = database();
        let first = LibraryRepository::create_asset(&mut connection, &asset(None), 1)
            .expect("应能创建第一件作品");
        let mut second_input = asset(None);
        second_input.media.stored_path = "media/images/second.png".to_owned();
        second_input.media.file_name = "second.png".to_owned();
        let second = LibraryRepository::create_asset(&mut connection, &second_input, 2)
            .expect("应能创建第二件作品");
        LibraryRepository::move_to_trash(&mut connection, TrashEntityType::Asset, first, 10)
            .expect("应能删除第一件作品");
        LibraryRepository::move_to_trash(&mut connection, TrashEntityType::Asset, second, 20)
            .expect("应能删除第二件作品");

        let first_page =
            LibraryRepository::list_trash(&connection, None, 1).expect("应能读取第一页");
        assert_eq!(first_page.items[0].entity_id, second);
        let next_page = LibraryRepository::list_trash(&connection, first_page.next_cursor, 1)
            .expect("应能按游标读取下一页");
        assert_eq!(next_page.items[0].entity_id, first);
        assert!(next_page.next_cursor.is_none());
    }

    #[test]
    fn project_asset_and_taxonomy_updates_form_a_complete_crud_cycle() {
        let mut connection = database();
        let project_id =
            LibraryRepository::create_project(&mut connection, &project("初始项目"), 1)
                .expect("应能创建项目");
        let mut updated_project = project("更新项目");
        updated_project.rating = 2;
        LibraryRepository::update_project(&mut connection, project_id, &updated_project, 2)
            .expect("应能更新项目");
        assert_eq!(
            LibraryRepository::get_project(&connection, project_id)
                .expect("应能读取更新项目")
                .summary
                .title,
            "更新项目"
        );

        let asset_id =
            LibraryRepository::create_asset(&mut connection, &asset(Some(project_id)), 3)
                .expect("应能创建作品");
        assert_eq!(
            LibraryRepository::get_project(&connection, project_id)
                .expect("项目详情应包含作品数")
                .summary
                .asset_count,
            1
        );
        assert_eq!(
            LibraryRepository::list_projects(&connection, None, 10)
                .expect("项目列表应包含作品数")
                .items[0]
                .asset_count,
            1
        );
        let mut updated_asset = asset(Some(project_id));
        updated_asset.prompt.prompt_zh = "更新提示词".to_owned();
        updated_asset.model = Some("Model B".to_owned());
        LibraryRepository::update_asset(&mut connection, asset_id, &updated_asset, 4)
            .expect("应能更新作品");
        let detail = LibraryRepository::get_asset(&connection, asset_id).expect("应能读取作品");
        assert_eq!(detail.summary.prompt.prompt_zh, "更新提示词");
        assert_eq!(detail.summary.model.as_deref(), Some("Model B"));
        let project_history: i64 = connection.query_row(
            "SELECT count(*) FROM edit_history WHERE target_type='project' AND target_id=?1 AND action='update' AND status='confirmed'",
            [project_id],
            |row| row.get(0),
        ).expect("应能读取项目修改历史");
        let asset_history: i64 = connection.query_row(
            "SELECT count(*) FROM edit_history WHERE target_type='asset' AND target_id=?1 AND action='update' AND status='confirmed'",
            [asset_id],
            |row| row.get(0),
        ).expect("应能读取作品修改历史");
        assert_eq!((project_history, asset_history), (1, 1));

        let dimension_input = DimensionInput {
            name: "测试主体".to_owned(),
            allows_multiple: true,
            ai_can_suggest_new: false,
            is_enabled: true,
        };
        let dimension = LibraryRepository::create_dimension(&mut connection, &dimension_input, 5)
            .expect("应能创建维度");
        let mut renamed_dimension = dimension_input.clone();
        renamed_dimension.name = "测试主体类型".to_owned();
        LibraryRepository::update_dimension(&mut connection, dimension, &renamed_dimension, 6)
            .expect("应能更新维度");
        let category = LibraryRepository::create_category(
            &mut connection,
            &CategoryInput {
                dimension_id: dimension,
                name: "人物".to_owned(),
                aliases: vec!["角色".to_owned()],
                description: String::new(),
                color: None,
                icon: None,
                is_enabled: true,
            },
            7,
        )
        .expect("应能创建分类");
        LibraryRepository::set_asset_categories(&mut connection, asset_id, &[category], 8)
            .expect("应能设置作品分类");
        assert_eq!(
            LibraryRepository::list_categories(&connection, Some(dimension))
                .expect("分类列表应包含作品数")[0]
                .asset_count,
            1
        );
        assert_eq!(
            LibraryRepository::category_impact(&connection, category)
                .expect("应能读取分类影响")
                .asset_count,
            1
        );
        LibraryRepository::delete_category(&mut connection, category, CategoryDeleteAction::Remove)
            .expect("应能仅移除分类关系");

        let tag = LibraryRepository::create_tag(&mut connection, "草稿", 9).expect("应能创建标签");
        LibraryRepository::update_tag(&mut connection, tag, "精选", 10).expect("应能更新标签");
        LibraryRepository::set_asset_tags(&mut connection, asset_id, &[tag], 11)
            .expect("应能设置作品标签");
        assert_eq!(
            LibraryRepository::list_tags(&connection).expect("标签列表应包含作品数")[0].asset_count,
            1
        );
        assert_eq!(
            LibraryRepository::delete_tag(&mut connection, tag)
                .expect("应能删除标签")
                .asset_count,
            1
        );
        LibraryRepository::delete_dimension(&mut connection, dimension)
            .expect("分类删除后应能删除维度");
        let final_detail =
            LibraryRepository::get_asset(&connection, asset_id).expect("作品应继续存在");
        assert!(final_detail.category_ids.is_empty());
        assert!(final_detail.tag_ids.is_empty());
    }

    #[test]
    fn project_trash_restore_recovers_relations_and_asset_membership() {
        let mut connection = database();
        let dimension = LibraryRepository::create_dimension(
            &mut connection,
            &DimensionInput {
                name: "项目类型".to_owned(),
                allows_multiple: true,
                ai_can_suggest_new: false,
                is_enabled: true,
            },
            1,
        )
        .expect("应能创建维度");
        let category = LibraryRepository::create_category(
            &mut connection,
            &CategoryInput {
                dimension_id: dimension,
                name: "实验".to_owned(),
                aliases: Vec::new(),
                description: String::new(),
                color: None,
                icon: None,
                is_enabled: true,
            },
            2,
        )
        .expect("应能创建分类");
        let tag =
            LibraryRepository::create_tag(&mut connection, "保留标签", 3).expect("应能创建标签");
        let mut project_input = project("trash-project-marker");
        project_input.kind = ProjectKind::Canvas;
        project_input.category_ids = vec![category];
        project_input.tag_ids = vec![tag];
        let project_id = LibraryRepository::create_project(&mut connection, &project_input, 4)
            .expect("应能创建项目");
        let asset_id =
            LibraryRepository::create_asset(&mut connection, &asset(Some(project_id)), 5)
                .expect("应能创建项目作品");
        let mut output = asset(Some(project_id));
        output.media.stored_path = "media/images/output.png".to_owned();
        output.media.file_name = "output.png".to_owned();
        let output_id =
            LibraryRepository::create_asset(&mut connection, &output, 5).expect("应能创建输出作品");
        LibraryRepository::set_canvas_member(
            &mut connection,
            project_id,
            asset_id,
            CanvasMemberRole::Reference,
            Some("style"),
            5,
        )
        .expect("应能设置参考图");
        connection
            .execute(
                "UPDATE canvas_project_members SET position=23 WHERE asset_id=?1",
                [asset_id],
            )
            .expect("应能设置画布位置");

        let trash_id = LibraryRepository::move_to_trash(
            &mut connection,
            TrashEntityType::Project,
            project_id,
            6,
        )
        .expect("应能把项目移入回收站");
        let trash =
            LibraryRepository::list_trash(&connection, None, 10).expect("应能读取项目回收站");
        assert_eq!(trash.items[0].display_name, "trash-project-marker");
        assert_eq!(trash.items[0].media_type, None);
        assert_eq!(
            LibraryRepository::get_asset(&connection, asset_id)
                .expect("删除项目不应删除作品")
                .summary
                .media
                .project_id,
            None
        );
        assert!(
            fts_asset_ids(&connection, "trash-project-marker").is_empty(),
            "已删除项目的词不得残留在作品 FTS 中"
        );

        assert_eq!(trash.items[0].id, trash_id);
        let restored_entry =
            LibraryRepository::undo_recent_delete(&mut connection).expect("应能撤销最近项目删除");
        assert_eq!(restored_entry.display_name, "trash-project-marker");
        assert_eq!(restored_entry.media_type, None);
        let restored =
            LibraryRepository::get_project(&connection, project_id).expect("项目应已恢复");
        assert_eq!(restored.category_ids, vec![category]);
        assert_eq!(restored.tag_ids, vec![tag]);
        assert_eq!(restored.summary.kind, ProjectKind::Canvas);
        let restored_member =
            canvas_member(&connection, project_id, asset_id).expect("应能恢复项目的参考图关系");
        assert_eq!(restored_member.role, CanvasMemberRole::Reference);
        assert_eq!(restored_member.reference_name.as_deref(), Some("style"));
        let restored_position: i64 = connection
            .query_row(
                "SELECT position FROM canvas_project_members WHERE asset_id=?1",
                [asset_id],
                |row| row.get(0),
            )
            .expect("应能读取恢复后位置");
        assert_eq!(restored_position, 23);
        assert_eq!(
            LibraryRepository::get_asset(&connection, asset_id)
                .expect("作品应继续存在")
                .summary
                .media
                .project_id,
            Some(project_id)
        );
        assert_eq!(
            fts_asset_ids(&connection, "trash-project-marker"),
            vec![asset_id, output_id]
        );
    }

    #[test]
    fn project_canvas_member_restore_failure_rolls_back_project_and_assignment() {
        let mut connection = database();
        let mut project_input = project("回滚画布");
        project_input.kind = ProjectKind::Canvas;
        let project_id = LibraryRepository::create_project(&mut connection, &project_input, 1)
            .expect("应能创建画布项目");
        let asset_id =
            LibraryRepository::create_asset(&mut connection, &asset(Some(project_id)), 2)
                .expect("应能创建参考图");
        let mut output = asset(Some(project_id));
        output.media.stored_path = "media/images/rollback-output.png".to_owned();
        output.media.file_name = "rollback-output.png".to_owned();
        LibraryRepository::create_asset(&mut connection, &output, 2).expect("应能创建输出作品");
        LibraryRepository::set_canvas_member(
            &mut connection,
            project_id,
            asset_id,
            CanvasMemberRole::Reference,
            Some("style"),
            3,
        )
        .expect("应能设置参考图");
        let trash_id = LibraryRepository::move_to_trash(
            &mut connection,
            TrashEntityType::Project,
            project_id,
            4,
        )
        .expect("应能移入回收站");
        connection
            .execute(
                "UPDATE assets SET media_type='video' WHERE id=?1",
                [asset_id],
            )
            .expect("应能模拟回收站期间媒体类型变化");

        assert_eq!(
            LibraryRepository::restore_trash(&mut connection, trash_id),
            Err(LibraryRepositoryError::Conflict)
        );
        assert_eq!(
            LibraryRepository::get_project(&connection, project_id),
            Err(LibraryRepositoryError::NotFound),
            "成员关系失败时不得留下半恢复项目"
        );
        assert_eq!(
            LibraryRepository::get_asset(&connection, asset_id)
                .expect("原作品应仍存在")
                .summary
                .media
                .project_id,
            None,
            "恢复失败时作品归属也必须回滚"
        );
        assert_eq!(
            LibraryRepository::list_trash(&connection, None, 10)
                .expect("应能读取回收站")
                .items[0]
                .id,
            trash_id
        );
    }

    #[test]
    fn canvas_reference_change_rejects_dangling_output_tokens() {
        let mut connection = database();
        let mut project_input = project("引用守卫");
        project_input.kind = ProjectKind::Canvas;
        let project_id = LibraryRepository::create_project(&mut connection, &project_input, 1)
            .expect("应能创建画布项目");
        let reference_id =
            LibraryRepository::create_asset(&mut connection, &asset(Some(project_id)), 2)
                .expect("应能创建参考图");
        let mut output = asset(Some(project_id));
        output.media.stored_path = "media/images/reference-output.png".to_owned();
        output.media.file_name = "reference-output.png".to_owned();
        let output_id =
            LibraryRepository::create_asset(&mut connection, &output, 2).expect("应能创建输出作品");
        LibraryRepository::set_canvas_member(
            &mut connection,
            project_id,
            reference_id,
            CanvasMemberRole::Reference,
            Some("style"),
            3,
        )
        .expect("应能设置参考图");
        LibraryRepository::update_canvas_output_prompt(
            &mut connection,
            project_id,
            output_id,
            &PromptText {
                prompt_zh: "@style 保留人物构图".to_owned(),
                ..PromptText::default()
            },
            4,
        )
        .expect("应能保存合法引用提示词");

        assert_eq!(
            LibraryRepository::set_canvas_member(
                &mut connection,
                project_id,
                reference_id,
                CanvasMemberRole::Reference,
                Some("portrait"),
                5,
            ),
            Err(LibraryRepositoryError::Conflict)
        );
        assert_eq!(
            LibraryRepository::set_canvas_member(
                &mut connection,
                project_id,
                reference_id,
                CanvasMemberRole::Output,
                None,
                5,
            ),
            Err(LibraryRepositoryError::Conflict)
        );
        assert_eq!(
            LibraryRepository::remove_assets_from_project(
                &mut connection,
                project_id,
                &[reference_id],
                5,
            ),
            Err(LibraryRepositoryError::Conflict)
        );
        assert_eq!(
            LibraryRepository::move_to_trash(
                &mut connection,
                TrashEntityType::Asset,
                reference_id,
                5,
            ),
            Err(LibraryRepositoryError::Conflict)
        );
        let mut moved_reference = asset(Some(project_id));
        moved_reference.media.project_id = None;
        assert_eq!(
            LibraryRepository::update_asset(&mut connection, reference_id, &moved_reference, 5,),
            Err(LibraryRepositoryError::Conflict)
        );
        let unchanged =
            canvas_member(&connection, project_id, reference_id).expect("冲突后参考关系应保持原样");
        assert_eq!(unchanged.role, CanvasMemberRole::Reference);
        assert_eq!(unchanged.reference_name.as_deref(), Some("style"));
    }

    #[test]
    fn canvas_project_rejects_removing_its_last_output_while_references_remain() {
        let mut connection = database();
        let mut project_input = project("输出守卫");
        project_input.kind = ProjectKind::Canvas;
        let project_id = LibraryRepository::create_project(&mut connection, &project_input, 1)
            .expect("应能创建画布项目");
        let reference_id =
            LibraryRepository::create_asset(&mut connection, &asset(Some(project_id)), 2)
                .expect("应能创建参考图");
        let mut output = asset(Some(project_id));
        output.media.stored_path = "media/images/last-output.png".to_owned();
        output.media.file_name = "last-output.png".to_owned();
        let output_id =
            LibraryRepository::create_asset(&mut connection, &output, 2).expect("应能创建最终输出");
        LibraryRepository::set_canvas_member(
            &mut connection,
            project_id,
            reference_id,
            CanvasMemberRole::Reference,
            Some("ref"),
            3,
        )
        .expect("应能设置参考图");

        assert_eq!(
            LibraryRepository::remove_assets_from_project(
                &mut connection,
                project_id,
                &[output_id],
                4,
            ),
            Err(LibraryRepositoryError::Conflict)
        );
        assert_eq!(
            LibraryRepository::move_to_trash(&mut connection, TrashEntityType::Asset, output_id, 4,),
            Err(LibraryRepositoryError::Conflict)
        );
        let mut moved_output = output;
        moved_output.media.project_id = None;
        assert_eq!(
            LibraryRepository::update_asset(&mut connection, output_id, &moved_output, 4),
            Err(LibraryRepositoryError::Conflict)
        );
        assert!(LibraryRepository::get_asset(&connection, output_id).is_ok());
        assert_eq!(
            connection
                .query_row("SELECT count(*) FROM trash_entries", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("失败操作不得写入回收站"),
            0
        );
    }

    #[test]
    fn canvas_prompt_edit_uses_copy_on_write_and_records_only_field_names() {
        let mut connection = database();
        let mut project_input = project("共享提示词");
        project_input.kind = ProjectKind::Canvas;
        let project_id = LibraryRepository::create_project(&mut connection, &project_input, 1)
            .expect("应能创建画布项目");
        let first_id =
            LibraryRepository::create_asset(&mut connection, &asset(Some(project_id)), 2)
                .expect("应能创建第一件输出");
        let mut second = asset(Some(project_id));
        second.media.stored_path = "media/images/shared-second.png".to_owned();
        second.media.file_name = "shared-second.png".to_owned();
        let second_id = LibraryRepository::create_asset(&mut connection, &second, 2)
            .expect("应能创建第二件输出");
        let first_prompt_id: i64 = connection
            .query_row(
                "SELECT prompt_id FROM assets WHERE id=?1",
                [first_id],
                |row| row.get(0),
            )
            .expect("第一件作品应有提示词");
        let second_prompt_id: i64 = connection
            .query_row(
                "SELECT prompt_id FROM assets WHERE id=?1",
                [second_id],
                |row| row.get(0),
            )
            .expect("第二件作品应有提示词");
        connection
            .execute(
                "UPDATE prompts SET prompt_zh='共享原文' WHERE id=?1",
                [first_prompt_id],
            )
            .expect("应能准备共享提示词");
        connection
            .execute(
                "UPDATE assets SET prompt_id=?1 WHERE id=?2",
                params![first_prompt_id, second_id],
            )
            .expect("应能准备共享关系");
        connection
            .execute("DELETE FROM prompts WHERE id=?1", [second_prompt_id])
            .expect("应能清理不再使用的提示词");

        LibraryRepository::update_canvas_output_prompt(
            &mut connection,
            project_id,
            first_id,
            &PromptText {
                prompt_zh: "只修改第一件".to_owned(),
                ..PromptText::default()
            },
            3,
        )
        .expect("共享提示词应使用写时复制");
        let prompt_rows = connection
            .prepare(
                "SELECT asset.id,prompt.id,prompt.prompt_zh
                   FROM assets asset JOIN prompts prompt ON prompt.id=asset.prompt_id
                  WHERE asset.id IN (?1,?2) ORDER BY asset.id",
            )
            .expect("应能读取写时复制结果")
            .query_map(params![first_id, second_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .expect("应能遍历写时复制结果")
            .collect::<Result<Vec<_>, _>>()
            .expect("写时复制结果应有效");
        assert_ne!(prompt_rows[0].1, prompt_rows[1].1);
        assert_eq!(prompt_rows[0].2, "只修改第一件");
        assert_eq!(prompt_rows[1].2, "共享原文");
        assert_eq!(
            connection
                .query_row(
                    "SELECT changed_fields_json FROM edit_history
                      WHERE target_type='asset' AND target_id=?1 ORDER BY id DESC LIMIT 1",
                    [first_id],
                    |row| row.get::<_, String>(0),
                )
                .expect("应记录脱敏修改历史"),
            "[\"prompt_zh\"]"
        );
    }

    #[test]
    fn asset_restore_keeps_hidden_canvas_relation_after_switching_to_simple() {
        let mut connection = database();
        let mut project_input = project("隐藏关系恢复");
        project_input.kind = ProjectKind::Canvas;
        let project_id = LibraryRepository::create_project(&mut connection, &project_input, 1)
            .expect("应能创建画布项目");
        let reference_id =
            LibraryRepository::create_asset(&mut connection, &asset(Some(project_id)), 2)
                .expect("应能创建参考图");
        let mut output = asset(Some(project_id));
        output.media.stored_path = "media/images/hidden-output.png".to_owned();
        output.media.file_name = "hidden-output.png".to_owned();
        LibraryRepository::create_asset(&mut connection, &output, 2).expect("应能创建输出");
        LibraryRepository::set_canvas_member(
            &mut connection,
            project_id,
            reference_id,
            CanvasMemberRole::Reference,
            Some("hidden"),
            3,
        )
        .expect("应能设置参考图");
        let trash_id = LibraryRepository::move_to_trash(
            &mut connection,
            TrashEntityType::Asset,
            reference_id,
            4,
        )
        .expect("参考图应能移入回收站");
        let mut simple_project = project("隐藏关系恢复");
        simple_project.kind = ProjectKind::Simple;
        LibraryRepository::update_project(&mut connection, project_id, &simple_project, 5)
            .expect("应能切回普通项目");

        LibraryRepository::restore_trash(&mut connection, trash_id)
            .expect("simple 项目也应恢复隐藏画布关系");
        let restored =
            canvas_member(&connection, project_id, reference_id).expect("隐藏画布关系应被保留");
        assert_eq!(restored.role, CanvasMemberRole::Reference);
        assert_eq!(restored.reference_name.as_deref(), Some("hidden"));
    }

    #[test]
    fn fts_tracks_asset_fields_and_relation_changes() {
        let mut connection = database();
        let dimension = LibraryRepository::create_dimension(
            &mut connection,
            &DimensionInput {
                name: "主题".to_owned(),
                allows_multiple: true,
                ai_can_suggest_new: false,
                is_enabled: true,
            },
            1,
        )
        .expect("应能创建维度");
        let category = LibraryRepository::create_category(
            &mut connection,
            &CategoryInput {
                dimension_id: dimension,
                name: "星云".to_owned(),
                aliases: Vec::new(),
                description: String::new(),
                color: None,
                icon: None,
                is_enabled: true,
            },
            2,
        )
        .expect("应能创建分类");
        let tag = LibraryRepository::create_tag(&mut connection, "发光", 3).expect("应能创建标签");
        let mut project_input = project("project-title-marker");
        project_input.description = "project-description-marker".to_owned();
        project_input.notes = "project-notes-marker".to_owned();
        let project_id = LibraryRepository::create_project(&mut connection, &project_input, 4)
            .expect("应能创建所属项目");
        let mut input = asset(Some(project_id));
        input.media.file_name = "nebula-dragon.png".to_owned();
        input.media.stored_path = "media/images/nebula-dragon.png".to_owned();
        input.prompt.prompt_en = "luminous dragon".to_owned();
        input.notes = "private archive".to_owned();
        let asset_id =
            LibraryRepository::create_asset(&mut connection, &input, 5).expect("应能创建作品");
        LibraryRepository::set_asset_categories(&mut connection, asset_id, &[category], 6)
            .expect("应能设置分类");
        LibraryRepository::set_asset_tags(&mut connection, asset_id, &[tag], 7)
            .expect("应能设置标签");

        for keyword in [
            "nebula",
            "dragon",
            "private",
            "发光",
            "星云",
            "Model",
            "Platform",
            "project-title-marker",
            "project-description-marker",
            "project-notes-marker",
        ] {
            assert_eq!(fts_asset_ids(&connection, keyword), vec![asset_id]);
        }

        LibraryRepository::update_tag(&mut connection, tag, "极光", 8).expect("应能更新标签");
        assert!(fts_asset_ids(&connection, "发光").is_empty());
        assert_eq!(fts_asset_ids(&connection, "极光"), vec![asset_id]);

        let mut updated = input;
        updated.prompt.prompt_en = "solar phoenix".to_owned();
        updated.model = Some("Model B".to_owned());
        LibraryRepository::update_asset(&mut connection, asset_id, &updated, 9)
            .expect("应能更新作品");
        assert!(fts_asset_ids(&connection, "luminous").is_empty());
        assert_eq!(fts_asset_ids(&connection, "Model B"), vec![asset_id]);

        let mut updated_project = project_input;
        updated_project.title = "project-title-updated".to_owned();
        updated_project.description = "project-description-updated".to_owned();
        updated_project.notes = "project-notes-updated".to_owned();
        LibraryRepository::update_project(&mut connection, project_id, &updated_project, 10)
            .expect("应能更新所属项目");
        assert!(fts_asset_ids(&connection, "project-title-marker").is_empty());
        for keyword in [
            "project-title-updated",
            "project-description-updated",
            "project-notes-updated",
        ] {
            assert_eq!(fts_asset_ids(&connection, keyword), vec![asset_id]);
        }
    }

    #[test]
    fn asset_search_combines_filters_with_stable_keyset_and_indexed_plan() {
        let mut connection = database();
        let dimension = LibraryRepository::create_dimension(
            &mut connection,
            &DimensionInput {
                name: "风格".to_owned(),
                allows_multiple: true,
                ai_can_suggest_new: false,
                is_enabled: true,
            },
            1,
        )
        .expect("应能创建维度");
        let category = LibraryRepository::create_category(
            &mut connection,
            &CategoryInput {
                dimension_id: dimension,
                name: "赛博".to_owned(),
                aliases: Vec::new(),
                description: String::new(),
                color: None,
                icon: None,
                is_enabled: true,
            },
            2,
        )
        .expect("应能创建分类");
        let mut first = asset(None);
        first.media.stored_path = "media/images/filter-first.png".to_owned();
        first.media.file_name = "filter-first.png".to_owned();
        first.media.width = Some(160);
        first.media.height = Some(100);
        first.category_ids = vec![category];
        let first_id =
            LibraryRepository::create_asset(&mut connection, &first, 10).expect("应能创建首项");
        let mut second = first.clone();
        second.media.stored_path = "media/images/filter-second.png".to_owned();
        second.media.file_name = "filter-second.png".to_owned();
        let second_id =
            LibraryRepository::create_asset(&mut connection, &second, 20).expect("应能创建次项");
        let query = AssetListQuery {
            media_type: Some(MediaType::Image),
            model: Some("Model A".to_owned()),
            platform: Some("Platform A".to_owned()),
            category_ids: vec![category],
            rating: Some(5),
            is_favorite: Some(true),
            is_public: Some(true),
            created_after: Some(0),
            created_before: Some(30),
            min_aspect_ratio: Some(1.5),
            max_aspect_ratio: Some(1.7),
            limit: 1,
            ..AssetListQuery::default()
        };
        let first_page =
            LibraryRepository::list_assets(&connection, query.clone()).expect("组合筛选应成功");
        assert_eq!(first_page.items[0].id, second_id);
        let keyset_query = AssetListQuery {
            cursor: first_page.next_cursor,
            ..query
        };
        let second_page = LibraryRepository::list_assets(&connection, keyset_query.clone())
            .expect("keyset 下一页应成功");
        assert_eq!(second_page.items[0].id, first_id);

        let name_keyset_query = AssetListQuery {
            keyword: Some("filter".to_owned()),
            ..keyset_query
        };
        let (sql, values) = LibraryRepository::build_asset_list_sql(&name_keyset_query);
        assert!(sql.contains("a.file_name LIKE ?"));
        assert!(sql.contains("a.updated_at < ? OR (a.updated_at = ? AND a.id < ?)"));
        let mut statement = connection
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .expect("应能解释真实列表查询计划");
        let plan = statement
            .query_map(rusqlite::params_from_iter(values), |row| {
                row.get::<_, String>(3)
            })
            .expect("应能读取查询计划")
            .collect::<Result<Vec<_>, _>>()
            .expect("查询计划行应可读取")
            .join("\n");
        assert!(
            !plan.contains("asset_search"),
            "作品名搜索不应读取全文索引：{plan}"
        );
        let uses_stable_filter_index = [
            "idx_assets_search_filter_page",
            "idx_assets_model_page",
            "idx_assets_platform_page",
        ]
        .iter()
        .any(|index| plan.contains(index));
        assert!(
            uses_stable_filter_index && !plan.contains("SCAN a"),
            "真实组合筛选必须使用支持稳定分页的资产复合索引：{plan}"
        );
    }

    #[test]
    fn numbered_asset_page_returns_total_and_stable_page_slice() {
        let mut connection = database();
        for index in 0..26 {
            let mut input = asset(None);
            input.media.stored_path = format!("media/images/page-{index}.png");
            input.media.file_name = format!("page-{index}.png");
            LibraryRepository::create_asset(&mut connection, &input, i64::from(index + 1))
                .expect("应能创建数字分页测试作品");
        }

        let page =
            LibraryRepository::list_assets_numbered(&connection, AssetListQuery::default(), 2, 10)
                .expect("应能读取数字分页");

        assert_eq!(page.page, 2);
        assert_eq!(page.page_size, 10);
        assert_eq!(page.total_count, 26);
        assert_eq!(page.total_pages, 3);
        assert_eq!(page.items.len(), 10);
        assert_eq!(page.items[0].media.file_name, "page-10.png");
        assert_eq!(page.items[9].media.file_name, "page-19.png");
    }

    #[test]
    fn newly_imported_images_stay_before_videos_and_new_videos_append() {
        let mut connection = database();
        let mut create = |name: &str, media_type: MediaType, now: i64| {
            let mut input = asset(None);
            input.media.media_type = media_type;
            input.media.stored_path = format!("media/{name}");
            input.media.file_name = name.to_owned();
            LibraryRepository::create_asset(&mut connection, &input, now)
                .expect("应能创建导入排序测试作品")
        };
        create("image-1.png", MediaType::Image, 1);
        create("video-1.mp4", MediaType::Video, 2);
        create("video-2.mp4", MediaType::Video, 3);
        create("image-2.png", MediaType::Image, 4);
        create("video-3.mp4", MediaType::Video, 5);

        let page =
            LibraryRepository::list_assets_numbered(&connection, AssetListQuery::default(), 1, 10)
                .expect("应能读取导入后的展示编号");
        assert_eq!(
            page.items
                .iter()
                .map(|item| (item.display_order, item.media.file_name.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (1, "image-1.png"),
                (2, "image-2.png"),
                (3, "video-1.mp4"),
                (4, "video-2.mp4"),
                (5, "video-3.mp4"),
            ]
        );
    }

    #[test]
    fn asset_name_search_is_case_insensitive_and_supports_exact_mode() {
        let mut connection = database();
        for (index, name) in ["SunSet.PNG", "sunny.png"].into_iter().enumerate() {
            let mut input = asset(None);
            input.media.stored_path = format!("media/images/{index}-{name}");
            input.media.file_name = name.to_owned();
            LibraryRepository::create_asset(&mut connection, &input, index as i64 + 1)
                .expect("应能创建搜索测试作品");
        }

        let fuzzy = LibraryRepository::list_assets_numbered(
            &connection,
            AssetListQuery {
                keyword: Some("SET".to_owned()),
                ..AssetListQuery::default()
            },
            1,
            10,
        )
        .expect("模糊文件名搜索应成功");
        assert_eq!(fuzzy.items.len(), 1);
        assert_eq!(fuzzy.items[0].media.file_name, "SunSet.PNG");

        let exact = LibraryRepository::list_assets_numbered(
            &connection,
            AssetListQuery {
                keyword: Some("sunset.png".to_owned()),
                exact_match: true,
                ..AssetListQuery::default()
            },
            1,
            10,
        )
        .expect("精确文件名搜索应成功");
        assert_eq!(exact.items.len(), 1);

        let exact_without_extension = LibraryRepository::list_assets_numbered(
            &connection,
            AssetListQuery {
                keyword: Some("sunset".to_owned()),
                exact_match: true,
                ..AssetListQuery::default()
            },
            1,
            10,
        )
        .expect("精确搜索无结果也应成功");
        assert!(exact_without_extension.items.is_empty());
    }

    #[test]
    fn prompt_and_notes_search_use_their_own_full_text_fields() {
        let mut connection = database();
        let mut prompt_asset = asset(None);
        prompt_asset.media.file_name = "prompt-result.png".to_owned();
        prompt_asset.media.stored_path = "media/images/prompt-result.png".to_owned();
        prompt_asset.prompt.prompt_zh = "描绘旋转星云".to_owned();
        let prompt_asset_id = LibraryRepository::create_asset(&mut connection, &prompt_asset, 1)
            .expect("应能创建提示词搜索作品");

        let mut notes_asset = asset(None);
        notes_asset.media.file_name = "notes-result.png".to_owned();
        notes_asset.media.stored_path = "media/images/notes-result.png".to_owned();
        notes_asset.notes = "雕刻风格参考图".to_owned();
        let notes_asset_id = LibraryRepository::create_asset(&mut connection, &notes_asset, 2)
            .expect("应能创建备注搜索作品");

        let prompt_results = LibraryRepository::list_assets(
            &connection,
            AssetListQuery {
                keyword: Some("旋转".to_owned()),
                search_field: AssetSearchField::Prompt,
                limit: 10,
                ..AssetListQuery::default()
            },
        )
        .expect("提示词全文检索应成功");
        assert_eq!(
            prompt_results
                .items
                .iter()
                .map(|asset| asset.id)
                .collect::<Vec<_>>(),
            vec![prompt_asset_id]
        );

        let notes_results = LibraryRepository::list_assets(
            &connection,
            AssetListQuery {
                keyword: Some("风格".to_owned()),
                search_field: AssetSearchField::Notes,
                limit: 10,
                ..AssetListQuery::default()
            },
        )
        .expect("备注全文检索应成功");
        assert_eq!(
            notes_results
                .items
                .iter()
                .map(|asset| asset.id)
                .collect::<Vec<_>>(),
            vec![notes_asset_id]
        );
    }

    #[test]
    fn asset_display_order_is_stable_and_supports_swap_and_shift() {
        let mut connection = database();
        let mut ids = Vec::new();
        for (index, media_type) in [MediaType::Image, MediaType::Image, MediaType::Video]
            .into_iter()
            .enumerate()
        {
            let mut input = asset(None);
            input.media.media_type = media_type;
            input.media.stored_path = format!("media/item-{index}");
            input.media.file_name = format!("item-{index}");
            ids.push(
                LibraryRepository::create_asset(
                    &mut connection,
                    &input,
                    i64::from(index as i32 + 1),
                )
                .expect("应能创建作品"),
            );
        }

        LibraryRepository::update_asset_display_order(
            &mut connection,
            ids[2],
            1,
            AssetOrderChangeMode::Swap,
        )
        .expect("应能交换编号");
        let after_swap =
            LibraryRepository::list_assets_numbered(&connection, AssetListQuery::default(), 1, 10)
                .expect("应能读取交换后的编号");
        assert_eq!(
            after_swap
                .items
                .iter()
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            vec![ids[2], ids[1], ids[0]]
        );

        LibraryRepository::update_asset_display_order(
            &mut connection,
            ids[0],
            1,
            AssetOrderChangeMode::ShiftFollowing,
        )
        .expect("应能顺延编号");
        let after_shift =
            LibraryRepository::list_assets_numbered(&connection, AssetListQuery::default(), 1, 10)
                .expect("应能读取顺延后的编号");
        assert_eq!(
            after_shift
                .items
                .iter()
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            vec![ids[0], ids[2], ids[1]]
        );
        assert_eq!(
            after_shift
                .items
                .iter()
                .map(|item| item.display_order)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn numbered_asset_page_count_and_items_share_combined_filters() {
        let mut connection = database();
        for index in 0..12 {
            let mut input = asset(None);
            input.media.stored_path = format!("media/images/night-{index}.png");
            input.media.file_name = format!("night-{index}.png");
            input.rating = if index % 2 == 0 { 4 } else { 2 };
            input.is_favorite = index % 2 == 0;
            LibraryRepository::create_asset(&mut connection, &input, i64::from(index + 1))
                .expect("应能创建组合筛选测试作品");
        }

        let page = LibraryRepository::list_assets_numbered(
            &connection,
            AssetListQuery {
                keyword: Some("night".to_owned()),
                media_type: Some(MediaType::Image),
                rating: Some(4),
                is_favorite: Some(true),
                ..AssetListQuery::default()
            },
            1,
            10,
        )
        .expect("数字分页应复用组合筛选");

        assert_eq!(page.total_count, 6);
        assert_eq!(page.total_pages, 1);
        assert_eq!(page.items.len(), 6);
        assert!(
            page.items
                .iter()
                .all(|item| item.rating == 4 && item.is_favorite)
        );
    }

    #[test]
    fn duplicate_groups_are_paginated_and_never_expose_stored_paths() {
        let mut connection = database();
        let mut created_ids = Vec::new();
        for (index, hash, now) in [
            (1, "hash-a", 10),
            (2, "hash-a", 20),
            (3, "hash-b", 30),
            (4, "hash-b", 40),
        ] {
            let mut input = asset(None);
            input.media.stored_path = format!("media/images/duplicate-{index}.png");
            input.media.file_name = format!("duplicate-{index}.png");
            input.media.content_hash = Some(hash.to_owned());
            if index == 4 {
                input.media.media_type = MediaType::Video;
                input.media.stored_path = "media/videos/duplicate-4.mp4".to_owned();
                input.media.file_name = "duplicate-4.mp4".to_owned();
                input.media.mime_type = Some("video/mp4".to_owned());
            }
            created_ids.push(
                LibraryRepository::create_asset(&mut connection, &input, now)
                    .expect("应能创建重复项"),
            );
        }
        for index in 0..2 {
            let mut input = asset(None);
            input.media.stored_path = format!("media/images/no-hash-{index}.png");
            input.media.file_name = format!("no-hash-{index}.png");
            input.media.content_hash = None;
            LibraryRepository::create_asset(&mut connection, &input, 50 + index)
                .expect("应能创建无哈希作品");
        }
        let first_page = LibraryRepository::list_duplicate_groups(&connection, None, 1)
            .expect("应能按组分页查询重复项");
        assert_eq!(first_page.items[0].content_hash, "hash-b");
        assert_eq!(first_page.items[0].asset_count, 2);
        let second_page =
            LibraryRepository::list_duplicate_groups(&connection, first_page.next_cursor, 1)
                .expect("重复组下一页应成功");
        assert_eq!(second_page.items[0].content_hash, "hash-a");
        let json = serde_json::to_string(&second_page.items[0]).expect("重复组应可序列化");
        assert!(!json.contains("storedPath"));
        assert!(!json.contains("media/images"));

        let plan = connection
            .prepare(
                "EXPLAIN QUERY PLAN SELECT content_hash,count(*) FROM assets
                 WHERE content_hash IS NOT NULL GROUP BY content_hash HAVING count(*) > 1",
            )
            .expect("应能解释重复检测查询")
            .query_map([], |row| row.get::<_, String>(3))
            .expect("应能读取重复检测查询计划")
            .collect::<Result<Vec<_>, _>>()
            .expect("查询计划应可读取")
            .join("\n");
        assert!(
            plan.contains("idx_assets_hash"),
            "精确重复检测必须使用哈希索引：{plan}"
        );

        LibraryRepository::move_to_trash(
            &mut connection,
            TrashEntityType::Asset,
            created_ids[3],
            60,
        )
        .expect("应能把视频重复项移入回收站");
        let after_trash = LibraryRepository::list_duplicate_groups(&connection, None, 10)
            .expect("回收站变更后应能重新读取重复组");
        assert_eq!(after_trash.items.len(), 1);
        assert_eq!(after_trash.items[0].content_hash, "hash-a");
    }

    #[test]
    #[ignore = "发布构建重复检测基准，仅手动运行"]
    fn benchmark_duplicate_groups_by_record_count() {
        use std::time::Instant;

        for asset_count in [100_i64, 1_000, 10_000] {
            let mut connection = database();
            let transaction = connection.transaction().expect("应能开启基准事务");
            for id in 0..asset_count {
                transaction
                    .execute(
                        "INSERT INTO assets(media_type,path_kind,stored_path,file_name,content_hash,created_at,updated_at)
                         VALUES('image','managed',?1,?2,?3,1,?4)",
                        params![
                            format!("media/images/duplicate-benchmark-{id}.png"),
                            format!("duplicate-benchmark-{id}.png"),
                            format!("hash-{}", id / 2),
                            id + 1,
                        ],
                    )
                    .expect("应能写入重复检测基准数据");
            }
            transaction.commit().expect("应能提交基准数据");
            let started = Instant::now();
            let page = LibraryRepository::list_duplicate_groups(&connection, None, 50)
                .expect("应能查询重复组首页");
            let elapsed = started.elapsed();
            assert_eq!(page.items.len(), 50.min((asset_count / 2) as usize));
            println!(
                "duplicate_group_benchmark asset_count={asset_count} elapsed_ms={:.3}",
                elapsed.as_secs_f64() * 1_000.0
            );
        }
    }

    #[test]
    fn bulk_asset_edit_enforces_raw_limit_and_deduplicates_targets() {
        let mut connection = database();
        let asset_id = LibraryRepository::create_asset(&mut connection, &asset(None), 1)
            .expect("应能创建作品");
        let input = BulkAssetEditInput {
            asset_ids: vec![asset_id; 100],
            rating: Some(3),
            ..BulkAssetEditInput::default()
        };

        let preview = LibraryRepository::preview_bulk_asset_edit(&connection, &input)
            .expect("不超过上限的重复目标应能预览");
        assert_eq!(preview.target_count, 1, "重复 ID 只能计为一个目标");
        LibraryRepository::bulk_edit_assets(&mut connection, &input, 2)
            .expect("重复目标应只更新一次");
        assert_eq!(
            LibraryRepository::get_asset(&connection, asset_id)
                .expect("作品应存在")
                .summary
                .rating,
            3
        );

        let over_limit = BulkAssetEditInput {
            asset_ids: vec![asset_id; 101],
            rating: Some(2),
            ..BulkAssetEditInput::default()
        };
        assert_eq!(
            LibraryRepository::preview_bulk_asset_edit(&connection, &over_limit),
            Err(LibraryRepositoryError::InvalidData),
            "输入项即使重复也不得绕过 100 项边界"
        );
    }

    #[test]
    fn metadata_presets_support_create_rename_and_delete() {
        let mut connection = database();
        let defaults =
            LibraryRepository::list_metadata_presets(&connection).expect("应能读取默认元数据预设");
        assert!(
            defaults
                .models
                .iter()
                .any(|item| item.name == "Seedance 2.0")
        );
        assert!(defaults.platforms.iter().any(|item| item.name == "豆包"));

        let model_id = LibraryRepository::create_metadata_preset(
            &mut connection,
            MetadataPresetKind::Model,
            " 自定义模型 ",
            10,
        )
        .expect("应能创建模型预设");
        LibraryRepository::update_metadata_preset(
            &mut connection,
            MetadataPresetKind::Model,
            model_id,
            "自定义模型 2",
            11,
        )
        .expect("应能重命名模型预设");

        let renamed = LibraryRepository::list_metadata_presets(&connection)
            .expect("应能读取重命名后的元数据预设");
        assert!(renamed.models.iter().any(|item| {
            item.id == model_id && item.name == "自定义模型 2" && item.asset_count == 0
        }));

        LibraryRepository::delete_metadata_preset(
            &mut connection,
            MetadataPresetKind::Model,
            model_id,
        )
        .expect("应能删除模型预设");
        let deleted = LibraryRepository::list_metadata_presets(&connection)
            .expect("应能读取删除后的元数据预设");
        assert!(!deleted.models.iter().any(|item| item.id == model_id));
    }

    #[test]
    fn project_assignment_uses_visible_numbers_and_rolls_back_invalid_batches() {
        let mut connection = database();
        let project_id =
            LibraryRepository::create_project(&mut connection, &project("批量归入项目"), 1)
                .expect("应能创建目标项目");
        let first_id = LibraryRepository::create_asset(&mut connection, &asset(None), 2)
            .expect("应能创建第一件作品");
        let mut second = asset(None);
        second.media.stored_path = "media/images/project-second.png".to_owned();
        second.media.file_name = "project-second.png".to_owned();
        let second_id = LibraryRepository::create_asset(&mut connection, &second, 3)
            .expect("应能创建第二件作品");
        let mut third = asset(None);
        third.media.stored_path = "media/images/project-third.png".to_owned();
        third.media.file_name = "project-third.png".to_owned();
        let third_id = LibraryRepository::create_asset(&mut connection, &third, 4)
            .expect("应能创建第三件作品");

        let assigned = LibraryRepository::assign_assets_to_project_by_display_numbers(
            &mut connection,
            project_id,
            &[1, 3],
            5,
        )
        .expect("应按可见编号批量归入项目");
        assert_eq!(assigned, 2);
        assert_eq!(
            LibraryRepository::get_asset(&connection, first_id)
                .expect("第一件作品应存在")
                .summary
                .media
                .project_id,
            Some(project_id)
        );
        assert_eq!(
            LibraryRepository::get_asset(&connection, third_id)
                .expect("第三件作品应存在")
                .summary
                .media
                .project_id,
            Some(project_id)
        );

        let error = LibraryRepository::assign_assets_to_project_by_display_numbers(
            &mut connection,
            project_id,
            &[2, 99],
            6,
        );
        assert_eq!(error, Err(LibraryRepositoryError::NotFound));
        assert_eq!(
            LibraryRepository::get_asset(&connection, second_id)
                .expect("无效批次后第二件作品仍应存在")
                .summary
                .media
                .project_id,
            None,
            "批次内任一编号无效时不得归入任何作品"
        );

        let removed = LibraryRepository::remove_assets_from_project(
            &mut connection,
            project_id,
            &[first_id],
            7,
        )
        .expect("应能从项目移出指定作品");
        assert_eq!(removed, 1);
        assert_eq!(
            LibraryRepository::get_asset(&connection, first_id)
                .expect("移出后作品仍应存在")
                .summary
                .media
                .project_id,
            None,
            "移出项目只解除关系，不删除作品"
        );
        assert_eq!(
            LibraryRepository::get_asset(&connection, third_id)
                .expect("未选择作品仍应存在")
                .summary
                .media
                .project_id,
            Some(project_id),
            "未选择的项目作品不得受影响"
        );
    }

    #[test]
    fn bulk_asset_edit_uses_visible_display_numbers_instead_of_internal_ids() {
        let mut connection = database();
        let first_id = LibraryRepository::create_asset(&mut connection, &asset(None), 1)
            .expect("应能创建第一件作品");
        let mut second = asset(None);
        second.media.stored_path = "media/images/display-number-second.png".to_owned();
        second.media.file_name = "display-number-second.png".to_owned();
        let second_id = LibraryRepository::create_asset(&mut connection, &second, 2)
            .expect("应能创建第二件作品");
        LibraryRepository::move_to_trash(&mut connection, TrashEntityType::Asset, first_id, 3)
            .expect("第一件作品应能进入回收站并压缩展示编号");

        let input = BulkAssetEditInput {
            asset_ids: vec![1],
            rating: Some(4),
            ..BulkAssetEditInput::default()
        };
        LibraryRepository::bulk_edit_assets(&mut connection, &input, 4)
            .expect("展示编号 1 应指向压缩后的第二件作品");

        assert_eq!(
            LibraryRepository::get_asset(&connection, second_id)
                .expect("第二件作品应存在")
                .summary
                .rating,
            4
        );
    }

    #[test]
    fn bulk_preview_counts_only_effective_category_and_tag_changes() {
        let mut connection = database();
        let dimension = LibraryRepository::create_dimension(
            &mut connection,
            &DimensionInput {
                name: "批量分类维度".to_owned(),
                allows_multiple: true,
                ai_can_suggest_new: false,
                is_enabled: true,
            },
            1,
        )
        .expect("应能创建维度");
        let category_to_add = LibraryRepository::create_category(
            &mut connection,
            &CategoryInput {
                dimension_id: dimension,
                name: "待添加分类".to_owned(),
                aliases: Vec::new(),
                description: String::new(),
                color: None,
                icon: None,
                is_enabled: true,
            },
            2,
        )
        .expect("应能创建待添加分类");
        let category_to_remove = LibraryRepository::create_category(
            &mut connection,
            &CategoryInput {
                dimension_id: dimension,
                name: "待移除分类".to_owned(),
                aliases: Vec::new(),
                description: String::new(),
                color: None,
                icon: None,
                is_enabled: true,
            },
            3,
        )
        .expect("应能创建待移除分类");
        let tag_to_add = LibraryRepository::create_tag(&mut connection, "待添加标签", 4)
            .expect("应能创建待添加标签");
        let tag_to_remove = LibraryRepository::create_tag(&mut connection, "待移除标签", 5)
            .expect("应能创建待移除标签");

        let mut first = asset(None);
        first.category_ids = vec![category_to_remove];
        first.tag_ids = vec![tag_to_remove];
        let first_id = LibraryRepository::create_asset(&mut connection, &first, 10)
            .expect("应能创建第一件作品");
        let mut second = asset(None);
        second.media.stored_path = "media/images/bulk-second.png".to_owned();
        second.media.file_name = "bulk-second.png".to_owned();
        second.category_ids = vec![category_to_add];
        second.tag_ids = vec![tag_to_add];
        let second_id = LibraryRepository::create_asset(&mut connection, &second, 11)
            .expect("应能创建第二件作品");
        let input = BulkAssetEditInput {
            asset_ids: vec![first_id, second_id, first_id],
            add_category_ids: vec![category_to_add, category_to_add],
            remove_category_ids: vec![category_to_remove, category_to_remove],
            add_tag_ids: vec![tag_to_add, tag_to_add],
            remove_tag_ids: vec![tag_to_remove, tag_to_remove],
            ..BulkAssetEditInput::default()
        };

        let preview = LibraryRepository::preview_bulk_asset_edit(&connection, &input)
            .expect("应能计算实际影响");
        assert_eq!(preview.target_count, 2);
        assert_eq!(preview.category_relations_to_add, 1);
        assert_eq!(preview.category_relations_to_remove, 1);
        assert_eq!(preview.tag_relations_to_add, 1);
        assert_eq!(preview.tag_relations_to_remove, 1);

        let applied = LibraryRepository::bulk_edit_assets(&mut connection, &input, 20)
            .expect("应能批量应用分类和标签");
        assert_eq!(applied, preview, "执行返回的影响应与预览一致");
        for asset_id in [first_id, second_id] {
            let detail =
                LibraryRepository::get_asset(&connection, asset_id).expect("批量编辑后作品应存在");
            assert_eq!(detail.category_ids, vec![category_to_add]);
            assert_eq!(detail.tag_ids, vec![tag_to_add]);
        }
    }

    #[test]
    fn bulk_asset_edit_rejects_trash_target_without_touching_live_targets() {
        let mut connection = database();
        let live_id = LibraryRepository::create_asset(&mut connection, &asset(None), 1)
            .expect("应能创建正常作品");
        let mut trashed = asset(None);
        trashed.media.stored_path = "media/images/to-trash.png".to_owned();
        trashed.media.file_name = "to-trash.png".to_owned();
        let trashed_id = LibraryRepository::create_asset(&mut connection, &trashed, 2)
            .expect("应能创建待回收作品");
        LibraryRepository::move_to_trash(&mut connection, TrashEntityType::Asset, trashed_id, 3)
            .expect("应能移入回收站");
        let input = BulkAssetEditInput {
            asset_ids: vec![live_id, trashed_id],
            rating: Some(1),
            ..BulkAssetEditInput::default()
        };

        assert_eq!(
            LibraryRepository::preview_bulk_asset_edit(&connection, &input),
            Err(LibraryRepositoryError::NotFound)
        );
        assert_eq!(
            LibraryRepository::bulk_edit_assets(&mut connection, &input, 4),
            Err(LibraryRepositoryError::NotFound)
        );
        assert_eq!(
            LibraryRepository::get_asset(&connection, live_id)
                .expect("正常作品不得受影响")
                .summary
                .rating,
            5
        );
    }

    #[test]
    fn bulk_asset_edit_rolls_back_all_fields_when_any_category_conflicts() {
        let mut connection = database();
        let dimension = LibraryRepository::create_dimension(
            &mut connection,
            &DimensionInput {
                name: "单选维度".to_owned(),
                allows_multiple: false,
                ai_can_suggest_new: false,
                is_enabled: true,
            },
            1,
        )
        .expect("应能创建单选维度");
        let existing = LibraryRepository::create_category(
            &mut connection,
            &CategoryInput {
                dimension_id: dimension,
                name: "已有分类".to_owned(),
                aliases: Vec::new(),
                description: String::new(),
                color: None,
                icon: None,
                is_enabled: true,
            },
            2,
        )
        .expect("应能创建已有分类");
        let conflicting = LibraryRepository::create_category(
            &mut connection,
            &CategoryInput {
                dimension_id: dimension,
                name: "冲突分类".to_owned(),
                aliases: Vec::new(),
                description: String::new(),
                color: None,
                icon: None,
                is_enabled: true,
            },
            3,
        )
        .expect("应能创建冲突分类");
        let first_id = LibraryRepository::create_asset(&mut connection, &asset(None), 10)
            .expect("应能创建第一件作品");
        let mut second = asset(None);
        second.media.stored_path = "media/images/conflict-second.png".to_owned();
        second.media.file_name = "conflict-second.png".to_owned();
        second.category_ids = vec![existing];
        let second_id = LibraryRepository::create_asset(&mut connection, &second, 11)
            .expect("应能创建带已有分类的作品");
        let input = BulkAssetEditInput {
            asset_ids: vec![first_id, second_id],
            rating: Some(1),
            model: BulkNullableTextEdit::Set("不应残留的模型".to_owned()),
            add_category_ids: vec![conflicting],
            ..BulkAssetEditInput::default()
        };

        assert_eq!(
            LibraryRepository::bulk_edit_assets(&mut connection, &input, 20),
            Err(LibraryRepositoryError::Conflict)
        );
        for asset_id in [first_id, second_id] {
            let detail =
                LibraryRepository::get_asset(&connection, asset_id).expect("冲突后作品应保持存在");
            assert_eq!(detail.summary.rating, 5, "任一冲突必须回滚整批字段");
            assert_eq!(detail.summary.model.as_deref(), Some("Model A"));
        }
        let leaked_model_count: i64 = connection
            .query_row(
                "SELECT count(*) FROM models WHERE name='不应残留的模型'",
                [],
                |row| row.get(0),
            )
            .expect("应能检查事务残留");
        assert_eq!(leaked_model_count, 0, "冲突事务不得残留命名实体");
        assert!(
            LibraryRepository::get_asset(&connection, first_id)
                .expect("第一件作品应存在")
                .category_ids
                .is_empty()
        );
        assert_eq!(
            LibraryRepository::get_asset(&connection, second_id)
                .expect("第二件作品应存在")
                .category_ids,
            vec![existing]
        );
    }

    #[test]
    fn bulk_asset_edit_can_clear_then_set_model_and_platform() {
        let mut connection = database();
        let first_id = LibraryRepository::create_asset(&mut connection, &asset(None), 1)
            .expect("应能创建第一件作品");
        let mut second = asset(None);
        second.media.stored_path = "media/images/named-second.png".to_owned();
        second.media.file_name = "named-second.png".to_owned();
        let second_id = LibraryRepository::create_asset(&mut connection, &second, 2)
            .expect("应能创建第二件作品");
        let clear = BulkAssetEditInput {
            asset_ids: vec![first_id, second_id],
            model: BulkNullableTextEdit::Clear,
            platform: BulkNullableTextEdit::Clear,
            ..BulkAssetEditInput::default()
        };
        LibraryRepository::bulk_edit_assets(&mut connection, &clear, 3)
            .expect("应能清空模型与平台");
        for asset_id in [first_id, second_id] {
            let summary = LibraryRepository::get_asset(&connection, asset_id)
                .expect("作品应存在")
                .summary;
            assert_eq!(summary.model, None);
            assert_eq!(summary.platform, None);
        }

        let set = BulkAssetEditInput {
            asset_ids: vec![first_id, second_id],
            model: BulkNullableTextEdit::Set("  Model B  ".to_owned()),
            platform: BulkNullableTextEdit::Set("  Platform B  ".to_owned()),
            ..BulkAssetEditInput::default()
        };
        LibraryRepository::bulk_edit_assets(&mut connection, &set, 4).expect("应能设置模型与平台");
        for asset_id in [first_id, second_id] {
            let summary = LibraryRepository::get_asset(&connection, asset_id)
                .expect("作品应存在")
                .summary;
            assert_eq!(summary.model.as_deref(), Some("Model B"));
            assert_eq!(summary.platform.as_deref(), Some("Platform B"));
            let history: Vec<String> = connection.prepare(
                "SELECT changed_fields_json FROM edit_history WHERE target_type='asset' AND target_id=?1 AND action='bulk_update' ORDER BY id",
            ).expect("应能准备批量历史查询").query_map([asset_id], |row| row.get(0))
                .expect("应能读取批量历史").collect::<Result<Vec<_>, _>>()
                .expect("批量历史行应有效");
            assert_eq!(history.len(), 2);
            assert!(
                history
                    .iter()
                    .all(|fields| fields.contains("model") && fields.contains("platform"))
            );
        }
    }

    #[test]
    fn model_comparison_paginates_project_and_exact_prompt_scopes() {
        let mut connection = database();
        let project_id =
            LibraryRepository::create_project(&mut connection, &project("对比项目"), 1)
                .expect("应能创建项目");
        let shared_prompt = PromptText {
            prompt_zh: "同一中文提示词".to_owned(),
            prompt_en: "same prompt".to_owned(),
            negative_prompt: "same negative".to_owned(),
        };
        let mut first = asset(Some(project_id));
        first.prompt = shared_prompt.clone();
        let first_id = LibraryRepository::create_asset(&mut connection, &first, 10)
            .expect("应能创建项目内第一件作品");
        let mut second = asset(Some(project_id));
        second.media.stored_path = "media/images/compare-second.png".to_owned();
        second.media.file_name = "compare-second.png".to_owned();
        second.prompt = shared_prompt.clone();
        let second_id = LibraryRepository::create_asset(&mut connection, &second, 20)
            .expect("应能创建项目内第二件作品");
        let mut cross_project = asset(None);
        cross_project.media.stored_path = "media/images/compare-cross.png".to_owned();
        cross_project.media.file_name = "compare-cross.png".to_owned();
        cross_project.prompt = shared_prompt;
        let cross_project_id = LibraryRepository::create_asset(&mut connection, &cross_project, 30)
            .expect("应能创建跨项目同提示词作品");
        let mut different = asset(None);
        different.media.stored_path = "media/images/compare-different.png".to_owned();
        different.media.file_name = "compare-different.png".to_owned();
        different.prompt = PromptText {
            prompt_zh: "不同提示词".to_owned(),
            prompt_en: "different".to_owned(),
            negative_prompt: String::new(),
        };
        LibraryRepository::create_asset(&mut connection, &different, 40)
            .expect("应能创建不同提示词作品");

        let project_page = LibraryRepository::list_model_comparison(
            &connection,
            ModelComparisonQuery {
                scope: ModelComparisonScope::Project { project_id },
                cursor: None,
                limit: 1,
            },
        )
        .expect("应能读取项目对比第一页");
        assert_eq!(project_page.items[0].id, second_id);
        let project_next = LibraryRepository::list_model_comparison(
            &connection,
            ModelComparisonQuery {
                scope: ModelComparisonScope::Project { project_id },
                cursor: project_page.next_cursor,
                limit: 1,
            },
        )
        .expect("应能读取项目对比下一页");
        assert_eq!(project_next.items[0].id, first_id);
        assert!(project_next.next_cursor.is_none());

        let prompt_page = LibraryRepository::list_model_comparison(
            &connection,
            ModelComparisonQuery {
                scope: ModelComparisonScope::MatchingPrompt {
                    baseline_asset_id: first_id,
                },
                cursor: None,
                limit: 2,
            },
        )
        .expect("应能读取同提示词第一页");
        assert_eq!(
            prompt_page
                .items
                .iter()
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            vec![cross_project_id, second_id]
        );
        let prompt_next = LibraryRepository::list_model_comparison(
            &connection,
            ModelComparisonQuery {
                scope: ModelComparisonScope::MatchingPrompt {
                    baseline_asset_id: first_id,
                },
                cursor: prompt_page.next_cursor,
                limit: 2,
            },
        )
        .expect("应能读取同提示词下一页");
        assert_eq!(
            prompt_next
                .items
                .iter()
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            vec![first_id]
        );
        assert!(prompt_next.next_cursor.is_none());
    }

    #[test]
    fn model_comparison_query_plans_use_page_indexes_without_temp_sort_or_correlated_queries() {
        let connection = database();
        for filter in [
            ModelComparisonFilter::Project(1),
            ModelComparisonFilter::MatchingPrompt(PromptText {
                prompt_zh: "中文".to_owned(),
                prompt_en: "prompt".to_owned(),
                negative_prompt: "negative".to_owned(),
            }),
        ] {
            let (sql, values) = LibraryRepository::build_model_comparison_sql(&filter, None, 24);
            assert!(sql.contains("LEFT JOIN prompts"));
            assert!(sql.contains("LEFT JOIN models"));
            assert!(sql.contains("LEFT JOIN platforms"));
            assert!(!sql.contains("(SELECT"), "对比列表不得引入逐项相关子查询");
            let plan = connection
                .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
                .expect("应能解释对比查询计划")
                .query_map(rusqlite::params_from_iter(values), |row| {
                    row.get::<_, String>(3)
                })
                .expect("应能读取查询计划")
                .collect::<Result<Vec<_>, _>>()
                .expect("查询计划行应可读取")
                .join("\n");
            assert!(
                !plan.contains("USE TEMP B-TREE"),
                "对比分页不得使用临时排序：{plan}"
            );
            assert!(
                !plan.contains("CORRELATED"),
                "对比查询不得产生逐项相关查询：{plan}"
            );
            match filter {
                ModelComparisonFilter::Project(_) => assert!(
                    plan.contains("idx_assets_project_page"),
                    "项目对比应使用项目分页索引：{plan}"
                ),
                ModelComparisonFilter::MatchingPrompt(_) => assert!(
                    plan.contains("idx_assets_updated_page"),
                    "同提示词对比应沿稳定分页索引扫描：{plan}"
                ),
            }
        }
    }

    #[test]
    #[ignore = "发布构建分页查询基准，仅手动运行"]
    fn benchmark_asset_first_page_by_record_count() {
        use std::{hint::black_box, time::Instant};

        let mut connection = database();
        let mut created = 0_i64;
        for target_count in [100_i64, 1_000, 10_000] {
            let transaction = connection.transaction().expect("应能开始基准事务");
            {
                let mut insert = transaction
                    .prepare(
                        "INSERT INTO assets
                         (media_type,path_kind,stored_path,file_name,created_at,updated_at)
                         VALUES('image','managed',?1,?2,?3,?3)",
                    )
                    .expect("应能准备基准写入");
                for id in (created + 1)..=target_count {
                    let path = format!("media/images/benchmark-{id}.png");
                    insert
                        .execute(rusqlite::params![path, format!("benchmark-{id}.png"), id])
                        .expect("应能写入基准作品");
                }
            }
            transaction.commit().expect("应能提交基准数据");
            created = target_count;

            let query = AssetListQuery {
                project_id: None,
                media_type: None,
                cursor: None,
                limit: 24,
                ..AssetListQuery::default()
            };
            for _ in 0..20 {
                black_box(
                    LibraryRepository::list_assets(&connection, query.clone())
                        .expect("基准预热查询应成功"),
                );
            }
            let iterations = 500_u32;
            let started = Instant::now();
            for _ in 0..iterations {
                black_box(
                    LibraryRepository::list_assets(&connection, query.clone())
                        .expect("基准分页查询应成功"),
                );
            }
            let elapsed = started.elapsed();
            println!(
                "asset_page_benchmark record_count={target_count} page_size=24 iterations={iterations} average_ms={:.4}",
                elapsed.as_secs_f64() * 1_000.0 / f64::from(iterations)
            );

            for _ in 0..20 {
                black_box(
                    LibraryRepository::list_assets_numbered(&connection, query.clone(), 1, 25)
                        .expect("数字分页基准预热查询应成功"),
                );
            }
            let started = Instant::now();
            for _ in 0..iterations {
                black_box(
                    LibraryRepository::list_assets_numbered(&connection, query.clone(), 1, 25)
                        .expect("数字分页基准查询应成功"),
                );
            }
            let elapsed = started.elapsed();
            println!(
                "numbered_asset_page_benchmark record_count={target_count} page_size=25 iterations={iterations} average_ms={:.4}",
                elapsed.as_secs_f64() * 1_000.0 / f64::from(iterations)
            );
        }
    }

    #[test]
    #[ignore = "发布构建 FTS/组合筛选基准，仅手动运行"]
    fn benchmark_asset_search_by_record_count_under_500ms() {
        use std::{
            hint::black_box,
            time::{Duration, Instant},
        };

        let mut connection = database();
        let mut created = 0_i64;
        for target_count in [100_i64, 1_000, 10_000] {
            let transaction = connection.transaction().expect("应能开始基准数据事务");
            {
                let mut insert = transaction
                    .prepare(
                        "INSERT INTO assets
                         (media_type,path_kind,stored_path,file_name,rating,is_favorite,is_public,created_at,updated_at)
                         VALUES('image','managed',?1,?2,5,1,1,?3,?3)",
                    )
                    .expect("应能准备 FTS 基准写入");
                for id in (created + 1)..=target_count {
                    insert
                        .execute(rusqlite::params![
                            format!("media/images/search-{id}.png"),
                            format!("needle-search-{id}.png"),
                            id
                        ])
                        .expect("应能写入 FTS 基准作品");
                }
            }
            transaction.commit().expect("应能提交 FTS 基准数据");
            created = target_count;

            let query = AssetListQuery {
                keyword: Some("needle".to_owned()),
                media_type: Some(MediaType::Image),
                rating: Some(5),
                is_favorite: Some(true),
                is_public: Some(true),
                limit: 24,
                ..AssetListQuery::default()
            };
            for _ in 0..5 {
                black_box(
                    LibraryRepository::list_assets(&connection, query.clone())
                        .expect("基准预热查询应成功"),
                );
            }
            let started = Instant::now();
            let page = LibraryRepository::list_assets(&connection, query).expect("基准查询应成功");
            let elapsed = started.elapsed();
            assert_eq!(page.items.len(), 24);
            assert!(
                elapsed < Duration::from_millis(500),
                "{target_count} 条 FTS/组合筛选耗时 {elapsed:?}"
            );
            println!(
                "asset_search_benchmark record_count={target_count} page_size=24 elapsed_ms={:.4}",
                elapsed.as_secs_f64() * 1_000.0
            );
        }
    }
}
