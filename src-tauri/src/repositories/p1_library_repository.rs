use rusqlite::{
    Connection, OptionalExtension, Row, params, params_from_iter, types::{Type, Value},
};

use crate::domain::{
    CustomField, CustomFieldPage, CustomFieldTargetType, CustomFieldValue, CustomFieldValueInput,
    CustomFieldValuePage, CustomFieldValueStatus, CustomFieldValueType, EditAction,
    EditHistoryEntry, EditHistoryInput, EditHistoryPage, EditHistoryStatus, EditHistoryTargetType,
    P1PageCursor, PromptText, PromptVersion, PromptVersionCursor, PromptVersionPage,
    SaveCustomField, SaveSavedFilter, SavedAssetFilter, SavedFilter, SavedFilterPage,
};

const MIN_PAGE_LIMIT: u32 = 1;
const MAX_PAGE_LIMIT: u32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum P1LibraryRepositoryError {
    NotFound,
    Conflict,
    InvalidData,
    DatabaseFailed,
}

/// M6 数据访问边界。这里的自动化写入接口固定生成 pending，不会修改正式字段。
pub(crate) struct P1LibraryRepository;

impl P1LibraryRepository {
    pub(crate) fn save_saved_filter(
        connection: &mut Connection,
        input: &SaveSavedFilter,
        now: i64,
    ) -> Result<SavedFilter, P1LibraryRepositoryError> {
        let name = normalized_name(&input.name)?;
        validate_saved_asset_filter(&input.filter)?;
        let filter = serde_json::to_string(&input.filter)
            .map_err(|_| P1LibraryRepositoryError::InvalidData)?;
        let transaction = connection.transaction().map_err(map_db_error)?;
        let id = if let Some(id) = input.id {
            ensure_changed(transaction.execute(
                "UPDATE saved_filters SET name=?1,filter_json=?2,updated_at=max(updated_at,?3) WHERE id=?4",
                params![name, filter, now, id],
            ).map_err(map_db_error)?)?;
            id
        } else {
            transaction.execute(
                "INSERT INTO saved_filters(name,filter_json,created_at,updated_at) VALUES(?1,?2,?3,?3)",
                params![name, filter, now],
            ).map_err(map_db_error)?;
            transaction.last_insert_rowid()
        };
        let saved = transaction
            .query_row(
                "SELECT id,name,filter_json,created_at,updated_at FROM saved_filters WHERE id=?1",
                [id],
                saved_filter_from_row,
            )
            .map_err(map_db_error)?;
        transaction.commit().map_err(map_db_error)?;
        Ok(saved)
    }

    pub(crate) fn list_saved_filters(
        connection: &Connection,
        cursor: Option<P1PageCursor>,
        limit: u32,
    ) -> Result<SavedFilterPage, P1LibraryRepositoryError> {
        validate_limit(limit)?;
        let (time, id) = cursor
            .map(|value| (Some(value.updated_at), Some(value.id)))
            .unwrap_or((None, None));
        let mut statement = connection
            .prepare(
                "SELECT id,name,filter_json,created_at,updated_at FROM saved_filters
             WHERE (?1 IS NULL OR updated_at < ?1 OR (updated_at=?1 AND id < ?2))
             ORDER BY updated_at DESC,id DESC LIMIT ?3",
            )
            .map_err(map_db_error)?;
        let mut values = statement
            .query_map(
                params![time, id, i64::from(limit) + 1],
                saved_filter_from_row,
            )
            .map_err(map_db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_db_error)?;
        Ok(page(&mut values, limit, |item| P1PageCursor {
            updated_at: item.updated_at,
            id: item.id,
        }))
    }

    pub(crate) fn delete_saved_filter(
        connection: &mut Connection,
        id: i64,
    ) -> Result<(), P1LibraryRepositoryError> {
        ensure_changed(
            connection
                .execute("DELETE FROM saved_filters WHERE id=?1", [id])
                .map_err(map_db_error)?,
        )
    }

