use rusqlite::{Connection, OptionalExtension, Row, Transaction, params, types::Type};

use crate::domain::{
    AiInputScope, AiProviderCapabilities, AiProviderConfig, AiProviderKind, AiSuggestion,
    AiSuggestionCategory, AiSuggestionTarget, AiSuggestionTargetType, AiSuggestionView,
    CreateAiSuggestions, SaveAiProviderConfig, SuggestionCursor, SuggestionImpact,
    SuggestionResolution, SuggestionStatus, SuggestionViewPage,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiRepositoryError {
    NotFound,
    Conflict,
    InvalidData,
    DatabaseFailed,
}

pub(crate) struct AiRepository;

impl AiRepository {
    pub(crate) fn list_providers(
        connection: &Connection,
    ) -> Result<Vec<AiProviderConfig>, AiRepositoryError> {
        let mut statement = connection
            .prepare("SELECT id,kind,display_name,endpoint,model,capabilities_json,timeout_ms,credential_id,is_enabled,created_at,updated_at FROM ai_providers ORDER BY id")
            .map_err(map_db_error)?;
        statement
            .query_map([], provider_from_row)
            .map_err(map_db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_db_error)
    }

    pub(crate) fn get_provider(
        connection: &Connection,
        id: i64,
    ) -> Result<AiProviderConfig, AiRepositoryError> {
        connection
            .query_row("SELECT id,kind,display_name,endpoint,model,capabilities_json,timeout_ms,credential_id,is_enabled,created_at,updated_at FROM ai_providers WHERE id=?1", [id], provider_from_row)
            .optional()
            .map_err(map_db_error)?
            .ok_or(AiRepositoryError::NotFound)
    }

    pub(crate) fn save_provider(
        connection: &mut Connection,
        input: &SaveAiProviderConfig,
        now: i64,
    ) -> Result<AiProviderConfig, AiRepositoryError> {
        let capabilities = serde_json::to_string(&input.capabilities)
            .map_err(|_| AiRepositoryError::InvalidData)?;
        let transaction = connection.transaction().map_err(map_db_error)?;
        let id = if let Some(id) = input.id {
            ensure_changed(transaction.execute(
                "UPDATE ai_providers SET kind=?1,display_name=?2,endpoint=?3,model=?4,capabilities_json=?5,timeout_ms=?6,credential_id=?7,is_enabled=?8,updated_at=max(updated_at,?9) WHERE id=?10",
                params![input.kind.as_db_str(), input.display_name.trim(), input.endpoint.trim(), input.model.trim(), capabilities, input.timeout_ms, input.credential_id.as_deref().map(str::trim).filter(|value| !value.is_empty()), input.is_enabled, now, id],
            ).map_err(map_db_error)?)?;
            id
        } else {
            transaction.execute(
                "INSERT INTO ai_providers(kind,display_name,endpoint,model,capabilities_json,timeout_ms,credential_id,is_enabled,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)",
                params![input.kind.as_db_str(), input.display_name.trim(), input.endpoint.trim(), input.model.trim(), capabilities, input.timeout_ms, input.credential_id.as_deref().map(str::trim).filter(|value| !value.is_empty()), input.is_enabled, now],
            ).map_err(map_db_error)?;
            transaction.last_insert_rowid()
        };
        let provider = transaction
            .query_row("SELECT id,kind,display_name,endpoint,model,capabilities_json,timeout_ms,credential_id,is_enabled,created_at,updated_at FROM ai_providers WHERE id=?1", [id], provider_from_row)
            .map_err(map_db_error)?;
        transaction.commit().map_err(map_db_error)?;
        Ok(provider)
    }

    pub(crate) fn delete_provider(
        connection: &mut Connection,
        id: i64,
    ) -> Result<(), AiRepositoryError> {
        ensure_changed(
            connection
                .execute("DELETE FROM ai_providers WHERE id=?1", [id])
                .map_err(map_db_error)?,
        )
    }

    /// 在发起批量 Provider 调用前统一校验目标仍存在且未进入回收站。
    ///
    /// 回收站中的实体已删除主记录，因此必须先检查回收站，才能给调用方返回稳定的冲突语义。
    pub(crate) fn ensure_targets_active(
        connection: &Connection,
        targets: &[AiSuggestionTarget],
    ) -> Result<(), AiRepositoryError> {
        for target in targets {
            let in_trash: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM trash_entries WHERE entity_type=?1 AND entity_id=?2)",
                    params![target.target_type.as_db_str(), target.target_id],
                    |row| row.get(0),
                )
                .map_err(map_db_error)?;
            if in_trash {
                return Err(AiRepositoryError::Conflict);
            }
            ensure_target_exists(connection, *target)?;
        }
        Ok(())
    }

    /// 只把已校验的 Provider 结果保存为 pending，绝不在这里修改正式分类关系。
    pub(crate) fn create_suggestions(
        connection: &mut Connection,
        input: &CreateAiSuggestions,
        now: i64,
    ) -> Result<Vec<i64>, AiRepositoryError> {
        if input.suggestions.is_empty() {
            return Err(AiRepositoryError::InvalidData);
        }
        let input_scope = serde_json::to_string(&input.input_scope)
            .map_err(|_| AiRepositoryError::InvalidData)?;
        let transaction = connection.transaction().map_err(map_db_error)?;
        ensure_target_exists(&transaction, input.target)?;
        if let Some(provider_id) = input.provider_id {
            ensure_provider_exists(&transaction, provider_id)?;
        }
        let mut ids = Vec::with_capacity(input.suggestions.len());
        for suggestion in &input.suggestions {
            validate_suggestion_draft(
                &transaction,
                suggestion.dimension_id,
                suggestion.category_id,
                suggestion.suggested_category_name.as_deref(),
                suggestion.confidence,
            )?;
            transaction.execute(
                "INSERT INTO ai_suggestions(provider_id,target_type,target_id,dimension_id,category_id,suggested_category_name,confidence,reason,source_model,input_scope_json,status,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'pending',?11,?11)",
                params![input.provider_id, input.target.target_type.as_db_str(), input.target.target_id, suggestion.dimension_id, suggestion.category_id, suggestion.suggested_category_name.as_deref().map(str::trim).filter(|value| !value.is_empty()), suggestion.confidence, suggestion.reason.trim(), input.source_model.trim(), input_scope, now],
            ).map_err(map_db_error)?;
            ids.push(transaction.last_insert_rowid());
        }
        transaction.commit().map_err(map_db_error)?;
        Ok(ids)
    }

    /// 审核页专用的分页投影。目标、维度、既有分类名称均由一次 SQL join 取得。
    pub(crate) fn list_pending_suggestion_views(
        connection: &Connection,
        cursor: Option<SuggestionCursor>,
        limit: u32,
    ) -> Result<SuggestionViewPage, AiRepositoryError> {
        let (cursor_updated_at, cursor_id) = cursor
            .map(|value| (Some(value.updated_at), Some(value.id)))
            .unwrap_or((None, None));
        let mut statement = connection.prepare(
            "SELECT s.id,s.provider_id,s.target_type,s.target_id,s.dimension_id,s.category_id,s.suggested_category_name,s.confidence,s.reason,s.source_model,s.input_scope_json,s.status,s.created_at,s.updated_at,
                    coalesce(pr.title,a.file_name,''),d.name,c.id,c.name
             FROM ai_suggestions s
             JOIN dimensions d ON d.id=s.dimension_id
             LEFT JOIN projects pr ON s.target_type='project' AND pr.id=s.target_id
             LEFT JOIN assets a ON s.target_type='asset' AND a.id=s.target_id
             LEFT JOIN categories c ON c.id=s.category_id
             WHERE s.status='pending'
               AND (pr.id IS NOT NULL OR a.id IS NOT NULL)
               AND (?1 IS NULL OR s.updated_at < ?1 OR (s.updated_at = ?1 AND s.id < ?2))
             ORDER BY s.updated_at DESC,s.id DESC LIMIT ?3",
        ).map_err(map_db_error)?;
        let mut items = statement
            .query_map(
                params![cursor_updated_at, cursor_id, i64::from(limit) + 1],
                suggestion_view_from_row,
            )
            .map_err(map_db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_db_error)?;
        let next_cursor = if items.len() > limit as usize {
            items.pop();
            items.last().map(|value| SuggestionCursor {
                updated_at: value.suggestion.updated_at,
                id: value.suggestion.id,
            })
        } else {
            None
        };
        Ok(SuggestionViewPage { items, next_cursor })
    }

    pub(crate) fn suggestion_impact(
        connection: &Connection,
        suggestion_id: i64,
    ) -> Result<SuggestionImpact, AiRepositoryError> {
        let suggestion = pending_suggestion(connection, suggestion_id)?;
        ensure_target_exists(connection, suggestion.target)?;
        let allows_multiple: bool = connection
            .query_row(
                "SELECT allows_multiple FROM dimensions WHERE id=?1 AND is_enabled=1",
                [suggestion.dimension_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(map_db_error)?
            .ok_or(AiRepositoryError::Conflict)?;
        let existing_category_ids =
            category_ids_in_dimension(connection, suggestion.target, suggestion.dimension_id)?;
        Ok(SuggestionImpact {
            suggestion_id,
            target: suggestion.target,
            dimension_id: suggestion.dimension_id,
            allows_multiple,
            existing_category_ids,
        })
    }

    /// 单个审核操作在一个事务中完成；任一验证或关系写入失败都会保留 pending 状态。
    pub(crate) fn resolve_suggestion(
        connection: &mut Connection,
        suggestion_id: i64,
        resolution: &SuggestionResolution,
        now: i64,
    ) -> Result<AiSuggestion, AiRepositoryError> {
        let transaction = connection.transaction().map_err(map_db_error)?;
        let suggestion = pending_suggestion(&transaction, suggestion_id)?;
        ensure_target_exists(&transaction, suggestion.target)?;
        let allows_multiple: bool = transaction
            .query_row(
                "SELECT allows_multiple FROM dimensions WHERE id=?1 AND is_enabled=1",
                [suggestion.dimension_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(map_db_error)?
            .ok_or(AiRepositoryError::Conflict)?;
        // ai_can_suggest_new 只约束模型生成建议；进入待审核后，用户点击接受就是明确授权。
        let final_status = match resolution {
            SuggestionResolution::Accept => {
                let category_id = match suggestion.category_id {
                    Some(category_id) => category_id,
                    None => find_or_create_category(
                        &transaction,
                        suggestion.dimension_id,
                        suggestion
                            .suggested_category_name
                            .as_deref()
                            .ok_or(AiRepositoryError::InvalidData)?,
                        now,
                    )?,
                };
                apply_category(
                    &transaction,
                    suggestion.target,
                    suggestion.dimension_id,
                    category_id,
                    allows_multiple,
                    now,
                )?;
                SuggestionStatus::Accepted
            }
            SuggestionResolution::AcceptExisting { category_id } => {
                apply_category(
                    &transaction,
                    suggestion.target,
                    suggestion.dimension_id,
                    *category_id,
                    allows_multiple,
                    now,
                )?;
                SuggestionStatus::Accepted
            }
            SuggestionResolution::CreateCategory { name } => {
                let category_id =
                    find_or_create_category(&transaction, suggestion.dimension_id, name, now)?;
                apply_category(
                    &transaction,
                    suggestion.target,
                    suggestion.dimension_id,
                    category_id,
                    allows_multiple,
                    now,
                )?;
                SuggestionStatus::Accepted
            }
            SuggestionResolution::MergeIntoExisting { category_id } => {
                apply_category(
                    &transaction,
                    suggestion.target,
                    suggestion.dimension_id,
                    *category_id,
                    allows_multiple,
                    now,
                )?;
                SuggestionStatus::Modified
            }
            SuggestionResolution::ConvertToTag { name } => {
                apply_tag(&transaction, suggestion.target, name, now)?;
                SuggestionStatus::Modified
            }
            SuggestionResolution::Reject => SuggestionStatus::Rejected,
        };
        ensure_changed(transaction.execute(
            "UPDATE ai_suggestions SET status=?1,updated_at=max(updated_at,?2) WHERE id=?3 AND status='pending'",
            params![final_status.as_db_str(), now, suggestion_id],
        ).map_err(map_db_error)?)?;
        let resolved = transaction.query_row(
            "SELECT id,provider_id,target_type,target_id,dimension_id,category_id,suggested_category_name,confidence,reason,source_model,input_scope_json,status,created_at,updated_at FROM ai_suggestions WHERE id=?1",
            [suggestion_id], suggestion_from_row,
        ).map_err(map_db_error)?;
        transaction.commit().map_err(map_db_error)?;
        Ok(resolved)
    }
}

fn provider_from_row(row: &Row<'_>) -> rusqlite::Result<AiProviderConfig> {
    let kind: String = row.get(1)?;
    let capabilities: String = row.get(5)?;
    Ok(AiProviderConfig {
        id: row.get(0)?,
        kind: AiProviderKind::from_db_str(&kind).ok_or_else(|| invalid_column(1, "kind"))?,
        display_name: row.get(2)?,
        endpoint: row.get(3)?,
        model: row.get(4)?,
        capabilities: serde_json::from_str::<AiProviderCapabilities>(&capabilities)
            .map_err(|_| invalid_column(5, "capabilities_json"))?,
        timeout_ms: row.get(6)?,
        credential_id: row.get(7)?,
        is_enabled: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn suggestion_from_row(row: &Row<'_>) -> rusqlite::Result<AiSuggestion> {
    let target_type: String = row.get(2)?;
    let input_scope: String = row.get(10)?;
    let status: String = row.get(11)?;
    Ok(AiSuggestion {
        id: row.get(0)?,
        provider_id: row.get(1)?,
        target: AiSuggestionTarget {
            target_type: AiSuggestionTargetType::from_db_str(&target_type)
                .ok_or_else(|| invalid_column(2, "target_type"))?,
            target_id: row.get(3)?,
        },
        dimension_id: row.get(4)?,
        category_id: row.get(5)?,
        suggested_category_name: row.get(6)?,
        confidence: row.get(7)?,
        reason: row.get(8)?,
        source_model: row.get(9)?,
        input_scope: serde_json::from_str::<AiInputScope>(&input_scope)
            .map_err(|_| invalid_column(10, "input_scope_json"))?,
        status: SuggestionStatus::from_db_str(&status)
            .ok_or_else(|| invalid_column(11, "status"))?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn suggestion_view_from_row(row: &Row<'_>) -> rusqlite::Result<AiSuggestionView> {
    let suggestion = suggestion_from_row(row)?;
    let category_id: Option<i64> = row.get(16)?;
    let category = match category_id {
        Some(id) => Some(AiSuggestionCategory {
            id,
            name: row.get(17)?,
        }),
        None => None,
    };
    Ok(AiSuggestionView {
        suggestion,
        target_title: row.get(14)?,
        dimension_name: row.get(15)?,
        category,
    })
}

fn invalid_column(index: usize, name: &str) -> rusqlite::Error {
    rusqlite::Error::InvalidColumnType(index, name.to_owned(), Type::Text)
}

fn pending_suggestion(connection: &Connection, id: i64) -> Result<AiSuggestion, AiRepositoryError> {
    connection.query_row(
        "SELECT id,provider_id,target_type,target_id,dimension_id,category_id,suggested_category_name,confidence,reason,source_model,input_scope_json,status,created_at,updated_at FROM ai_suggestions WHERE id=?1 AND status='pending'",
        [id], suggestion_from_row,
    ).optional().map_err(map_db_error)?.ok_or(AiRepositoryError::Conflict)
}

fn ensure_provider_exists(tx: &Transaction<'_>, id: i64) -> Result<(), AiRepositoryError> {
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM ai_providers WHERE id=?1)",
            [id],
            |row| row.get(0),
        )
        .map_err(map_db_error)?;
    if exists {
        Ok(())
    } else {
        Err(AiRepositoryError::NotFound)
    }
}

fn ensure_target_exists(
    connection: &Connection,
    target: AiSuggestionTarget,
) -> Result<(), AiRepositoryError> {
    let table = match target.target_type {
        AiSuggestionTargetType::Project => "projects",
        AiSuggestionTargetType::Asset => "assets",
    };
    let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id=?1)");
    let exists: bool = connection
        .query_row(&sql, [target.target_id], |row| row.get(0))
        .map_err(map_db_error)?;
    if exists {
        Ok(())
    } else {
        Err(AiRepositoryError::NotFound)
    }
}

fn validate_suggestion_draft(
    tx: &Transaction<'_>,
    dimension_id: i64,
    category_id: Option<i64>,
    suggested_name: Option<&str>,
    confidence: Option<f64>,
) -> Result<(), AiRepositoryError> {
    if confidence.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value)) {
        return Err(AiRepositoryError::InvalidData);
    }
    if category_id.is_none()
        && suggested_name
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
    {
        return Err(AiRepositoryError::InvalidData);
    }
    let dimension_enabled: bool = tx
        .query_row(
            "SELECT is_enabled FROM dimensions WHERE id=?1",
            [dimension_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_db_error)?
        .ok_or(AiRepositoryError::NotFound)?;
    if !dimension_enabled {
        return Err(AiRepositoryError::Conflict);
    }
    if let Some(category_id) = category_id {
        let valid: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM categories WHERE id=?1 AND dimension_id=?2 AND is_enabled=1)", params![category_id, dimension_id], |row| row.get(0)).map_err(map_db_error)?;
        if !valid {
            return Err(AiRepositoryError::Conflict);
        }
    }
    Ok(())
}

fn category_ids_in_dimension(
    connection: &Connection,
    target: AiSuggestionTarget,
    dimension_id: i64,
) -> Result<Vec<i64>, AiRepositoryError> {
    let (table, owner) = match target.target_type {
        AiSuggestionTargetType::Project => ("project_categories", "project_id"),
        AiSuggestionTargetType::Asset => ("asset_categories", "asset_id"),
    };
    let sql = format!(
        "SELECT relation.category_id FROM {table} relation JOIN categories c ON c.id=relation.category_id WHERE relation.{owner}=?1 AND c.dimension_id=?2 ORDER BY relation.category_id"
    );
    let mut statement = connection.prepare(&sql).map_err(map_db_error)?;
    statement
        .query_map(params![target.target_id, dimension_id], |row| row.get(0))
        .map_err(map_db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_db_error)
}

fn apply_category(
    tx: &Transaction<'_>,
    target: AiSuggestionTarget,
    dimension_id: i64,
    category_id: i64,
    allows_multiple: bool,
    now: i64,
) -> Result<(), AiRepositoryError> {
    let valid: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM categories WHERE id=?1 AND dimension_id=?2 AND is_enabled=1)", params![category_id, dimension_id], |row| row.get(0)).map_err(map_db_error)?;
    if !valid {
        return Err(AiRepositoryError::Conflict);
    }
    let (table, owner) = match target.target_type {
        AiSuggestionTargetType::Project => ("project_categories", "project_id"),
        AiSuggestionTargetType::Asset => ("asset_categories", "asset_id"),
    };
    if !allows_multiple {
        let delete = format!(
            "DELETE FROM {table} WHERE {owner}=?1 AND category_id IN (SELECT id FROM categories WHERE dimension_id=?2)"
        );
        tx.execute(&delete, params![target.target_id, dimension_id])
            .map_err(map_db_error)?;
    }
    let insert =
        format!("INSERT OR IGNORE INTO {table}({owner},category_id,created_at) VALUES(?1,?2,?3)");
    tx.execute(&insert, params![target.target_id, category_id, now])
        .map_err(map_db_error)?;
    Ok(())
}

fn find_or_create_category(
    tx: &Transaction<'_>,
    dimension_id: i64,
    name: &str,
    now: i64,
) -> Result<i64, AiRepositoryError> {
    let normalized = normalize_name(name).ok_or(AiRepositoryError::InvalidData)?;
    let mut statement = tx.prepare("SELECT id,name,aliases_json,is_enabled FROM categories WHERE dimension_id=?1 ORDER BY id").map_err(map_db_error)?;
    let rows = statement
        .query_map([dimension_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, bool>(3)?,
            ))
        })
        .map_err(map_db_error)?;
    for row in rows {
        let (id, category_name, aliases_json, enabled) = row.map_err(map_db_error)?;
        let aliases: Vec<String> =
            serde_json::from_str(&aliases_json).map_err(|_| AiRepositoryError::InvalidData)?;
        if normalize_name(&category_name).as_deref() == Some(&normalized)
            || aliases
                .iter()
                .any(|alias| normalize_name(alias).as_deref() == Some(&normalized))
        {
            return if enabled {
                Ok(id)
            } else {
                Err(AiRepositoryError::Conflict)
            };
        }
    }
    tx.execute("INSERT INTO categories(dimension_id,name,aliases_json,description,is_enabled,created_at,updated_at) VALUES(?1,?2,'[]','',1,?3,?3)", params![dimension_id, name.trim(), now]).map_err(map_db_error)?;
    Ok(tx.last_insert_rowid())
}

fn apply_tag(
    tx: &Transaction<'_>,
    target: AiSuggestionTarget,
    name: &str,
    now: i64,
) -> Result<(), AiRepositoryError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AiRepositoryError::InvalidData);
    }
    tx.execute(
        "INSERT OR IGNORE INTO tags(name,created_at,updated_at) VALUES(?1,?2,?2)",
        params![name, now],
    )
    .map_err(map_db_error)?;
    let tag_id: i64 = tx
        .query_row("SELECT id FROM tags WHERE name=?1", [name], |row| {
            row.get(0)
        })
        .map_err(map_db_error)?;
    let (table, owner) = match target.target_type {
        AiSuggestionTargetType::Project => ("project_tags", "project_id"),
        AiSuggestionTargetType::Asset => ("asset_tags", "asset_id"),
    };
    let insert =
        format!("INSERT OR IGNORE INTO {table}({owner},tag_id,created_at) VALUES(?1,?2,?3)");
    tx.execute(&insert, params![target.target_id, tag_id, now])
        .map_err(map_db_error)?;
    Ok(())
}

