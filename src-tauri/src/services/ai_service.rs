use std::{
    ops::{Deref, DerefMut},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    adapters::{
        ai_provider::{
            AiProviderAdapterError, ProviderAdapterRegistry, ProviderCategory,
            ProviderClassificationContent, ProviderClassificationRequest, ProviderDimension,
        },
        secure_credential_store::{
            SecureCredentialStore, SecureCredentialStoreError, SystemCredentialStore,
        },
    },
    domain::{
        AiInputScope, AiProviderConfig, AiProviderKind, AiSuggestion, AiSuggestionTarget,
        AiSuggestionTargetType, BatchAiSendPreview, BatchAiSuggestionCreated,
        BatchAiSuggestionFailure, BatchAiSuggestionFailureKind, BatchAiSuggestionsResult,
        BatchClassifyRequest, ClassifyRequest, CreateAiSuggestions, SaveAiProviderConfig,
        SuggestionCursor, SuggestionImpact, SuggestionResolution, SuggestionViewPage,
    },
    repositories::{AiRepository, AiRepositoryError, LibraryRepository},
    services::{
        AccessModeServiceError, DatabaseService, DatabaseServiceError, WriteAccessGuard,
        WriteAccessLease,
    },
};

const MIN_TIMEOUT_MS: u32 = 1_000;
const MAX_TIMEOUT_MS: u32 = 60_000;
const MAX_PROVIDER_TEXT_LENGTH: usize = 200;
const MAX_SUGGESTIONS_PER_REQUEST: usize = 20;
const MAX_BATCH_CLASSIFICATION_TARGETS: usize = 100;
const MIN_PAGE_LIMIT: u32 = 1;
const MAX_PAGE_LIMIT: u32 = 100;
static NEXT_CREDENTIAL_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiServiceError {
    InvalidInput,
    NotFound,
    Conflict,
    DatabaseUnavailable,
    CredentialUnavailable,
    RequestFailed,
    ResponseInvalid,
    ReadOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiSendPreview {
    pub(crate) field_names: Vec<&'static str>,
    pub(crate) taxonomy_dimension_count: usize,
}

pub(crate) struct AiService<S = SystemCredentialStore> {
    credentials: S,
    adapters: ProviderAdapterRegistry,
}

struct WritableConnection {
    connection: rusqlite::Connection,
    _lease: WriteAccessLease,
}

impl Deref for WritableConnection {
    type Target = rusqlite::Connection;

    fn deref(&self) -> &Self::Target {
        &self.connection
    }
}

impl DerefMut for WritableConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.connection
    }
}

impl AiService<SystemCredentialStore> {
    pub(crate) fn new() -> Self {
        Self::with_credential_store(SystemCredentialStore::new())
    }
}

impl<S: SecureCredentialStore> AiService<S> {
    pub(crate) fn with_credential_store(credentials: S) -> Self {
        Self {
            credentials,
            adapters: ProviderAdapterRegistry,
        }
    }

    pub(crate) fn list_providers(
        &self,
        root: &Path,
    ) -> Result<Vec<AiProviderConfig>, AiServiceError> {
        let connection = self.open(root)?;
        AiRepository::list_providers(&connection).map_err(map_repository_error)
    }

    /// 只返回是否需要重新配置，不会将安全存储中的值返回给 IPC。
    pub(crate) fn provider_needs_credential(&self, provider: &AiProviderConfig) -> bool {
        matches!(
            provider.kind,
            AiProviderKind::OpenaiCompatible | AiProviderKind::Gemini
        ) && self.credential_for(provider).is_err()
    }

    pub(crate) fn save_provider(
        &self,
        root: &Path,
        input: &SaveAiProviderConfig,
        api_key: Option<&str>,
    ) -> Result<AiProviderConfig, AiServiceError> {
        validate_provider_input(input)?;
        let mut connection = self.open_writable(root)?;
        let existing = input
            .id
            .map(|id| {
                validate_id(id)?;
                AiRepository::get_provider(&connection, id).map_err(map_repository_error)
            })
            .transpose()?;
        let secret = api_key.map(str::trim).filter(|value| !value.is_empty());
        if api_key.is_some() && secret.is_none() {
            return Err(AiServiceError::InvalidInput);
        }
        let credential_id = credential_id_for_save(existing.as_ref(), secret.is_some())?;
        let mut saved_input = input.clone();
        saved_input.credential_id = credential_id.clone();

        let previous_secret = credential_id
            .as_deref()
            .map(|id| self.credentials.get(id).map_err(map_credential_error))
            .transpose()?
            .flatten();
        if let (Some(id), Some(value)) = (credential_id.as_deref(), secret) {
            self.credentials
                .set(id, value)
                .map_err(map_credential_error)?;
        }

        match AiRepository::save_provider(&mut connection, &saved_input, now()?) {
            Ok(provider) => Ok(provider),
            Err(error) => {
                if let (Some(id), Some(value)) = (credential_id.as_deref(), secret) {
                    restore_credential(&self.credentials, id, previous_secret.as_deref(), value)?;
                }
                Err(map_repository_error(error))
            }
        }
    }