    pub(crate) fn save_custom_field(
        connection: &mut Connection,
        input: &SaveCustomField,
        now: i64,
    ) -> Result<CustomField, P1LibraryRepositoryError> {
        let name = normalized_name(&input.name)?;
        if !input.options.is_object() {
            return Err(P1LibraryRepositoryError::InvalidData);
        }
        let options = serde_json::to_string(&input.options)
            .map_err(|_| P1LibraryRepositoryError::InvalidData)?;
        let transaction = connection.transaction().map_err(map_db_error)?;
        let id = if let Some(id) = input.id {
            let current = transaction
                .query_row(
                    "SELECT target_type,value_type FROM custom_fields WHERE id=?1",
                    [id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()
                .map_err(map_db_error)?
                .ok_or(P1LibraryRepositoryError::NotFound)?;
            let has_values: bool = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM custom_field_values WHERE field_id=?1)",
                    [id],
                    |row| row.get(0),
                )
                .map_err(map_db_error)?;
            if has_values
                && (current.0 != input.target_type.as_db_str()
                    || current.1 != input.value_type.as_db_str())
            {
                return Err(P1LibraryRepositoryError::Conflict);
            }
            ensure_changed(transaction.execute(
                "UPDATE custom_fields SET name=?1,target_type=?2,value_type=?3,options_json=?4,is_enabled=?5,updated_at=max(updated_at,?6) WHERE id=?7",
                params![name, input.target_type.as_db_str(), input.value_type.as_db_str(), options, input.is_enabled, now, id],
            ).map_err(map_db_error)?)?;
            id
        } else {
            transaction.execute(
                "INSERT INTO custom_fields(name,target_type,value_type,options_json,is_enabled,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?6)",
                params![name, input.target_type.as_db_str(), input.value_type.as_db_str(), options, input.is_enabled, now],
            ).map_err(map_db_error)?;
            transaction.last_insert_rowid()
        };
        let field = transaction.query_row(
            "SELECT id,name,target_type,value_type,options_json,is_enabled,created_at,updated_at FROM custom_fields WHERE id=?1",
            [id], custom_field_from_row,
        ).map_err(map_db_error)?;
        transaction.commit().map_err(map_db_error)?;
        Ok(field)
    }

    pub(crate) fn list_custom_fields(
        connection: &Connection,
        target_type: CustomFieldTargetType,
        cursor: Option<P1PageCursor>,
        limit: u32,
    ) -> Result<CustomFieldPage, P1LibraryRepositoryError> {
        validate_limit(limit)?;
        let (time, id) = cursor
            .map(|value| (Some(value.updated_at), Some(value.id)))
            .unwrap_or((None, None));
        let mut statement = connection.prepare(
            "SELECT id,name,target_type,value_type,options_json,is_enabled,created_at,updated_at FROM custom_fields
             WHERE target_type=?1 AND (?2 IS NULL OR updated_at < ?2 OR (updated_at=?2 AND id < ?3))
             ORDER BY updated_at DESC,id DESC LIMIT ?4",
        ).map_err(map_db_error)?;
        let mut values = statement
            .query_map(
                params![target_type.as_db_str(), time, id, i64::from(limit) + 1],
                custom_field_from_row,
            )
            .map_err(map_db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_db_error)?;
        Ok(page(&mut values, limit, |item| P1PageCursor {
            updated_at: item.updated_at,
            id: item.id,
        }))
    }

    pub(crate) fn save_manual_custom_field_value(
        connection: &mut Connection,
        input: &CustomFieldValueInput,
        now: i64,
    ) -> Result<CustomFieldValue, P1LibraryRepositoryError> {
        let value = serde_json::to_string(&input.value)
            .map_err(|_| P1LibraryRepositoryError::InvalidData)?;
        let transaction = connection.transaction().map_err(map_db_error)?;
        transaction.execute(
            "INSERT INTO custom_field_values(field_id,target_type,target_id,value_json,source,status,created_at,updated_at)
             VALUES(?1,?2,?3,?4,'manual','confirmed',?5,?5)
             ON CONFLICT(field_id,target_type,target_id,status) DO UPDATE SET value_json=excluded.value_json,source=excluded.source,updated_at=max(custom_field_values.updated_at,excluded.updated_at)",
            params![input.field_id, input.target_type.as_db_str(), input.target_id, value, now],
        ).map_err(map_db_error)?;
        let result = transaction.query_row(
            "SELECT id,field_id,target_type,target_id,value_json,status,created_at,updated_at FROM custom_field_values WHERE field_id=?1 AND target_type=?2 AND target_id=?3 AND status='confirmed'",
            params![input.field_id, input.target_type.as_db_str(), input.target_id], custom_field_value_from_row,
        ).map_err(map_db_error)?;
        Self::record_manual_edit(
            &transaction,
            &EditHistoryInput {
                target_type: EditHistoryTargetType::CustomFieldValue,
                target_id: result.id,
                action: EditAction::Update,
                changed_fields: vec!["value".to_owned()],
            },
            now,
        )?;
        transaction.commit().map_err(map_db_error)?;
        Ok(result)
    }

