use std::path::Path;

use serde::Deserialize;

use crate::{
    commands::CommandErrorDto,
    domain::{
        CustomField, CustomFieldPage, CustomFieldTargetType, CustomFieldValue,
        CustomFieldValueInput, CustomFieldValuePage, EditHistoryPage, EditHistoryTargetType,
        P1PageCursor, PromptText, PromptVersion, PromptVersionCursor, PromptVersionPage,
        SaveCustomField, SaveSavedFilter, SavedFilter, SavedFilterPage,
    },
    services::{P1LibraryService, P1LibraryServiceError},
};

/// 所有 P1 作品库命令均由服务层负责校验、权限保护和持久化。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SaveSavedFilterRequestDto {
    root_path: String,
    input: SaveSavedFilter,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SavedFilterPageRequestDto {
    root_path: String,
    cursor: Option<P1PageCursor>,
    limit: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct P1EntityRequestDto {
    root_path: String,
    id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SaveCustomFieldRequestDto {
    root_path: String,
    input: SaveCustomField,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CustomFieldPageRequestDto {
    root_path: String,
    target_type: CustomFieldTargetType,
    cursor: Option<P1PageCursor>,
    limit: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SaveManualCustomFieldValueRequestDto {
    root_path: String,
    input: CustomFieldValueInput,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CustomFieldValuePageRequestDto {
    root_path: String,
    target_type: CustomFieldTargetType,
    target_id: i64,
    cursor: Option<P1PageCursor>,
    limit: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CreatePromptVersionRequestDto {
    root_path: String,
    prompt_id: i64,
    prompt: PromptText,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PromptVersionPageRequestDto {
    root_path: String,
    prompt_id: i64,
    cursor: Option<PromptVersionCursor>,
    limit: u32,
}

/// 编辑历史只包含服务层筛选后的字段名，绝不携带本地路径或完整提示词。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EditHistoryPageRequestDto {
    root_path: String,
    target_type: EditHistoryTargetType,
    target_id: i64,
    cursor: Option<P1PageCursor>,
    limit: u32,
}

impl From<P1LibraryServiceError> for CommandErrorDto {
    fn from(error: P1LibraryServiceError) -> Self {
        match error {
            P1LibraryServiceError::InvalidInput => {
                Self::new("P1_LIBRARY_INVALID_INPUT", "提交的作品库增强数据无法识别。")
            }
            P1LibraryServiceError::NotFound => {
                Self::new("P1_LIBRARY_NOT_FOUND", "未找到指定的作品库增强记录。")
            }
            P1LibraryServiceError::Conflict => Self::new(
                "P1_LIBRARY_CONFLICT",
                "当前作品库增强数据无法按该方式保存。",
            ),
            P1LibraryServiceError::DatabaseUnavailable => Self::new(
                "P1_LIBRARY_DATABASE_UNAVAILABLE",
                "作品库增强数据暂时不可用。",
            ),
            P1LibraryServiceError::DataInvalid => {
                Self::new("P1_LIBRARY_DATA_INVALID", "作品库增强数据不完整或已损坏。")
            }
            P1LibraryServiceError::ReadOnly => Self::new(
                "WORKSPACE_READ_ONLY",
                "工作区正处于只读展示模式，无法修改内容。",
            ),
        }
    }
}

#[tauri::command]
pub(crate) fn p1_library_save_saved_filter(
    request: SaveSavedFilterRequestDto,
) -> Result<SavedFilter, CommandErrorDto> {
    P1LibraryService::new()
        .save_saved_filter(Path::new(&request.root_path), &request.input)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn p1_library_list_saved_filters(
    request: SavedFilterPageRequestDto,
) -> Result<SavedFilterPage, CommandErrorDto> {
    P1LibraryService::new()
        .list_saved_filters(Path::new(&request.root_path), request.cursor, request.limit)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn p1_library_delete_saved_filter(
    request: P1EntityRequestDto,
) -> Result<(), CommandErrorDto> {
    P1LibraryService::new()
        .delete_saved_filter(Path::new(&request.root_path), request.id)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn p1_library_save_custom_field(
    request: SaveCustomFieldRequestDto,
) -> Result<CustomField, CommandErrorDto> {
    P1LibraryService::new()
        .save_custom_field(Path::new(&request.root_path), &request.input)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn p1_library_list_custom_fields(
    request: CustomFieldPageRequestDto,
) -> Result<CustomFieldPage, CommandErrorDto> {
    P1LibraryService::new()
        .list_custom_fields(
            Path::new(&request.root_path),
            request.target_type,
            request.cursor,
            request.limit,
        )
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn p1_library_save_manual_custom_field_value(
    request: SaveManualCustomFieldValueRequestDto,
) -> Result<CustomFieldValue, CommandErrorDto> {
    P1LibraryService::new()
        .save_manual_custom_field_value(Path::new(&request.root_path), &request.input)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn p1_library_confirm_pending_custom_field_value(
    request: P1EntityRequestDto,
) -> Result<CustomFieldValue, CommandErrorDto> {
    P1LibraryService::new()
        .confirm_pending_custom_field_value(Path::new(&request.root_path), request.id)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn p1_library_list_custom_field_values(
    request: CustomFieldValuePageRequestDto,
) -> Result<CustomFieldValuePage, CommandErrorDto> {
    P1LibraryService::new()
        .list_custom_field_values(
            Path::new(&request.root_path),
            request.target_type,
            request.target_id,
            request.cursor,
            request.limit,
        )
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn p1_library_create_prompt_version(
    request: CreatePromptVersionRequestDto,
) -> Result<PromptVersion, CommandErrorDto> {
    P1LibraryService::new()
        .create_prompt_version(
            Path::new(&request.root_path),
            request.prompt_id,
            &request.prompt,
        )
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn p1_library_list_prompt_versions(
    request: PromptVersionPageRequestDto,
) -> Result<PromptVersionPage, CommandErrorDto> {
    P1LibraryService::new()
        .list_prompt_versions(
            Path::new(&request.root_path),
            request.prompt_id,
            request.cursor,
            request.limit,
        )
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn p1_library_list_edit_history(
    request: EditHistoryPageRequestDto,
) -> Result<EditHistoryPage, CommandErrorDto> {
    P1LibraryService::new()
        .list_edit_history(
            Path::new(&request.root_path),
            request.target_type,
            request.target_id,
            request.cursor,
            request.limit,
        )
        .map_err(CommandErrorDto::from)
}

#[cfg(test)]
mod tests {
    use super::{EditHistoryPageRequestDto, SaveSavedFilterRequestDto};

    #[test]
    fn saved_filter_request_rejects_unknown_fields() {
        let request = r#"{
            "rootPath":"workspace",
            "input":{"name":"收藏","filter":{"version":1}},
            "unexpected":true
        }"#;

        assert!(serde_json::from_str::<SaveSavedFilterRequestDto>(request).is_err());
    }

    #[test]
    fn saved_filter_rejects_unversioned_or_unknown_filter_fields() {
        let unversioned = r#"{
            "rootPath":"workspace",
            "input":{"name":"收藏","filter":{"isFavorite":true}}
        }"#;
        let leaking_field = r#"{
            "rootPath":"workspace",
            "input":{"name":"收藏","filter":{"version":1,"storedPath":"private/file.png"}}
        }"#;

        assert!(serde_json::from_str::<SaveSavedFilterRequestDto>(unversioned).is_err());
        assert!(serde_json::from_str::<SaveSavedFilterRequestDto>(leaking_field).is_err());
    }

    #[test]
    fn edit_history_request_accepts_only_history_locator_fields() {
        let request = r#"{
            "rootPath":"workspace",
            "targetType":"asset",
            "targetId":1,
            "cursor":null,
            "limit":20,
            "prompt":"不得传入"
        }"#;

        assert!(serde_json::from_str::<EditHistoryPageRequestDto>(request).is_err());
    }
}