    pub(crate) fn delete_provider(&self, root: &Path, id: i64) -> Result<(), AiServiceError> {
        validate_id(id)?;
        let mut connection = self.open_writable(root)?;
        let provider = AiRepository::get_provider(&connection, id).map_err(map_repository_error)?;
        let previous_secret = provider
            .credential_id
            .as_deref()
            .map(|credential_id| {
                self.credentials
                    .get(credential_id)
                    .map_err(map_credential_error)
            })
            .transpose()?
            .flatten();
        if let Some(credential_id) = provider.credential_id.as_deref() {
            self.credentials
                .delete(credential_id)
                .map_err(map_credential_error)?;
        }
        if let Err(error) = AiRepository::delete_provider(&mut connection, id) {
            if let (Some(credential_id), Some(secret)) = (
                provider.credential_id.as_deref(),
                previous_secret.as_deref(),
            ) {
                self.credentials
                    .set(credential_id, secret)
                    .map_err(map_credential_error)?;
            }
            return Err(map_repository_error(error));
        }
        Ok(())
    }

    pub(crate) fn test_provider_connection(
        &self,
        root: &Path,
        provider_id: i64,
    ) -> Result<(), AiServiceError> {
        WriteAccessGuard::ensure_writable(root).map_err(map_access_mode_error)?;
        let provider = self.provider(root, provider_id)?;
        let credential = self.credential_for(&provider)?;
        self.adapters
            .test_connection(&provider, credential.as_deref())
            .map_err(map_adapter_error)
    }

    pub(crate) fn send_preview(
        &self,
        root: &Path,
        request: &ClassifyRequest,
    ) -> Result<AiSendPreview, AiServiceError> {
        let connection = self.open(root)?;
        let (content, dimensions) = build_request(&connection, request)?;
        let _ = content;
        Ok(AiSendPreview {
            field_names: selected_field_names(&request.input_scope, request.target.target_type),
            taxonomy_dimension_count: dimensions.len(),
        })
    }

    /// 返回批量操作的隐私范围和分类体系规模；不发起 Provider 请求，也不写入数据库。
    pub(crate) fn batch_send_preview(
        &self,
        root: &Path,
        request: &BatchClassifyRequest,
    ) -> Result<BatchAiSendPreview, AiServiceError> {
        validate_batch_classification_request(request)?;
        let connection = self.open(root)?;
        AiRepository::ensure_targets_active(&connection, &request.targets)
            .map_err(map_repository_error)?;
        // 批量模式允许部分作品没有提示词；实际执行时这些作品会被逐项跳过，
        // 不能因此阻断其余可分类作品的预览与提交。
        let dimensions = build_enabled_dimensions(&connection)?;
        Ok(BatchAiSendPreview {
            target_count: request.targets.len(),
            field_names: batch_selected_field_names(&request.input_scope, &request.targets),
            taxonomy_dimension_count: dimensions.len(),
        })
    }

    pub(crate) fn batch_send_preview_by_asset_display_numbers(
        &self,
        root: &Path,
        display_numbers: &[i64],
        input_scope: &AiInputScope,
    ) -> Result<BatchAiSendPreview, AiServiceError> {
        let request = self.asset_batch_request(root, display_numbers, input_scope)?;
        self.batch_send_preview(root, &request)
    }