    /// 确认候选值时先替换旧正式值，再将候选转为正式值，整体处于一个事务内。
    pub(crate) fn confirm_pending_custom_field_value(
        connection: &mut Connection,
        id: i64,
        now: i64,
    ) -> Result<CustomFieldValue, P1LibraryRepositoryError> {
        let transaction = connection.transaction().map_err(map_db_error)?;
        let (field_id, target_type, target_id): (i64, String, i64) = transaction.query_row(
            "SELECT field_id,target_type,target_id FROM custom_field_values WHERE id=?1 AND status='pending'", [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).optional().map_err(map_db_error)?.ok_or(P1LibraryRepositoryError::NotFound)?;
        transaction.execute(
            "DELETE FROM custom_field_values WHERE field_id=?1 AND target_type=?2 AND target_id=?3 AND status='confirmed'",
            params![field_id, target_type, target_id],
        ).map_err(map_db_error)?;
        ensure_changed(transaction.execute(
            "UPDATE custom_field_values SET source='manual',status='confirmed',updated_at=max(updated_at,?1) WHERE id=?2 AND status='pending'",
            params![now, id],
        ).map_err(map_db_error)?)?;
        let value = transaction.query_row(
            "SELECT id,field_id,target_type,target_id,value_json,status,created_at,updated_at FROM custom_field_values WHERE id=?1", [id], custom_field_value_from_row,
        ).map_err(map_db_error)?;
        Self::record_manual_edit(
            &transaction,
            &EditHistoryInput {
                target_type: EditHistoryTargetType::CustomFieldValue,
                target_id: id,
                action: EditAction::Update,
                changed_fields: vec!["value".to_owned(), "status".to_owned()],
            },
            now,
        )?;
        transaction.commit().map_err(map_db_error)?;
        Ok(value)
    }

    pub(crate) fn list_custom_field_values(
        connection: &Connection,
        target_type: CustomFieldTargetType,
        target_id: i64,
        cursor: Option<P1PageCursor>,
        limit: u32,
    ) -> Result<CustomFieldValuePage, P1LibraryRepositoryError> {
        validate_limit(limit)?;
        let (time, id) = cursor
            .map(|value| (Some(value.updated_at), Some(value.id)))
            .unwrap_or((None, None));
        let mut statement = connection.prepare(
            "SELECT id,field_id,target_type,target_id,value_json,status,created_at,updated_at FROM custom_field_values
             WHERE target_type=?1 AND target_id=?2 AND (?3 IS NULL OR updated_at < ?3 OR (updated_at=?3 AND id < ?4))
             ORDER BY updated_at DESC,id DESC LIMIT ?5",
        ).map_err(map_db_error)?;
        let mut values = statement
            .query_map(
                params![
                    target_type.as_db_str(),
                    target_id,
                    time,
                    id,
                    i64::from(limit) + 1
                ],
                custom_field_value_from_row,
            )
            .map_err(map_db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_db_error)?;
        Ok(page(&mut values, limit, |item| P1PageCursor {
            updated_at: item.updated_at,
            id: item.id,
        }))
    }

    pub(crate) fn list_custom_field_values_for_targets(
        connection: &Connection,
        target_type: CustomFieldTargetType,
        target_ids: &[i64],
        cursor: Option<P1PageCursor>,
        limit: u32,
    ) -> Result<CustomFieldValuePage, P1LibraryRepositoryError> {
        validate_limit(limit)?;
        if target_ids.is_empty() {
            return Ok(crate::domain::Page {
                items: Vec::new(),
                next_cursor: None,
            });
        }

        let mut sql = String::from(
            "SELECT id,field_id,target_type,target_id,value_json,status,created_at,updated_at FROM custom_field_values
             WHERE target_type=?1 AND target_id IN (",
        );
        let placeholders = (0..target_ids.len())
            .map(|index| format!("?{}", index + 2))
            .collect::<Vec<_>>()
            .join(",");
        sql.push_str(&placeholders);
        let cursor_parameter = target_ids.len() + 2;
        sql.push_str(&format!(
            ") AND (?{cursor_parameter} IS NULL OR updated_at < ?{} OR (updated_at=?{} AND id < ?{}))
             ORDER BY updated_at DESC,id DESC LIMIT ?{}",
            cursor_parameter + 1,
            cursor_parameter + 2,
            cursor_parameter + 3,
            cursor_parameter + 4
        ));

        let (time, id) = cursor
            .map(|value| (Some(value.updated_at), Some(value.id)))
            .unwrap_or((None, None));
        let mut parameters = Vec::with_capacity(target_ids.len() + 6);
        parameters.push(Value::Text(target_type.as_db_str().to_owned()));
        parameters.extend(target_ids.iter().copied().map(Value::Integer));
        parameters.push(time.map(Value::Integer).unwrap_or(Value::Null));
        parameters.push(time.map(Value::Integer).unwrap_or(Value::Null));
        parameters.push(time.map(Value::Integer).unwrap_or(Value::Null));
        parameters.push(id.map(Value::Integer).unwrap_or(Value::Null));
        parameters.push(Value::Integer(i64::from(limit) + 1));

        let mut statement = connection.prepare(&sql).map_err(map_db_error)?;
        let mut values = statement
            .query_map(params_from_iter(parameters), custom_field_value_from_row)
            .map_err(map_db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_db_error)?;
        Ok(page(&mut values, limit, |item| P1PageCursor {
            updated_at: item.updated_at,
            id: item.id,
        }))
    }

    /// 版本不可变；调用方在保存提示词前创建一个快照，以便可靠回溯。
    pub(crate) fn create_prompt_version(
        connection: &mut Connection,
        prompt_id: i64,
        prompt: &PromptText,
        now: i64,
    ) -> Result<PromptVersion, P1LibraryRepositoryError> {
        let transaction = connection.transaction().map_err(map_db_error)?;
        let exists: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM prompts WHERE id=?1)",
                [prompt_id],
                |row| row.get(0),
            )
            .map_err(map_db_error)?;
        if !exists {
            return Err(P1LibraryRepositoryError::NotFound);
        }
        let version: i64 = transaction
            .query_row(
                "SELECT coalesce(max(version),0)+1 FROM prompt_versions WHERE prompt_id=?1",
                [prompt_id],
                |row| row.get(0),
            )
            .map_err(map_db_error)?;
        transaction.execute(
            "INSERT INTO prompt_versions(prompt_id,version,prompt_zh,prompt_en,negative_prompt,created_at) VALUES(?1,?2,?3,?4,?5,?6)",
            params![prompt_id, version, prompt.prompt_zh, prompt.prompt_en, prompt.negative_prompt, now],
        ).map_err(map_db_error)?;
        let result = transaction.query_row(
            "SELECT id,prompt_id,version,prompt_zh,prompt_en,negative_prompt,created_at FROM prompt_versions WHERE id=?1",
            [transaction.last_insert_rowid()], prompt_version_from_row,
        ).map_err(map_db_error)?;
        transaction.commit().map_err(map_db_error)?;
        Ok(result)
    }

