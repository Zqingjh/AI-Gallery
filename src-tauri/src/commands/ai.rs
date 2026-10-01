use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{
    commands::CommandErrorDto,
    domain::{
        AiInputScope, AiProviderCapabilities, AiProviderConfig, AiProviderKind, AiSuggestionTarget,
        AiSuggestionTargetType, AiSuggestionView, BatchAiSendPreview, BatchAiSuggestionCreated,
        BatchAiSuggestionFailure, BatchAiSuggestionFailureKind, BatchAiSuggestionsResult,
        ClassifyRequest, SaveAiProviderConfig, SuggestionCursor, SuggestionImpact,
        SuggestionResolution,
    },
    services::{AiSendPreview, AiService, AiServiceError},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AiRootRequestDto {
    root_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AiProviderSaveRequestDto {
    root_path: String,
    id: Option<i64>,
    input: AiProviderInputDto,
    api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AiProviderInputDto {
    kind: AiProviderKind,
    display_name: String,
    endpoint: String,
    model: String,
    capabilities: AiProviderCapabilities,
    timeout_ms: u32,
    is_enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AiEntityRequestDto {
    root_path: String,
    id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AiClassifyRequestDto {
    root_path: String,
    #[serde(default)]
    provider_id: Option<i64>,
    target_type: AiSuggestionTargetType,
    target_id: i64,
    input_scope: AiInputScope,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AiBatchSendPreviewRequestDto {
    root_path: String,
    asset_display_numbers: Vec<i64>,
    input_scope: AiInputScope,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AiBatchClassifyRequestDto {
    root_path: String,
    provider_id: i64,
    asset_display_numbers: Vec<i64>,
    input_scope: AiInputScope,
    confirmed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AiSuggestionPageRequestDto {
    root_path: String,
    cursor: Option<String>,
    limit: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AiResolveSuggestionRequestDto {
    root_path: String,
    id: i64,
    resolution: SuggestionResolutionDto,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum SuggestionResolutionDto {
    Accept,
    AcceptExisting { category_id: String },
    CreateCategory { name: String },
    MergeIntoExisting { category_id: String },
    ConvertToTag { name: String },
    Reject,
}

impl TryFrom<SuggestionResolutionDto> for SuggestionResolution {
    type Error = CommandErrorDto;

    fn try_from(value: SuggestionResolutionDto) -> Result<Self, Self::Error> {
        let parse_category_id = |value: String| {
            value
                .parse::<i64>()
                .ok()
                .filter(|id| *id > 0)
                .ok_or_else(|| CommandErrorDto::from(AiServiceError::InvalidInput))
        };
        match value {
            SuggestionResolutionDto::Accept => Ok(Self::Accept),
            SuggestionResolutionDto::AcceptExisting { category_id } => Ok(Self::AcceptExisting {
                category_id: parse_category_id(category_id)?,
            }),
            SuggestionResolutionDto::CreateCategory { name } => Ok(Self::CreateCategory { name }),
            SuggestionResolutionDto::MergeIntoExisting { category_id } => {
                Ok(Self::MergeIntoExisting {
                    category_id: parse_category_id(category_id)?,
                })
            }
            SuggestionResolutionDto::ConvertToTag { name } => Ok(Self::ConvertToTag { name }),
            SuggestionResolutionDto::Reject => Ok(Self::Reject),
        }
    }
}

impl From<AiServiceError> for CommandErrorDto {
    fn from(error: AiServiceError) -> Self {
        match error {
            AiServiceError::InvalidInput => {
                Self::new("AI_INVALID_INPUT", "AI 配置或请求无法识别。")
            }
            AiServiceError::NotFound => Self::new("AI_NOT_FOUND", "未找到指定的 AI 配置或建议。"),
            AiServiceError::Conflict => Self::new("AI_CONFLICT", "当前 AI 建议无法按该方式处理。"),
            AiServiceError::DatabaseUnavailable => {
                Self::new("AI_DATABASE_UNAVAILABLE", "AI 本地数据暂时不可用。")
            }
            AiServiceError::CredentialUnavailable => Self::new(
                "AI_CREDENTIAL_UNAVAILABLE",
                "请在本机安全存储中重新配置密钥。",
            ),
            AiServiceError::RequestFailed => {
                Self::new("AI_REQUEST_FAILED", "AI 服务请求未完成，请检查连接与配置。")
            }
            AiServiceError::ResponseInvalid => {
                Self::new("AI_RESPONSE_INVALID", "AI 服务返回了无法审核的建议。")
            }
            AiServiceError::ReadOnly => Self::new(
                "WORKSPACE_READ_ONLY",
                "工作区正处于只读展示模式，无法修改内容。",
            ),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiProviderDto {
    id: String,
    kind: &'static str,
    display_name: String,
    endpoint: String,
    model: String,
    capabilities: AiProviderCapabilitiesDto,
    timeout_ms: u32,
    is_enabled: bool,
    needs_credential: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiProviderCapabilitiesDto {
    text_classification: bool,
    vision: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiSendPreviewDto {
    target_type: &'static str,
    target_id: String,
    fields: Vec<&'static str>,
    field_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiBatchSendPreviewDto {
    target_count: usize,
    field_names: Vec<String>,
    taxonomy_dimension_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiBatchSuggestionsResultDto {
    created: Vec<AiBatchSuggestionCreatedDto>,
    failed: Vec<AiBatchSuggestionFailureDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiBatchSuggestionCreatedDto {
    target: AiBatchTargetDto,
    suggestion_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiBatchSuggestionFailureDto {
    target: AiBatchTargetDto,
    kind: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiBatchTargetDto {
    r#type: &'static str,
    id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiSuggestionPageDto {
    items: Vec<AiSuggestionDto>,
    next_cursor: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiSuggestionImpactDto {
    suggestion_id: String,
    target_type: &'static str,
    target_id: String,
    dimension_id: String,
    allows_multiple: bool,
    existing_category_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiSuggestionDto {
    id: String,
    target: AiSuggestionTargetDto,
    dimension: AiSuggestionDimensionDto,
    category: Option<AiSuggestionCategoryDto>,
    suggested_category_name: Option<String>,
    confidence: f64,
    reason: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiSuggestionTargetDto {
    r#type: &'static str,
    id: String,
    title: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiSuggestionDimensionDto {
    id: String,
    name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiSuggestionCategoryDto {
    id: String,
    name: String,
}

#[tauri::command]
pub(crate) fn ai_list_providers(
    request: AiRootRequestDto,
) -> Result<Vec<AiProviderDto>, CommandErrorDto> {
    let service = AiService::new();
    service
        .list_providers(Path::new(&request.root_path))
        .map(|providers| {
            providers
                .iter()
                .map(|provider| provider_dto(&service, provider))
                .collect()
        })
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn ai_save_provider(
    request: AiProviderSaveRequestDto,
) -> Result<AiProviderDto, CommandErrorDto> {
    let service = AiService::new();
    let input = SaveAiProviderConfig {
        id: request.id,
        kind: request.input.kind,
        display_name: request.input.display_name,
        endpoint: request.input.endpoint,
        model: request.input.model,
        capabilities: request.input.capabilities,
        timeout_ms: request.input.timeout_ms,
        credential_id: None,
        is_enabled: request.input.is_enabled,
    };
    service
        .save_provider(
            Path::new(&request.root_path),
            &input,
            request.api_key.as_deref(),
        )
        .map(|provider| provider_dto(&service, &provider))
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn ai_delete_provider(request: AiEntityRequestDto) -> Result<(), CommandErrorDto> {
    AiService::new()
        .delete_provider(Path::new(&request.root_path), request.id)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn ai_test_provider_connection(
    request: AiEntityRequestDto,
) -> Result<(), CommandErrorDto> {
    AiService::new()
        .test_provider_connection(Path::new(&request.root_path), request.id)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn ai_get_send_preview(
    request: AiClassifyRequestDto,
) -> Result<AiSendPreviewDto, CommandErrorDto> {
    let classify = classify_request(&request);
    AiService::new()
        .send_preview(Path::new(&request.root_path), &classify)
        .map(|preview| preview_dto(&classify, preview))
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn ai_get_batch_send_preview(
    request: AiBatchSendPreviewRequestDto,
) -> Result<AiBatchSendPreviewDto, CommandErrorDto> {
    AiService::new()
        .batch_send_preview_by_asset_display_numbers(
            Path::new(&request.root_path),
            &request.asset_display_numbers,
            &request.input_scope,
        )
        .map(batch_preview_dto)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn ai_create_classification_suggestions_batch(
    request: AiBatchClassifyRequestDto,
) -> Result<AiBatchSuggestionsResultDto, CommandErrorDto> {
    if !request.confirmed {
        return Err(CommandErrorDto::from(AiServiceError::InvalidInput));
    }
    AiService::new()
        .create_classification_suggestions_batch_by_asset_display_numbers(
            Path::new(&request.root_path),
            request.provider_id,
            &request.asset_display_numbers,
            &request.input_scope,
        )
        .map(batch_result_dto)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn ai_create_classification_suggestions(
    request: AiClassifyRequestDto,
) -> Result<(), CommandErrorDto> {
    let provider_id = request
        .provider_id
        .ok_or_else(|| CommandErrorDto::from(AiServiceError::InvalidInput))?;
    let classify = classify_request(&request);
    AiService::new()
        .create_classification_suggestions(Path::new(&request.root_path), provider_id, &classify)
        .map(|_| ())
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn ai_list_pending_suggestions(
    request: AiSuggestionPageRequestDto,
) -> Result<AiSuggestionPageDto, CommandErrorDto> {
    let cursor = request.cursor.as_deref().map(decode_cursor).transpose()?;
    AiService::new()
        .list_pending_suggestion_views(Path::new(&request.root_path), cursor, request.limit)
        .map(|page| AiSuggestionPageDto {
            items: page.items.into_iter().map(suggestion_dto).collect(),
            next_cursor: page.next_cursor.map(encode_cursor),
        })
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn ai_get_suggestion_impact(
    request: AiEntityRequestDto,
) -> Result<AiSuggestionImpactDto, CommandErrorDto> {
    AiService::new()
        .suggestion_impact(Path::new(&request.root_path), request.id)
        .map(impact_dto)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn ai_resolve_suggestion(
    request: AiResolveSuggestionRequestDto,
) -> Result<(), CommandErrorDto> {
    let resolution = SuggestionResolution::try_from(request.resolution)?;
    AiService::new()
        .resolve_suggestion(Path::new(&request.root_path), request.id, &resolution)
        .map(|_| ())
        .map_err(CommandErrorDto::from)
}

fn provider_dto(service: &AiService, provider: &AiProviderConfig) -> AiProviderDto {
    let needs_credential = service.provider_needs_credential(provider);
    AiProviderDto {
        id: provider.id.to_string(),
        kind: provider_kind(provider.kind),
        display_name: provider.display_name.clone(),
        endpoint: provider.endpoint.clone(),
        model: provider.model.clone(),
        capabilities: AiProviderCapabilitiesDto {
            text_classification: provider.capabilities.classification,
            vision: false,
        },
        timeout_ms: provider.timeout_ms,
        is_enabled: provider.is_enabled,
        needs_credential,
    }
}

fn classify_request(request: &AiClassifyRequestDto) -> ClassifyRequest {
    ClassifyRequest {
        target: AiSuggestionTarget {
            target_type: request.target_type,
            target_id: request.target_id,
        },
        input_scope: request.input_scope.clone(),
    }
}

fn preview_dto(request: &ClassifyRequest, preview: AiSendPreview) -> AiSendPreviewDto {
    let fields = preview.field_names;
    AiSendPreviewDto {
        target_type: target_type(request.target.target_type),
        target_id: request.target.target_id.to_string(),
        field_count: fields.len(),
        fields,
    }
}

fn batch_preview_dto(preview: BatchAiSendPreview) -> AiBatchSendPreviewDto {
    AiBatchSendPreviewDto {
        target_count: preview.target_count,
        field_names: preview.field_names,
        taxonomy_dimension_count: preview.taxonomy_dimension_count,
    }
}

fn batch_result_dto(result: BatchAiSuggestionsResult) -> AiBatchSuggestionsResultDto {
    AiBatchSuggestionsResultDto {
        created: result.created.into_iter().map(batch_created_dto).collect(),
        failed: result.failed.into_iter().map(batch_failure_dto).collect(),
    }
}

fn batch_created_dto(value: BatchAiSuggestionCreated) -> AiBatchSuggestionCreatedDto {
    AiBatchSuggestionCreatedDto {
        target: batch_target_dto(value.target),
        suggestion_ids: value
            .suggestion_ids
            .into_iter()
            .map(|id| id.to_string())
            .collect(),
    }
}

fn batch_failure_dto(value: BatchAiSuggestionFailure) -> AiBatchSuggestionFailureDto {
    AiBatchSuggestionFailureDto {
        target: batch_target_dto(value.target),
        kind: batch_failure_kind(value.kind),
    }
}

fn batch_target_dto(target: AiSuggestionTarget) -> AiBatchTargetDto {
    AiBatchTargetDto {
        r#type: target_type(target.target_type),
        id: target.target_id.to_string(),
    }
}

fn batch_failure_kind(value: BatchAiSuggestionFailureKind) -> &'static str {
    match value {
        BatchAiSuggestionFailureKind::InputUnavailable => "input_unavailable",
        BatchAiSuggestionFailureKind::RequestFailed => "request_failed",
        BatchAiSuggestionFailureKind::ResponseInvalid => "response_invalid",
        BatchAiSuggestionFailureKind::PersistFailed => "persist_failed",
    }
}

fn suggestion_dto(value: AiSuggestionView) -> AiSuggestionDto {
    let suggestion = value.suggestion;
    AiSuggestionDto {
        id: suggestion.id.to_string(),
        target: AiSuggestionTargetDto {
            r#type: target_type(suggestion.target.target_type),
            id: suggestion.target.target_id.to_string(),
            title: value.target_title,
        },
        dimension: AiSuggestionDimensionDto {
            id: suggestion.dimension_id.to_string(),
            name: value.dimension_name,
        },
        category: value.category.map(|category| AiSuggestionCategoryDto {
            id: category.id.to_string(),
            name: category.name,
        }),
        suggested_category_name: suggestion.suggested_category_name,
        confidence: suggestion.confidence.unwrap_or(0.0),
        reason: suggestion.reason,
        updated_at: suggestion.updated_at.to_string(),
    }
}

fn impact_dto(value: SuggestionImpact) -> AiSuggestionImpactDto {
    AiSuggestionImpactDto {
        suggestion_id: value.suggestion_id.to_string(),
        target_type: target_type(value.target.target_type),
        target_id: value.target.target_id.to_string(),
        dimension_id: value.dimension_id.to_string(),
        allows_multiple: value.allows_multiple,
        existing_category_ids: value
            .existing_category_ids
            .into_iter()
            .map(|id| id.to_string())
            .collect(),
    }
}

fn provider_kind(kind: AiProviderKind) -> &'static str {
    match kind {
        AiProviderKind::OpenaiCompatible => "openaiCompatible",
        AiProviderKind::Gemini => "gemini",
        AiProviderKind::Ollama => "ollama",
    }
}

fn target_type(value: AiSuggestionTargetType) -> &'static str {
    match value {
        AiSuggestionTargetType::Project => "project",
        AiSuggestionTargetType::Asset => "asset",
    }
}

fn encode_cursor(cursor: SuggestionCursor) -> String {
    format!("{}:{}", cursor.updated_at, cursor.id)
}

fn decode_cursor(value: &str) -> Result<SuggestionCursor, CommandErrorDto> {
    let (updated_at, id) = value
        .split_once(':')
        .ok_or_else(|| CommandErrorDto::from(AiServiceError::InvalidInput))?;
    let updated_at = updated_at
        .parse::<i64>()
        .ok()
        .filter(|number| *number >= 0)
        .ok_or_else(|| CommandErrorDto::from(AiServiceError::InvalidInput))?;
    let id = id
        .parse::<i64>()
        .ok()
        .filter(|number| *number > 0)
        .ok_or_else(|| CommandErrorDto::from(AiServiceError::InvalidInput))?;
    Ok(SuggestionCursor { updated_at, id })
}

#[cfg(test)]
mod tests {
    use super::{decode_cursor, encode_cursor};
    use crate::domain::SuggestionCursor;

    #[test]
    fn cursor_round_trip_uses_opaque_frontend_string() {
        let cursor = SuggestionCursor {
            updated_at: 123,
            id: 4,
        };
        assert_eq!(decode_cursor(&encode_cursor(cursor)).unwrap(), cursor);
        assert!(decode_cursor("not-a-cursor").is_err());
    }
}

#[cfg(test)]
mod read_only_tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::{AiProviderInputDto, AiProviderSaveRequestDto, ai_save_provider};
    use crate::{
        domain::{AccessMode, AiProviderCapabilities, AiProviderKind},
        services::{AccessModeService, DatabaseService, WorkspaceService},
    };

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "ai-gallery-ai-read-only-{}-{}",
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
    fn ai_write_command_rejects_read_only_workspace_before_provider_persistence() {
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

        let error = ai_save_provider(AiProviderSaveRequestDto {
            root_path: root.to_string_lossy().into_owned(),
            id: None,
            input: AiProviderInputDto {
                kind: AiProviderKind::Ollama,
                display_name: "本地模型".to_owned(),
                endpoint: "http://127.0.0.1:11434".to_owned(),
                model: "llama3".to_owned(),
                capabilities: AiProviderCapabilities {
                    classification: true,
                },
                timeout_ms: 5_000,
                is_enabled: true,
            },
            api_key: None,
        })
        .expect_err("只读模式不得绕过 AI 配置写入命令");

        assert_eq!(error.code, "WORKSPACE_READ_ONLY");
        let table_count: i64 = rusqlite::Connection::open(root.join("data/library.sqlite3"))
            .expect("应能打开数据库")
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'ai_provider_configs'",
                [],
                |row| row.get(0),
            )
            .expect("应能检查 AI 配置表");
        assert_eq!(table_count, 0, "拒绝后不得触发 AI 配置写入");
    }
}

#[cfg(test)]
mod batch_command_dto_tests {
    use super::{
        AiBatchClassifyRequestDto, AiBatchSendPreviewRequestDto,
        ai_create_classification_suggestions_batch, batch_preview_dto, batch_result_dto,
    };
    use crate::domain::{
        AiSuggestionTarget, AiSuggestionTargetType, BatchAiSendPreview, BatchAiSuggestionCreated,
        BatchAiSuggestionFailure, BatchAiSuggestionFailureKind, BatchAiSuggestionsResult,
    };

    const INPUT_SCOPE: &str = r#"{
        "title":true,
        "promptZh":true,
        "promptEn":false,
        "negativePrompt":false
    }"#;

    #[test]
    fn batch_ai_dto_rejects_unknown_sensitive_fields_and_missing_confirmation() {
        let unknown = format!(
            r#"{{
                "rootPath":"workspace",
                "assetDisplayNumbers":[1],
                "filePath":"secret",
                "inputScope":{}
            }}"#,
            INPUT_SCOPE
        );
        let missing_confirmation = format!(
            r#"{{
                "rootPath":"workspace",
                "providerId":1,
                "assetDisplayNumbers":[1],
                "inputScope":{}
            }}"#,
            INPUT_SCOPE
        );

        assert!(serde_json::from_str::<AiBatchSendPreviewRequestDto>(&unknown).is_err());
        assert!(serde_json::from_str::<AiBatchClassifyRequestDto>(&missing_confirmation).is_err());
    }

    #[test]
    fn batch_ai_execution_rejects_false_confirmation_before_accessing_workspace() {
        let request = format!(
            r#"{{
                "rootPath":"Z:\\private\\workspace",
                "providerId":1,
                "assetDisplayNumbers":[1],
                "inputScope":{},
                "confirmed":false
            }}"#,
            INPUT_SCOPE
        );
        let request = serde_json::from_str::<AiBatchClassifyRequestDto>(&request)
            .expect("合法请求应能反序列化");

        let error =
            ai_create_classification_suggestions_batch(request).expect_err("未确认必须拒绝");
        assert_eq!(error.code, "AI_INVALID_INPUT");
        assert!(!error.message.contains("private"));
    }

    #[test]
    fn batch_ai_result_contains_only_safe_ids_and_failure_categories() {
        let result = batch_result_dto(BatchAiSuggestionsResult {
            created: vec![BatchAiSuggestionCreated {
                target: AiSuggestionTarget {
                    target_type: AiSuggestionTargetType::Asset,
                    target_id: 7,
                },
                suggestion_ids: vec![11, 12],
            }],
            failed: vec![BatchAiSuggestionFailure {
                target: AiSuggestionTarget {
                    target_type: AiSuggestionTargetType::Project,
                    target_id: 8,
                },
                kind: BatchAiSuggestionFailureKind::RequestFailed,
            }],
        });
        let value = serde_json::to_value(result).expect("批量结果应可序列化");
        let text = value.to_string();

        assert_eq!(value["created"][0]["suggestionIds"][0], "11");
        assert_eq!(value["failed"][0]["kind"], "request_failed");
        assert!(!text.contains("prompt"));
        assert!(!text.contains("provider"));
        assert!(!text.contains("path"));
    }

    #[test]
    fn batch_preview_uses_the_frontend_field_names_contract() {
        let value = serde_json::to_value(batch_preview_dto(BatchAiSendPreview {
            target_count: 1,
            field_names: vec!["promptZh".to_owned()],
            taxonomy_dimension_count: 2,
        }))
        .expect("批量预览应可序列化");

        assert_eq!(value["fieldNames"], serde_json::json!(["promptZh"]));
        assert!(value.get("fields").is_none());
    }
}