    pub(crate) fn create_classification_suggestions(
        &self,
        root: &Path,
        provider_id: i64,
        request: &ClassifyRequest,
    ) -> Result<Vec<i64>, AiServiceError> {
        validate_id(provider_id)?;
        validate_classification_request(request)?;
        // 远程调用前只做权限检查和只读查询，绝不持有写租约。
        WriteAccessGuard::ensure_writable(root).map_err(map_access_mode_error)?;
        let connection = self.open(root)?;
        let provider =
            AiRepository::get_provider(&connection, provider_id).map_err(map_repository_error)?;
        if !provider.is_enabled || !provider.capabilities.classification {
            return Err(AiServiceError::Conflict);
        }
        let (content, dimensions) = build_request(&connection, request)?;
        let credential = self.credential_for(&provider)?;
        let suggestions = self
            .adapters
            .classify(
                &provider,
                credential.as_deref(),
                &ProviderClassificationRequest {
                    content,
                    dimensions,
                },
            )
            .map_err(map_adapter_error)?;
        if suggestions.is_empty() || suggestions.len() > MAX_SUGGESTIONS_PER_REQUEST {
            return Err(AiServiceError::ResponseInvalid);
        }
        // 网络调用期间模式可能切换，持久化 pending 前必须重新确认。
        // 仅在写入 pending 前取得短租约；若此时已经切为只读，直接拒绝落库。
        let _lease = WriteAccessGuard::acquire_writable(root).map_err(map_access_mode_error)?;
        let mut connection = self.open(root)?;
        AiRepository::create_suggestions(
            &mut connection,
            &CreateAiSuggestions {
                provider_id: Some(provider_id),
                target: request.target,
                source_model: provider.model,
                input_scope: request.input_scope.clone(),
                suggestions,
            },
            now()?,
        )
        .map_err(map_repository_error)
    }

    /// 逐个目标请求 Provider 并仅写入 pending 建议。
    ///
    /// 目标集合在网络调用前一次性验证；单个目标的解析或写入失败会记录为失败结果，
    /// 并依赖单目标 SQLite 事务确保该目标不留下半成品。
    pub(crate) fn create_classification_suggestions_batch(
        &self,
        root: &Path,
        provider_id: i64,
        request: &BatchClassifyRequest,
    ) -> Result<BatchAiSuggestionsResult, AiServiceError> {
        validate_id(provider_id)?;
        validate_batch_classification_request(request)?;
        WriteAccessGuard::ensure_writable(root).map_err(map_access_mode_error)?;
        let connection = self.open(root)?;
        let provider =
            AiRepository::get_provider(&connection, provider_id).map_err(map_repository_error)?;
        if !provider.is_enabled || !provider.capabilities.classification {
            return Err(AiServiceError::Conflict);
        }
        AiRepository::ensure_targets_active(&connection, &request.targets)
            .map_err(map_repository_error)?;
        let dimensions = build_enabled_dimensions(&connection)?;
        let credential = self.credential_for(&provider)?;
        let mut created = Vec::with_capacity(request.targets.len());
        let mut failed = Vec::new();

        for target in &request.targets {
            let target_request = ClassifyRequest {
                target: *target,
                input_scope: request.input_scope.clone(),
            };
            let content = match build_classification_content(&connection, &target_request) {
                Ok(content) => content,
                Err(error) => {
                    failed.push(batch_failure(*target, error));
                    continue;
                }
            };
            let suggestions = match self.adapters.classify(
                &provider,
                credential.as_deref(),
                &ProviderClassificationRequest {
                    content,
                    dimensions: dimensions.clone(),
                },
            ) {
                Ok(suggestions)
                    if !suggestions.is_empty()
                        && suggestions.len() <= MAX_SUGGESTIONS_PER_REQUEST =>
                {
                    suggestions
                }
                Ok(_) => {
                    failed.push(BatchAiSuggestionFailure {
                        target: *target,
                        kind: BatchAiSuggestionFailureKind::ResponseInvalid,
                    });
                    continue;
                }
                Err(error) => {
                    failed.push(BatchAiSuggestionFailure {
                        target: *target,
                        kind: batch_adapter_failure(error),
                    });
                    continue;
                }
            };
            match self.persist_pending_suggestions(
                root,
                provider_id,
                *target,
                &request.input_scope,
                &provider.model,
                suggestions,
            ) {
                Ok(suggestion_ids) => created.push(BatchAiSuggestionCreated {
                    target: *target,
                    suggestion_ids,
                }),
                Err(AiServiceError::ReadOnly) => return Err(AiServiceError::ReadOnly),
                Err(error) => failed.push(batch_failure(*target, error)),
            }
        }
        Ok(BatchAiSuggestionsResult { created, failed })
    }