    pub(crate) fn list_prompt_versions(
        connection: &Connection,
        prompt_id: i64,
        cursor: Option<PromptVersionCursor>,
        limit: u32,
    ) -> Result<PromptVersionPage, P1LibraryRepositoryError> {
        validate_limit(limit)?;
        let (version, id) = cursor
            .map(|value| (Some(value.version), Some(value.id)))
            .unwrap_or((None, None));
        let mut statement = connection.prepare(
            "SELECT id,prompt_id,version,prompt_zh,prompt_en,negative_prompt,created_at FROM prompt_versions
             WHERE prompt_id=?1 AND (?2 IS NULL OR version < ?2 OR (version=?2 AND id < ?3))
             ORDER BY version DESC,id DESC LIMIT ?4",
        ).map_err(map_db_error)?;
        let mut items = statement
            .query_map(
                params![prompt_id, version, id, i64::from(limit) + 1],
                prompt_version_from_row,
            )
            .map_err(map_db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_db_error)?;
        let next_cursor = if items.len() > limit as usize {
            items.pop();
            items.last().map(|item| PromptVersionCursor {
                version: item.version,
                id: item.id,
            })
        } else {
            None
        };
        Ok(PromptVersionPage { items, next_cursor })
    }

    pub(crate) fn record_manual_edit(
        connection: &Connection,
        input: &EditHistoryInput,
        now: i64,
    ) -> Result<EditHistoryEntry, P1LibraryRepositoryError> {
        Self::record_edit(connection, input, "manual", "confirmed", now)
    }

    fn record_edit(
        connection: &Connection,
        input: &EditHistoryInput,
        source: &str,
        status: &str,
        now: i64,
    ) -> Result<EditHistoryEntry, P1LibraryRepositoryError> {
        let fields = normalized_field_names(&input.changed_fields)?;
        let fields =
            serde_json::to_string(&fields).map_err(|_| P1LibraryRepositoryError::InvalidData)?;
        connection.execute(
            "INSERT INTO edit_history(target_type,target_id,action,changed_fields_json,source,status,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?7)",
            params![input.target_type.as_db_str(), input.target_id, input.action.as_db_str(), fields, source, status, now],
        ).map_err(map_db_error)?;
        connection.query_row(
            "SELECT id,target_type,target_id,action,changed_fields_json,status,created_at,updated_at FROM edit_history WHERE id=?1",
            [connection.last_insert_rowid()], edit_history_from_row,
        ).map_err(map_db_error)
    }