fn normalize_name(value: &str) -> Option<String> {
    let normalized = value.trim().to_lowercase();
    (!normalized.is_empty()).then_some(normalized)
}

fn ensure_changed(changed: usize) -> Result<(), AiRepositoryError> {
    if changed == 0 {
        Err(AiRepositoryError::NotFound)
    } else {
        Ok(())
    }
}

fn map_db_error(error: rusqlite::Error) -> AiRepositoryError {
    match error {
        rusqlite::Error::SqliteFailure(code, _)
            if code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
                || code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY
                || code.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            AiRepositoryError::Conflict
        }
        rusqlite::Error::FromSqlConversionFailure(..)
        | rusqlite::Error::IntegralValueOutOfRange(..) => AiRepositoryError::InvalidData,
        _ => AiRepositoryError::DatabaseFailed,
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::{Connection, params};

    use super::{AiRepository, AiRepositoryError};
    use crate::domain::{
        AiInputScope, AiSuggestionTarget, AiSuggestionTargetType, ClassifySuggestion,
        CreateAiSuggestions, SuggestionResolution, SuggestionStatus,
    };

    fn database() -> Connection {
        let connection = Connection::open_in_memory().expect("应能打开内存数据库");
        connection
            .pragma_update(None, "foreign_keys", true)
            .expect("应能启用外键");
        connection
            .execute_batch(include_str!("../migrations/0001_initial.sql"))
            .expect("应能建立 v1");
        connection
            .execute_batch(include_str!("../migrations/0002_search.sql"))
            .expect("应能建立 v2");
        connection
            .execute_batch(include_str!("../migrations/0003_ai.sql"))
            .expect("应能建立 v3");
        connection
    }

    fn create_project(connection: &Connection) -> i64 {
        connection
            .execute(
                "INSERT INTO projects(title,created_at,updated_at) VALUES('测试项目',1,1)",
                [],
            )
            .expect("应能创建项目");
        connection.last_insert_rowid()
    }

    fn create_dimension(
        connection: &Connection,
        allows_multiple: bool,
        ai_can_suggest_new: bool,
    ) -> i64 {
        connection.execute("INSERT INTO dimensions(name,allows_multiple,ai_can_suggest_new,is_enabled,created_at,updated_at) VALUES('风格',?1,?2,1,1,1)", params![allows_multiple, ai_can_suggest_new]).expect("应能创建维度");
        connection.last_insert_rowid()
    }

    fn create_category(
        connection: &Connection,
        dimension_id: i64,
        name: &str,
        aliases: &str,
    ) -> i64 {
        connection.execute("INSERT INTO categories(dimension_id,name,aliases_json,created_at,updated_at) VALUES(?1,?2,?3,1,1)", params![dimension_id, name, aliases]).expect("应能创建分类");
        connection.last_insert_rowid()
    }

    fn suggestion(
        target_id: i64,
        dimension_id: i64,
        category_id: Option<i64>,
        name: Option<&str>,
    ) -> CreateAiSuggestions {
        CreateAiSuggestions {
            provider_id: None,
            target: AiSuggestionTarget {
                target_type: AiSuggestionTargetType::Project,
                target_id,
            },
            source_model: "测试模型".to_owned(),
            input_scope: AiInputScope::default(),
            suggestions: vec![ClassifySuggestion {
                dimension_id,
                category_id,
                suggested_category_name: name.map(str::to_owned),
                confidence: Some(0.9),
                reason: "测试原因".to_owned(),
            }],
        }
    }

    #[test]
    fn resolution_reuses_exact_alias_and_replaces_single_dimension_relation() {
        let mut connection = database();
        let project_id = create_project(&connection);
        let dimension_id = create_dimension(&connection, false, true);
        let old = create_category(&connection, dimension_id, "旧分类", "[]");
        let expected = create_category(&connection, dimension_id, "电影感", "[\"cinematic\"]");
        connection
            .execute(
                "INSERT INTO project_categories(project_id,category_id,created_at) VALUES(?1,?2,1)",
                params![project_id, old],
            )
            .expect("应能建立旧关系");
        let id = AiRepository::create_suggestions(
            &mut connection,
            &suggestion(project_id, dimension_id, None, Some(" CINEMATIC ")),
            2,
        )
        .expect("应能创建待审核建议")[0];
        let resolved =
            AiRepository::resolve_suggestion(&mut connection, id, &SuggestionResolution::Accept, 3)
                .expect("应能通过别名复用分类");
        assert_eq!(resolved.status, SuggestionStatus::Accepted);
        let categories: Vec<i64> = connection
            .prepare("SELECT category_id FROM project_categories WHERE project_id=?1")
            .expect("应能查询关系")
            .query_map([project_id], |row| row.get(0))
            .expect("应能读取关系")
            .collect::<Result<_, _>>()
            .expect("关系应可解析");
        assert_eq!(categories, vec![expected]);
        let count: i64 = connection
            .query_row(
                "SELECT count(*) FROM categories WHERE dimension_id=?1",
                [dimension_id],
                |row| row.get(0),
            )
            .expect("应能统计分类");
        assert_eq!(count, 2);
    }

    #[test]
    fn accepted_new_category_is_created_even_when_ai_cannot_suggest_new() {
        let mut connection = database();
        let project_id = create_project(&connection);
        let dimension_id = create_dimension(&connection, true, false);
        let id = AiRepository::create_suggestions(
            &mut connection,
            &suggestion(project_id, dimension_id, None, Some("不允许创建")),
            2,
        )
        .expect("应能创建建议")[0];
        let resolved =
            AiRepository::resolve_suggestion(&mut connection, id, &SuggestionResolution::Accept, 3)
                .expect("用户接受后应能创建 AI 建议的新分类");
        assert_eq!(resolved.status, SuggestionStatus::Accepted);
        let status: String = connection
            .query_row(
                "SELECT status FROM ai_suggestions WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .expect("建议应存在");
        assert_eq!(status, "accepted");
        let categories: i64 = connection
            .query_row("SELECT count(*) FROM categories", [], |row| row.get(0))
            .expect("应能统计分类");
        assert_eq!(categories, 1);
        let relations: i64 = connection
            .query_row(
                "SELECT count(*) FROM project_categories WHERE project_id=?1",
                [project_id],
                |row| row.get(0),
            )
            .expect("应能统计作品与新分类的关系");
        assert_eq!(relations, 1);
    }

    #[test]
    fn pending_suggestions_are_paginated_and_target_deletion_rejects_them() {
        let mut connection = database();
        let project_id = create_project(&connection);
        let dimension_id = create_dimension(&connection, true, true);
        for now in [2, 3] {
            AiRepository::create_suggestions(
                &mut connection,
                &suggestion(project_id, dimension_id, None, Some("新分类")),
                now,
            )
            .expect("应能创建建议");
        }
        let page = AiRepository::list_pending_suggestion_views(&connection, None, 1)
            .expect("应能分页读取建议");
        assert_eq!(page.items.len(), 1);
        let second = AiRepository::list_pending_suggestion_views(&connection, page.next_cursor, 1)
            .expect("应能读取下一页");
        assert_eq!(second.items.len(), 1);
        connection
            .execute("DELETE FROM projects WHERE id=?1", [project_id])
            .expect("应能删除项目");
        assert!(
            AiRepository::list_pending_suggestion_views(&connection, None, 10)
                .expect("孤儿建议不得列出")
                .items
                .is_empty()
        );
        let rejected: i64 = connection
            .query_row(
                "SELECT count(*) FROM ai_suggestions WHERE status='rejected'",
                [],
                |row| row.get(0),
            )
            .expect("建议应已拒绝");
        assert_eq!(rejected, 2);
    }

    #[test]
    fn batch_target_validation_rejects_missing_or_trashed_targets_before_provider_calls() {
        let connection = database();
        let project_id = create_project(&connection);
        let active = AiSuggestionTarget {
            target_type: AiSuggestionTargetType::Project,
            target_id: project_id,
        };
        assert!(AiRepository::ensure_targets_active(&connection, &[active]).is_ok());
        assert_eq!(
            AiRepository::ensure_targets_active(
                &connection,
                &[AiSuggestionTarget {
                    target_type: AiSuggestionTargetType::Project,
                    target_id: project_id + 1,
                }],
            ),
            Err(AiRepositoryError::NotFound)
        );
        connection
            .execute(
                "INSERT INTO trash_entries(entity_type,entity_id,snapshot_json,media_action,deleted_at) VALUES('project',?1,'{}','keep',2)",
                [project_id],
            )
            .expect("应能构造回收站目标");
        assert_eq!(
            AiRepository::ensure_targets_active(&connection, &[active]),
            Err(AiRepositoryError::Conflict)
        );
    }

    #[test]
    fn invalid_suggestion_in_one_target_batch_rolls_back_all_of_that_target_pending_rows() {
        let mut connection = database();
        let project_id = create_project(&connection);
        let dimension_id = create_dimension(&connection, true, true);
        let mut draft = suggestion(project_id, dimension_id, None, Some("可用建议"));
        draft.suggestions.push(ClassifySuggestion {
            dimension_id: dimension_id + 10_000,
            category_id: None,
            suggested_category_name: Some("无效维度".to_owned()),
            confidence: Some(0.8),
            reason: "应触发事务回滚".to_owned(),
        });
        assert_eq!(
            AiRepository::create_suggestions(&mut connection, &draft, 2),
            Err(AiRepositoryError::NotFound)
        );
        let pending: i64 = connection
            .query_row(
                "SELECT count(*) FROM ai_suggestions WHERE status='pending'",
                [],
                |row| row.get(0),
            )
            .expect("应能统计待确认建议");
        assert_eq!(pending, 0, "失败目标不得留下半成品 pending 建议");
    }
}