    pub(crate) fn create_classification_suggestions_batch_by_asset_display_numbers(
        &self,
        root: &Path,
        provider_id: i64,
        display_numbers: &[i64],
        input_scope: &AiInputScope,
    ) -> Result<BatchAiSuggestionsResult, AiServiceError> {
        let request = self.asset_batch_request(root, display_numbers, input_scope)?;
        self.create_classification_suggestions_batch(root, provider_id, &request)
    }

    pub(crate) fn list_pending_suggestion_views(
        &self,
        root: &Path,
        cursor: Option<SuggestionCursor>,
        limit: u32,
    ) -> Result<SuggestionViewPage, AiServiceError> {
        validate_page(limit, cursor)?;
        let connection = self.open(root)?;
        AiRepository::list_pending_suggestion_views(&connection, cursor, limit)
            .map_err(map_repository_error)
    }

    pub(crate) fn suggestion_impact(
        &self,
        root: &Path,
        id: i64,
    ) -> Result<SuggestionImpact, AiServiceError> {
        validate_id(id)?;
        let connection = self.open(root)?;
        AiRepository::suggestion_impact(&connection, id).map_err(map_repository_error)
    }

    pub(crate) fn resolve_suggestion(
        &self,
        root: &Path,
        id: i64,
        resolution: &SuggestionResolution,
    ) -> Result<AiSuggestion, AiServiceError> {
        validate_id(id)?;
        validate_resolution(resolution)?;
        let mut connection = self.open_writable(root)?;
        AiRepository::resolve_suggestion(&mut connection, id, resolution, now()?)
            .map_err(map_repository_error)
    }

    fn provider(&self, root: &Path, id: i64) -> Result<AiProviderConfig, AiServiceError> {
        validate_id(id)?;
        let connection = self.open(root)?;
        AiRepository::get_provider(&connection, id).map_err(map_repository_error)
    }

    fn persist_pending_suggestions(
        &self,
        root: &Path,
        provider_id: i64,
        target: AiSuggestionTarget,
        input_scope: &AiInputScope,
        source_model: &str,
        suggestions: Vec<crate::domain::ClassifySuggestion>,
    ) -> Result<Vec<i64>, AiServiceError> {
        // 网络调用期间模式可能切换，持久化每个目标前必须重新确认并获取短租约。
        let _lease = WriteAccessGuard::acquire_writable(root).map_err(map_access_mode_error)?;
        let mut connection = self.open(root)?;
        AiRepository::create_suggestions(
            &mut connection,
            &CreateAiSuggestions {
                provider_id: Some(provider_id),
                target,
                source_model: source_model.to_owned(),
                input_scope: input_scope.clone(),
                suggestions,
            },
            now()?,
        )
        .map_err(map_repository_error)
    }

    fn credential_for(
        &self,
        provider: &AiProviderConfig,
    ) -> Result<Option<String>, AiServiceError> {
        match provider.kind {
            AiProviderKind::Ollama => provider
                .credential_id
                .as_deref()
                .map(|id| self.credentials.get(id).map_err(map_credential_error))
                .transpose()
                .map(|value| value.flatten()),
            AiProviderKind::OpenaiCompatible | AiProviderKind::Gemini => provider
                .credential_id
                .as_deref()
                .ok_or(AiServiceError::CredentialUnavailable)
                .and_then(|id| self.credentials.get(id).map_err(map_credential_error))
                .and_then(|value| value.ok_or(AiServiceError::CredentialUnavailable))
                .map(Some),
        }
    }

    fn open(&self, root: &Path) -> Result<rusqlite::Connection, AiServiceError> {
        DatabaseService::new()
            .open_current_workspace_database(root)
            .map_err(map_database_error)
    }

    fn asset_batch_request(
        &self,
        root: &Path,
        display_numbers: &[i64],
        input_scope: &AiInputScope,
    ) -> Result<BatchClassifyRequest, AiServiceError> {
        if display_numbers.is_empty() || display_numbers.len() > MAX_BATCH_CLASSIFICATION_TARGETS {
            return Err(AiServiceError::InvalidInput);
        }
        let connection = self.open(root)?;
        let asset_ids =
            LibraryRepository::asset_ids_for_display_numbers(&connection, display_numbers)
                .map_err(map_library_error)?;
        Ok(BatchClassifyRequest {
            targets: asset_ids
                .into_iter()
                .map(|target_id| AiSuggestionTarget {
                    target_type: AiSuggestionTargetType::Asset,
                    target_id,
                })
                .collect(),
            input_scope: input_scope.clone(),
        })
    }