    pub(crate) fn list_edit_history(
        connection: &Connection,
        target_type: crate::domain::EditHistoryTargetType,
        target_id: i64,
        cursor: Option<P1PageCursor>,
        limit: u32,
    ) -> Result<EditHistoryPage, P1LibraryRepositoryError> {
        validate_limit(limit)?;
        let (time, id) = cursor
            .map(|value| (Some(value.updated_at), Some(value.id)))
            .unwrap_or((None, None));
        let mut statement = connection.prepare(
            "SELECT id,target_type,target_id,action,changed_fields_json,status,created_at,updated_at FROM edit_history
             WHERE target_type=?1 AND target_id=?2 AND (?3 IS NULL OR updated_at < ?3 OR (updated_at=?3 AND id < ?4))
             ORDER BY updated_at DESC,id DESC LIMIT ?5",
        ).map_err(map_db_error)?;
        let mut items = statement
            .query_map(
                params![
                    target_type.as_db_str(),
                    target_id,
                    time,
                    id,
                    i64::from(limit) + 1
                ],
                edit_history_from_row,
            )
            .map_err(map_db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_db_error)?;
        Ok(page(&mut items, limit, |item| P1PageCursor {
            updated_at: item.updated_at,
            id: item.id,
        }))
    }
}

fn saved_filter_from_row(row: &Row<'_>) -> rusqlite::Result<SavedFilter> {
    let filter: String = row.get(2)?;
    Ok(SavedFilter {
        id: row.get(0)?,
        name: row.get(1)?,
        filter: serde_json::from_str::<SavedAssetFilter>(&filter)
            .map_err(|_| invalid_column(2, "saved asset filter"))?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

fn custom_field_from_row(row: &Row<'_>) -> rusqlite::Result<CustomField> {
    let target_type: String = row.get(2)?;
    let value_type: String = row.get(3)?;
    let options: String = row.get(4)?;
    Ok(CustomField {
        id: row.get(0)?,
        name: row.get(1)?,
        target_type: CustomFieldTargetType::from_db_str(&target_type)
            .ok_or_else(|| invalid_column(2, "target_type"))?,
        value_type: CustomFieldValueType::from_db_str(&value_type)
            .ok_or_else(|| invalid_column(3, "value_type"))?,
        options: json_from_db(&options, 4)?,
        is_enabled: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn custom_field_value_from_row(row: &Row<'_>) -> rusqlite::Result<CustomFieldValue> {
    let target_type: String = row.get(2)?;
    let value: String = row.get(4)?;
    let status: String = row.get(5)?;
    Ok(CustomFieldValue {
        id: row.get(0)?,
        field_id: row.get(1)?,
        target_type: CustomFieldTargetType::from_db_str(&target_type)
            .ok_or_else(|| invalid_column(2, "target_type"))?,
        target_id: row.get(3)?,
        value: json_from_db(&value, 4)?,
        status: CustomFieldValueStatus::from_db_str(&status)
            .ok_or_else(|| invalid_column(5, "status"))?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn prompt_version_from_row(row: &Row<'_>) -> rusqlite::Result<PromptVersion> {
    Ok(PromptVersion {
        id: row.get(0)?,
        prompt_id: row.get(1)?,
        version: row.get(2)?,
        prompt: PromptText {
            prompt_zh: row.get(3)?,
            prompt_en: row.get(4)?,
            negative_prompt: row.get(5)?,
        },
        created_at: row.get(6)?,
    })
}

fn edit_history_from_row(row: &Row<'_>) -> rusqlite::Result<EditHistoryEntry> {
    let target_type: String = row.get(1)?;
    let action: String = row.get(3)?;
    let fields: String = row.get(4)?;
    let status: String = row.get(5)?;
    Ok(EditHistoryEntry {
        id: row.get(0)?,
        input: EditHistoryInput {
            target_type: crate::domain::EditHistoryTargetType::from_db_str(&target_type)
                .ok_or_else(|| invalid_column(1, "target_type"))?,
            target_id: row.get(2)?,
            action: crate::domain::EditAction::from_db_str(&action)
                .ok_or_else(|| invalid_column(3, "action"))?,
            changed_fields: serde_json::from_str(&fields)
                .map_err(|_| invalid_column(4, "changed_fields_json"))?,
        },
        status: EditHistoryStatus::from_db_str(&status)
            .ok_or_else(|| invalid_column(5, "status"))?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn json_from_db(value: &str, column: usize) -> rusqlite::Result<serde_json::Value> {
    serde_json::from_str(value).map_err(|_| invalid_column(column, "json"))
}

fn invalid_column(index: usize, name: &str) -> rusqlite::Error {
    rusqlite::Error::InvalidColumnType(index, name.to_owned(), Type::Text)
}

fn normalized_name(value: &str) -> Result<&str, P1LibraryRepositoryError> {
    let value = value.trim();
    if value.is_empty() {
        Err(P1LibraryRepositoryError::InvalidData)
    } else {
        Ok(value)
    }
}

fn validate_saved_asset_filter(filter: &SavedAssetFilter) -> Result<(), P1LibraryRepositoryError> {
    let valid_text = |value: &Option<String>| {
        value
            .as_ref()
            .is_none_or(|value| !value.trim().is_empty() && value.chars().count() <= 200)
    };
    let ids_valid = filter.category_ids.len() <= 100
        && filter.category_ids.iter().all(|id| *id > 0)
        && filter
            .category_ids
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            == filter.category_ids.len();
    let dates_valid = match (filter.created_after, filter.created_before) {
        (Some(start), Some(end)) => start >= 0 && end >= start,
        (Some(start), None) => start >= 0,
        (None, Some(end)) => end >= 0,
        (None, None) => true,
    };
    let aspect_valid = match (filter.min_aspect_ratio, filter.max_aspect_ratio) {
        (Some(min), Some(max)) => min.is_finite() && min > 0.0 && max.is_finite() && max >= min,
        (Some(value), None) | (None, Some(value)) => value.is_finite() && value > 0.0,
        (None, None) => true,
    };
    if filter.version != 1
        || !valid_text(&filter.keyword)
        || !valid_text(&filter.model)
        || !valid_text(&filter.platform)
        || !ids_valid
        || filter.rating.is_some_and(|rating| rating > 5)
        || !dates_valid
        || !aspect_valid
    {
        return Err(P1LibraryRepositoryError::InvalidData);
    }
    Ok(())
}

fn normalized_field_names(fields: &[String]) -> Result<Vec<String>, P1LibraryRepositoryError> {
    if fields.is_empty() {
        return Err(P1LibraryRepositoryError::InvalidData);
    }
    let mut normalized = Vec::with_capacity(fields.len());
    for field in fields {
        let field = field.trim();
        if field.is_empty()
            || field.len() > 64
            || field.chars().any(char::is_whitespace)
            || field.chars().any(char::is_control)
        {
            return Err(P1LibraryRepositoryError::InvalidData);
        }
        normalized.push(field.to_owned());
    }
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

fn validate_limit(limit: u32) -> Result<(), P1LibraryRepositoryError> {
    if (MIN_PAGE_LIMIT..=MAX_PAGE_LIMIT).contains(&limit) {
        Ok(())
    } else {
        Err(P1LibraryRepositoryError::InvalidData)
    }
}

fn page<T>(
    items: &mut Vec<T>,
    limit: u32,
    cursor: impl Fn(&T) -> P1PageCursor,
) -> crate::domain::Page<T> {
    let next_cursor = if items.len() > limit as usize {
        items.pop();
        items.last().map(cursor)
    } else {
        None
    };
    crate::domain::Page {
        items: std::mem::take(items),
        next_cursor,
    }
}

fn ensure_changed(changed: usize) -> Result<(), P1LibraryRepositoryError> {
    if changed == 0 {
        Err(P1LibraryRepositoryError::NotFound)
    } else {
        Ok(())
    }
}

fn map_db_error(error: rusqlite::Error) -> P1LibraryRepositoryError {
    match error {
        rusqlite::Error::SqliteFailure(code, _)
            if code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
                || code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY
                || code.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            P1LibraryRepositoryError::Conflict
        }
        rusqlite::Error::FromSqlConversionFailure(..)
        | rusqlite::Error::IntegralValueOutOfRange(..) => P1LibraryRepositoryError::InvalidData,
        _ => P1LibraryRepositoryError::DatabaseFailed,
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::{Connection, params};
    use serde_json::json;

    use super::{P1LibraryRepository, P1LibraryRepositoryError};
    use crate::domain::{
        CustomFieldTargetType, CustomFieldValueInput, CustomFieldValueStatus, CustomFieldValueType,
        EditAction, EditHistoryInput, EditHistoryStatus, EditHistoryTargetType, PromptText,
        SaveCustomField, SaveSavedFilter, SavedAssetFilter,
    };

    fn database() -> Connection {
        let connection = Connection::open_in_memory().expect("应能打开内存数据库");
        connection
            .pragma_update(None, "foreign_keys", true)
            .expect("应能启用外键");
        for sql in [
            include_str!("../migrations/0001_initial.sql"),
            include_str!("../migrations/0002_search.sql"),
            include_str!("../migrations/0003_ai.sql"),
            include_str!("../migrations/0004_p1_library.sql"),
        ] {
            connection.execute_batch(sql).expect("应能按顺序建立迁移");
        }
        connection
    }

    fn project(connection: &Connection) -> i64 {
        connection
            .execute(
                "INSERT INTO projects(title,created_at,updated_at) VALUES('测试项目',1,1)",
                [],
            )
            .expect("应能创建项目");
        connection.last_insert_rowid()
    }

    fn prompt(connection: &Connection, project_id: i64) -> i64 {
        connection
            .execute(
                "INSERT INTO prompts(project_id,created_at,updated_at) VALUES(?1,1,1)",
                [project_id],
            )
            .expect("应能创建提示词");
        connection.last_insert_rowid()
    }

    #[test]
    fn saved_filters_are_json_objects_and_use_keyset_pagination() {
        let mut connection = database();
        for (name, now) in [("第一组", 1), ("第二组", 2)] {
            P1LibraryRepository::save_saved_filter(
                &mut connection,
                &SaveSavedFilter {
                    id: None,
                    name: name.to_owned(),
                    filter: SavedAssetFilter {
                        rating: Some(5),
                        ..SavedAssetFilter::default()
                    },
                },
                now,
            )
            .expect("应能保存筛选");
        }
        let first =
            P1LibraryRepository::list_saved_filters(&connection, None, 1).expect("应能分页读取");
        assert_eq!(first.items[0].name, "第二组");
        let second = P1LibraryRepository::list_saved_filters(&connection, first.next_cursor, 1)
            .expect("应能读取下一页");
        assert_eq!(second.items[0].name, "第一组");
        assert_eq!(
            P1LibraryRepository::save_saved_filter(
                &mut connection,
                &SaveSavedFilter {
                    id: None,
                    name: "错误".to_owned(),
                    filter: SavedAssetFilter {
                        version: 99,
                        ..SavedAssetFilter::default()
                    }
                },
                3
            ),
            Err(P1LibraryRepositoryError::InvalidData)
        );
    }

    #[test]
    fn custom_field_pending_value_stays_pending_until_confirmation_transaction() {
        let mut connection = database();
        let project_id = project(&connection);
        let field = P1LibraryRepository::save_custom_field(
            &mut connection,
            &SaveCustomField {
                id: None,
                name: "构图评分".to_owned(),
                target_type: CustomFieldTargetType::Project,
                value_type: CustomFieldValueType::Number,
                options: json!({"min": 0, "max": 10}),
                is_enabled: true,
            },
            2,
        )
        .expect("应能保存字段");
        let input = CustomFieldValueInput {
            field_id: field.id,
            target_type: CustomFieldTargetType::Project,
            target_id: project_id,
            value: json!(7),
        };
        let confirmed =
            P1LibraryRepository::save_manual_custom_field_value(&mut connection, &input, 3)
                .expect("应能保存正式值");
        assert_eq!(confirmed.status, CustomFieldValueStatus::Confirmed);
        connection.execute(
            "INSERT INTO custom_field_values(field_id,target_type,target_id,value_json,source,status,created_at,updated_at) VALUES(?1,'project',?2,'9','automation','pending',4,4)",
            params![field.id, project_id],
        ).expect("自动化适配器应能保存待确认值");
        let pending_id = connection.last_insert_rowid();
        let resolved =
            P1LibraryRepository::confirm_pending_custom_field_value(&mut connection, pending_id, 5)
                .expect("确认应在单一事务内替换正式值");
        assert_eq!(resolved.status, CustomFieldValueStatus::Confirmed);
        let values = P1LibraryRepository::list_custom_field_values(
            &connection,
            CustomFieldTargetType::Project,
            project_id,
            None,
            10,
        )
        .expect("应能读取字段值");
        assert_eq!(values.items.len(), 1);
        assert_eq!(values.items[0].value, json!(9));
        assert_eq!(
            P1LibraryRepository::save_manual_custom_field_value(
                &mut connection,
                &CustomFieldValueInput {
                    field_id: field.id,
                    target_type: CustomFieldTargetType::Project,
                    target_id: project_id,
                    value: json!("错误类型")
                },
                6
            ),
            Err(P1LibraryRepositoryError::Conflict)
        );
    }

    #[test]
    fn custom_field_values_can_be_batched_and_keyset_paginated() {
        let mut connection = database();
        let first_project_id = project(&connection);
        let second_project_id = project(&connection);
        let field = P1LibraryRepository::save_custom_field(
            &mut connection,
            &SaveCustomField {
                id: None,
                name: "用途".to_owned(),
                target_type: CustomFieldTargetType::Project,
                value_type: CustomFieldValueType::Text,
                options: json!({}),
                is_enabled: true,
            },
            1,
        )
        .expect("应能保存自定义字段");
        for (target_id, value, updated_at) in [
            (first_project_id, "人物设定", 2),
            (second_project_id, "场景参考", 3),
        ] {
            P1LibraryRepository::save_manual_custom_field_value(
                &mut connection,
                &CustomFieldValueInput {
                    field_id: field.id,
                    target_type: CustomFieldTargetType::Project,
                    target_id,
                    value: json!(value),
                },
                updated_at,
            )
            .expect("应能保存项目自定义字段值");
        }

        let target_ids = [first_project_id, second_project_id];
        let first = P1LibraryRepository::list_custom_field_values_for_targets(
            &connection,
            CustomFieldTargetType::Project,
            &target_ids,
            None,
            1,
        )
        .expect("应能批量分页读取自定义字段值");
        assert_eq!(first.items.len(), 1);
        assert_eq!(first.items[0].target_id, second_project_id);
        let second = P1LibraryRepository::list_custom_field_values_for_targets(
            &connection,
            CustomFieldTargetType::Project,
            &target_ids,
            first.next_cursor,
            1,
        )
        .expect("应能读取下一页");
        assert_eq!(second.items.len(), 1);
        assert_eq!(second.items[0].target_id, first_project_id);
        assert_eq!(second.items[0].value, json!("人物设定"));
    }

    #[test]
    fn prompt_versions_are_immutable_and_paginated_by_version() {
        let mut connection = database();
        let prompt_id = prompt(&connection, project(&connection));
        for (value, now) in [("v1", 2), ("v2", 3)] {
            P1LibraryRepository::create_prompt_version(
                &mut connection,
                prompt_id,
                &PromptText {
                    prompt_zh: value.to_owned(),
                    prompt_en: String::new(),
                    negative_prompt: String::new(),
                },
                now,
            )
            .expect("应能建立提示词版本");
        }
        let first = P1LibraryRepository::list_prompt_versions(&connection, prompt_id, None, 1)
            .expect("应能读取首个版本");
        assert_eq!(first.items[0].version, 2);
        assert_eq!(first.items[0].prompt.prompt_zh, "v2");
        let second =
            P1LibraryRepository::list_prompt_versions(&connection, prompt_id, first.next_cursor, 1)
                .expect("应能读取旧版本");
        assert_eq!(second.items[0].version, 1);
        assert_eq!(second.items[0].prompt.prompt_zh, "v1");
        let history_count: i64 = connection
            .query_row(
                "SELECT count(*) FROM edit_history WHERE target_type='prompt' AND target_id=?1",
                [prompt_id],
                |row| row.get(0),
            )
            .expect("应能读取提示词历史");
        assert_eq!(history_count, 0, "创建不可变快照不应虚报正式提示词更新");
    }

    #[test]
    fn manual_edit_history_never_stores_prompt_body() {
        let connection = database();
        let project_id = project(&connection);
        let input = EditHistoryInput {
            target_type: EditHistoryTargetType::Project,
            target_id: project_id,
            action: EditAction::Update,
            changed_fields: vec!["prompt_zh".to_owned(), "rating".to_owned()],
        };
        let confirmed = P1LibraryRepository::record_manual_edit(&connection, &input, 2)
            .expect("应能记录人工确认的编辑");
        assert_eq!(confirmed.status, EditHistoryStatus::Confirmed);
        let rows = P1LibraryRepository::list_edit_history(
            &connection,
            EditHistoryTargetType::Project,
            project_id,
            None,
            10,
        )
        .expect("应能分页读取历史");
        let serialized = serde_json::to_string(&rows).expect("应能序列化历史");
        assert!(!serialized.contains("完整提示词"));
        assert_eq!(
            P1LibraryRepository::record_manual_edit(
                &connection,
                &EditHistoryInput {
                    changed_fields: vec!["包含 空格".to_owned()],
                    ..input
                },
                4
            ),
            Err(P1LibraryRepositoryError::InvalidData)
        );
        let invalid_source = connection.execute(
            "INSERT INTO edit_history(target_type,target_id,action,changed_fields_json,source,status,created_at,updated_at) VALUES('project',?1,'update','[]','automation','confirmed',5,5)",
            params![project_id],
        );
        assert!(
            invalid_source.is_err(),
            "数据库也必须拒绝绕过仓储的自动化正式写入"
        );
    }
}