    fn open_writable(&self, root: &Path) -> Result<WritableConnection, AiServiceError> {
        let lease = WriteAccessGuard::acquire_writable(root).map_err(map_access_mode_error)?;
        let connection = self.open(root)?;
        Ok(WritableConnection {
            connection,
            _lease: lease,
        })
    }
}

fn build_request(
    connection: &rusqlite::Connection,
    request: &ClassifyRequest,
) -> Result<(ProviderClassificationContent, Vec<ProviderDimension>), AiServiceError> {
    Ok((
        build_classification_content(connection, request)?,
        build_enabled_dimensions(connection)?,
    ))
}

fn build_classification_content(
    connection: &rusqlite::Connection,
    request: &ClassifyRequest,
) -> Result<ProviderClassificationContent, AiServiceError> {
    validate_classification_request(request)?;
    let content = match request.target.target_type {
        AiSuggestionTargetType::Project => {
            let project = LibraryRepository::get_project(connection, request.target.target_id)
                .map_err(map_library_error)?;
            ProviderClassificationContent {
                title: request.input_scope.title.then_some(project.summary.title),
                prompt_zh: request
                    .input_scope
                    .prompt_zh
                    .then_some(project.summary.prompt.prompt_zh),
                prompt_en: request
                    .input_scope
                    .prompt_en
                    .then_some(project.summary.prompt.prompt_en),
                negative_prompt: request
                    .input_scope
                    .negative_prompt
                    .then_some(project.summary.prompt.negative_prompt),
            }
        }
        AiSuggestionTargetType::Asset => {
            let asset = LibraryRepository::get_asset(connection, request.target.target_id)
                .map_err(map_library_error)?;
            ProviderClassificationContent {
                title: None,
                prompt_zh: request
                    .input_scope
                    .prompt_zh
                    .then_some(asset.summary.prompt.prompt_zh),
                prompt_en: request
                    .input_scope
                    .prompt_en
                    .then_some(asset.summary.prompt.prompt_en),
                negative_prompt: request
                    .input_scope
                    .negative_prompt
                    .then_some(asset.summary.prompt.negative_prompt),
            }
        }
    };
    if selected_text_is_empty(&content) {
        return Err(AiServiceError::InvalidInput);
    }
    Ok(content)
}

fn build_enabled_dimensions(
    connection: &rusqlite::Connection,
) -> Result<Vec<ProviderDimension>, AiServiceError> {
    let dimensions = LibraryRepository::list_dimensions(connection)
        .map_err(map_library_error)?
        .into_iter()
        .filter(|dimension| dimension.is_enabled)
        .map(|dimension| {
            let categories = LibraryRepository::list_categories(connection, Some(dimension.id))
                .map_err(map_library_error)?
                .into_iter()
                .filter(|category| category.is_enabled)
                .map(|category| ProviderCategory {
                    id: category.id,
                    name: category.name,
                    aliases: category.aliases,
                    description: category.description,
                })
                .collect();
            Ok(ProviderDimension {
                id: dimension.id,
                name: dimension.name,
                allows_multiple: dimension.allows_multiple,
                can_suggest_new: dimension.ai_can_suggest_new,
                categories,
            })
        })
        .collect::<Result<Vec<_>, AiServiceError>>()?;
    if dimensions.is_empty() {
        return Err(AiServiceError::Conflict);
    }
    Ok(dimensions)
}

fn selected_text_is_empty(content: &ProviderClassificationContent) -> bool {
    [
        content.title.as_deref(),
        content.prompt_zh.as_deref(),
        content.prompt_en.as_deref(),
        content.negative_prompt.as_deref(),
    ]
    .into_iter()
    .flatten()
    .all(|value| value.trim().is_empty())
}

fn selected_field_names(
    scope: &AiInputScope,
    target_type: AiSuggestionTargetType,
) -> Vec<&'static str> {
    let mut fields = Vec::with_capacity(4);
    if scope.title && matches!(target_type, AiSuggestionTargetType::Project) {
        fields.push("title");
    }
    if scope.prompt_zh {
        fields.push("promptZh");
    }
    if scope.prompt_en {
        fields.push("promptEn");
    }
    if scope.negative_prompt {
        fields.push("negativePrompt");
    }
    fields
}

fn batch_selected_field_names(scope: &AiInputScope, targets: &[AiSuggestionTarget]) -> Vec<String> {
    let contains_project = targets
        .iter()
        .any(|target| matches!(target.target_type, AiSuggestionTargetType::Project));
    let mut fields = Vec::with_capacity(4);
    if scope.title && contains_project {
        fields.push("title".to_owned());
    }
    if scope.prompt_zh {
        fields.push("promptZh".to_owned());
    }
    if scope.prompt_en {
        fields.push("promptEn".to_owned());
    }
    if scope.negative_prompt {
        fields.push("negativePrompt".to_owned());
    }
    fields
}

fn validate_provider_input(input: &SaveAiProviderConfig) -> Result<(), AiServiceError> {
    if input.display_name.trim().is_empty()
        || input.display_name.chars().count() > MAX_PROVIDER_TEXT_LENGTH
        || input.endpoint.trim().is_empty()
        || input.endpoint.len() > 2_000
        || input.model.trim().is_empty()
        || input.model.chars().count() > MAX_PROVIDER_TEXT_LENGTH
        || !(MIN_TIMEOUT_MS..=MAX_TIMEOUT_MS).contains(&input.timeout_ms)
    {
        return Err(AiServiceError::InvalidInput);
    }
    crate::adapters::ai_provider::validate_provider_configuration(
        input.kind,
        &input.endpoint,
        &input.model,
    )
    .map_err(|_| AiServiceError::InvalidInput)
}

fn validate_classification_request(request: &ClassifyRequest) -> Result<(), AiServiceError> {
    validate_id(request.target.target_id)?;
    if request.input_scope.title
        || request.input_scope.prompt_zh
        || request.input_scope.prompt_en
        || request.input_scope.negative_prompt
    {
        Ok(())
    } else {
        Err(AiServiceError::InvalidInput)
    }
}

fn validate_batch_classification_request(
    request: &BatchClassifyRequest,
) -> Result<(), AiServiceError> {
    if request.targets.is_empty() || request.targets.len() > MAX_BATCH_CLASSIFICATION_TARGETS {
        return Err(AiServiceError::InvalidInput);
    }
    if !(request.input_scope.title
        || request.input_scope.prompt_zh
        || request.input_scope.prompt_en
        || request.input_scope.negative_prompt)
    {
        return Err(AiServiceError::InvalidInput);
    }
    for (index, target) in request.targets.iter().enumerate() {
        validate_id(target.target_id)?;
        if request.targets[..index].contains(target) {
            return Err(AiServiceError::InvalidInput);
        }
    }
    Ok(())
}

fn validate_page(limit: u32, cursor: Option<SuggestionCursor>) -> Result<(), AiServiceError> {
    if !(MIN_PAGE_LIMIT..=MAX_PAGE_LIMIT).contains(&limit)
        || cursor.is_some_and(|value| value.id <= 0 || value.updated_at < 0)
    {
        return Err(AiServiceError::InvalidInput);
    }
    Ok(())
}

fn validate_resolution(resolution: &SuggestionResolution) -> Result<(), AiServiceError> {
    match resolution {
        SuggestionResolution::Accept | SuggestionResolution::Reject => Ok(()),
        SuggestionResolution::AcceptExisting { category_id }
        | SuggestionResolution::MergeIntoExisting { category_id } => validate_id(*category_id),
        SuggestionResolution::CreateCategory { name }
        | SuggestionResolution::ConvertToTag { name }
            if name.trim().is_empty() || name.chars().count() > MAX_PROVIDER_TEXT_LENGTH =>
        {
            Err(AiServiceError::InvalidInput)
        }
        _ => Ok(()),
    }
}

fn validate_id(id: i64) -> Result<(), AiServiceError> {
    (id > 0).then_some(()).ok_or(AiServiceError::InvalidInput)
}

fn now() -> Result<i64, AiServiceError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AiServiceError::DatabaseUnavailable)
        .and_then(|value| {
            i64::try_from(value.as_millis()).map_err(|_| AiServiceError::DatabaseUnavailable)
        })
}

fn new_credential_id() -> Result<String, AiServiceError> {
    let mut bytes = [0_u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|_| AiServiceError::CredentialUnavailable)?;
    let sequence = NEXT_CREDENTIAL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let encoded = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("provider-{encoded}-{sequence:x}"))
}

fn credential_id_for_save(
    existing: Option<&AiProviderConfig>,
    has_new_secret: bool,
) -> Result<Option<String>, AiServiceError> {
    match (existing, has_new_secret) {
        // 允许为旧的“无密钥”服务补填密钥；此前会沿用 None，导致密钥未写入安全存储。
        (Some(provider), true) => Ok(Some(
            provider
                .credential_id
                .clone()
                .map_or_else(new_credential_id, Ok)?,
        )),
        (Some(provider), false) => Ok(provider.credential_id.clone()),
        (None, true) => Ok(Some(new_credential_id()?)),
        (None, false) => Ok(None),
    }
}

fn restore_credential<S: SecureCredentialStore>(
    credentials: &S,
    credential_id: &str,
    previous: Option<&str>,
    _attempted: &str,
) -> Result<(), AiServiceError> {
    match previous {
        Some(value) => credentials
            .set(credential_id, value)
            .map_err(map_credential_error),
        None => credentials
            .delete(credential_id)
            .map_err(map_credential_error),
    }
}

fn map_repository_error(error: AiRepositoryError) -> AiServiceError {
    match error {
        AiRepositoryError::NotFound => AiServiceError::NotFound,
        AiRepositoryError::Conflict => AiServiceError::Conflict,
        AiRepositoryError::InvalidData => AiServiceError::ResponseInvalid,
        AiRepositoryError::DatabaseFailed => AiServiceError::DatabaseUnavailable,
    }
}

fn map_library_error(error: crate::repositories::LibraryRepositoryError) -> AiServiceError {
    match error {
        crate::repositories::LibraryRepositoryError::NotFound => AiServiceError::NotFound,
        crate::repositories::LibraryRepositoryError::Conflict => AiServiceError::Conflict,
        crate::repositories::LibraryRepositoryError::InvalidData => AiServiceError::ResponseInvalid,
        crate::repositories::LibraryRepositoryError::DatabaseFailed => {
            AiServiceError::DatabaseUnavailable
        }
    }
}

fn map_database_error(_: DatabaseServiceError) -> AiServiceError {
    AiServiceError::DatabaseUnavailable
}
fn map_access_mode_error(error: AccessModeServiceError) -> AiServiceError {
    match error {
        AccessModeServiceError::ReadOnly => AiServiceError::ReadOnly,
        AccessModeServiceError::Workspace(_)
        | AccessModeServiceError::DatabaseUnavailable
        | AccessModeServiceError::InvalidSetting
        | AccessModeServiceError::ClockUnavailable => AiServiceError::DatabaseUnavailable,
    }
}

fn map_credential_error(error: SecureCredentialStoreError) -> AiServiceError {
    match error {
        SecureCredentialStoreError::InvalidReference => AiServiceError::InvalidInput,
        SecureCredentialStoreError::Unavailable => AiServiceError::CredentialUnavailable,
    }
}

fn map_adapter_error(error: AiProviderAdapterError) -> AiServiceError {
    match error {
        AiProviderAdapterError::InvalidConfiguration => AiServiceError::InvalidInput,
        AiProviderAdapterError::CredentialRequired => AiServiceError::CredentialUnavailable,
        AiProviderAdapterError::RequestFailed | AiProviderAdapterError::RequestRejected(_) => {
            AiServiceError::RequestFailed
        }
        AiProviderAdapterError::ResponseInvalid => AiServiceError::ResponseInvalid,
    }
}

fn batch_adapter_failure(error: AiProviderAdapterError) -> BatchAiSuggestionFailureKind {
    match error {
        AiProviderAdapterError::RequestFailed | AiProviderAdapterError::RequestRejected(_) => {
            BatchAiSuggestionFailureKind::RequestFailed
        }
        AiProviderAdapterError::ResponseInvalid => BatchAiSuggestionFailureKind::ResponseInvalid,
        AiProviderAdapterError::InvalidConfiguration
        | AiProviderAdapterError::CredentialRequired => {
            BatchAiSuggestionFailureKind::InputUnavailable
        }
    }
}

fn batch_failure(target: AiSuggestionTarget, error: AiServiceError) -> BatchAiSuggestionFailure {
    let kind = match error {
        AiServiceError::RequestFailed => BatchAiSuggestionFailureKind::RequestFailed,
        AiServiceError::ResponseInvalid => BatchAiSuggestionFailureKind::ResponseInvalid,
        AiServiceError::InvalidInput | AiServiceError::NotFound | AiServiceError::Conflict => {
            BatchAiSuggestionFailureKind::InputUnavailable
        }
        AiServiceError::DatabaseUnavailable
        | AiServiceError::CredentialUnavailable
        | AiServiceError::ReadOnly => BatchAiSuggestionFailureKind::PersistFailed,
    };
    BatchAiSuggestionFailure { target, kind }
}

#[cfg(test)]
mod tests {
    use super::{
        AiServiceError, credential_id_for_save, validate_batch_classification_request,
        validate_classification_request, validate_provider_input,
    };
    use crate::domain::{
        AiInputScope, AiProviderCapabilities, AiProviderConfig, AiProviderKind, AiSuggestionTarget,
        AiSuggestionTargetType, BatchClassifyRequest, ClassifyRequest, SaveAiProviderConfig,
    };

    #[test]
    fn remote_provider_requires_https_but_local_ollama_can_use_loopback_http() {
        let remote = SaveAiProviderConfig {
            id: None,
            kind: AiProviderKind::OpenaiCompatible,
            display_name: "远端".to_owned(),
            endpoint: "http://example.test".to_owned(),
            model: "model".to_owned(),
            capabilities: AiProviderCapabilities {
                classification: true,
            },
            timeout_ms: 5_000,
            credential_id: None,
            is_enabled: true,
        };
        assert_eq!(
            validate_provider_input(&remote),
            Err(AiServiceError::InvalidInput)
        );
        let unsafe_remote = SaveAiProviderConfig {
            endpoint: "https://user@example.com".to_owned(),
            ..remote.clone()
        };
        assert_eq!(
            validate_provider_input(&unsafe_remote),
            Err(AiServiceError::InvalidInput)
        );
        let ollama = SaveAiProviderConfig {
            kind: AiProviderKind::Ollama,
            endpoint: "http://127.0.0.1:11434".to_owned(),
            ..remote
        };
        assert!(validate_provider_input(&ollama).is_ok());
    }

    #[test]
    fn existing_provider_without_credential_can_be_reconfigured() {
        let provider = AiProviderConfig {
            id: 1,
            kind: AiProviderKind::OpenaiCompatible,
            display_name: "existing".to_owned(),
            endpoint: "https://api.example.test/v1".to_owned(),
            model: "model".to_owned(),
            capabilities: AiProviderCapabilities {
                classification: true,
            },
            timeout_ms: 5_000,
            credential_id: None,
            is_enabled: true,
            created_at: 1,
            updated_at: 1,
        };

        assert!(
            credential_id_for_save(Some(&provider), true)
                .expect("补填密钥应生成安全存储引用")
                .is_some()
        );
        assert_eq!(
            credential_id_for_save(Some(&provider), false).expect("未输入密钥时保持旧状态"),
            None
        );
    }

    #[test]
    fn classification_needs_an_explicit_whitelisted_field() {
        let request = ClassifyRequest {
            target: AiSuggestionTarget {
                target_type: AiSuggestionTargetType::Project,
                target_id: 1,
            },
            input_scope: AiInputScope {
                title: false,
                prompt_zh: false,
                prompt_en: false,
                negative_prompt: false,
            },
        };
        assert_eq!(
            validate_classification_request(&request),
            Err(AiServiceError::InvalidInput)
        );
    }

    #[test]
    fn batch_classification_is_bounded_to_one_hundred_unique_targets() {
        let scope = AiInputScope::default();
        let targets = (1..=100)
            .map(|target_id| AiSuggestionTarget {
                target_type: AiSuggestionTargetType::Project,
                target_id,
            })
            .collect::<Vec<_>>();
        assert!(
            validate_batch_classification_request(&BatchClassifyRequest {
                targets: targets.clone(),
                input_scope: scope.clone(),
            })
            .is_ok()
        );
        let mut too_many = targets;
        too_many.push(AiSuggestionTarget {
            target_type: AiSuggestionTargetType::Project,
            target_id: 101,
        });
        assert_eq!(
            validate_batch_classification_request(&BatchClassifyRequest {
                targets: too_many,
                input_scope: scope.clone(),
            }),
            Err(AiServiceError::InvalidInput)
        );
        assert_eq!(
            validate_batch_classification_request(&BatchClassifyRequest {
                targets: vec![
                    AiSuggestionTarget {
                        target_type: AiSuggestionTargetType::Asset,
                        target_id: 7,
                    },
                    AiSuggestionTarget {
                        target_type: AiSuggestionTargetType::Asset,
                        target_id: 7,
                    },
                ],
                input_scope: scope,
            }),
            Err(AiServiceError::InvalidInput)
        );
    }
}
