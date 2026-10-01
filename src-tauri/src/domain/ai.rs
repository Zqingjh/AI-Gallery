use serde::{Deserialize, Serialize};

/// 已配置 AI 服务的类型。凭据始终保存在系统安全存储，不属于此枚举或配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AiProviderKind {
    OpenaiCompatible,
    Gemini,
    Ollama,
}

impl AiProviderKind {
    pub(crate) fn as_db_str(self) -> &'static str {
        match self {
            Self::OpenaiCompatible => "openai_compatible",
            Self::Gemini => "gemini",
            Self::Ollama => "ollama",
        }
    }

    pub(crate) fn from_db_str(value: &str) -> Option<Self> {
        match value {
            "openai_compatible" => Some(Self::OpenaiCompatible),
            "gemini" => Some(Self::Gemini),
            "ollama" => Some(Self::Ollama),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AiProviderCapabilities {
    pub(crate) classification: bool,
}

/// 数据库中的 Provider 配置；credential_id 只是安全存储的查找键，绝不是 API Key。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AiProviderConfig {
    pub(crate) id: i64,
    pub(crate) kind: AiProviderKind,
    pub(crate) display_name: String,
    pub(crate) endpoint: String,
    pub(crate) model: String,
    pub(crate) capabilities: AiProviderCapabilities,
    pub(crate) timeout_ms: u32,
    pub(crate) credential_id: Option<String>,
    pub(crate) is_enabled: bool,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SaveAiProviderConfig {
    pub(crate) id: Option<i64>,
    pub(crate) kind: AiProviderKind,
    pub(crate) display_name: String,
    pub(crate) endpoint: String,
    pub(crate) model: String,
    pub(crate) capabilities: AiProviderCapabilities,
    pub(crate) timeout_ms: u32,
    pub(crate) credential_id: Option<String>,
    pub(crate) is_enabled: bool,
}

/// 允许发送给 AI 的字段开关。此版本没有备注、路径、媒体或生成参数字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AiInputScope {
    pub(crate) title: bool,
    pub(crate) prompt_zh: bool,
    pub(crate) prompt_en: bool,
    pub(crate) negative_prompt: bool,
}

impl Default for AiInputScope {
    fn default() -> Self {
        Self {
            title: true,
            prompt_zh: true,
            prompt_en: true,
            negative_prompt: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AiSuggestionTargetType {
    Project,
    Asset,
}

impl AiSuggestionTargetType {
    pub(crate) fn as_db_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Asset => "asset",
        }
    }

    pub(crate) fn from_db_str(value: &str) -> Option<Self> {
        match value {
            "project" => Some(Self::Project),
            "asset" => Some(Self::Asset),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AiSuggestionTarget {
    pub(crate) target_type: AiSuggestionTargetType,
    pub(crate) target_id: i64,
}

/// Provider 的最小分类请求。实际出站内容由 service 根据 input_scope 白名单组装。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClassifyRequest {
    pub(crate) target: AiSuggestionTarget,
    pub(crate) input_scope: AiInputScope,
}

/// 批量分类请求只携带目标与已确认的字段范围；不携带媒体、路径、备注或提示词正文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BatchClassifyRequest {
    pub(crate) targets: Vec<AiSuggestionTarget>,
    pub(crate) input_scope: AiInputScope,
}

/// 批量发送前的隐私与影响预览。此结构不含任何待发送的文本内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BatchAiSendPreview {
    pub(crate) target_count: usize,
    pub(crate) field_names: Vec<String>,
    pub(crate) taxonomy_dimension_count: usize,
}

/// 单个目标的批量执行结果；`suggestion_ids` 中的记录均处于 pending 状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BatchAiSuggestionCreated {
    pub(crate) target: AiSuggestionTarget,
    pub(crate) suggestion_ids: Vec<i64>,
}

/// 单个目标失败时仅返回可安全展示的类别，绝不回传 Provider 原始响应或提示词。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BatchAiSuggestionFailureKind {
    InputUnavailable,
    RequestFailed,
    ResponseInvalid,
    PersistFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BatchAiSuggestionFailure {
    pub(crate) target: AiSuggestionTarget,
    pub(crate) kind: BatchAiSuggestionFailureKind,
}

/// 批量执行不会自动应用正式分类；成功项只创建 pending 建议，失败项不会留下半成品。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BatchAiSuggestionsResult {
    pub(crate) created: Vec<BatchAiSuggestionCreated>,
    pub(crate) failed: Vec<BatchAiSuggestionFailure>,
}

/// Provider 受限结构化输出，不包含原始响应或完整输入文本。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClassifySuggestion {
    pub(crate) dimension_id: i64,
    pub(crate) category_id: Option<i64>,
    pub(crate) suggested_category_name: Option<String>,
    pub(crate) confidence: Option<f64>,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CreateAiSuggestions {
    pub(crate) provider_id: Option<i64>,
    pub(crate) target: AiSuggestionTarget,
    pub(crate) source_model: String,
    pub(crate) input_scope: AiInputScope,
    pub(crate) suggestions: Vec<ClassifySuggestion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SuggestionStatus {
    Pending,
    Accepted,
    Rejected,
    Modified,
}

impl SuggestionStatus {
    pub(crate) fn as_db_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Modified => "modified",
        }
    }

    pub(crate) fn from_db_str(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "accepted" => Some(Self::Accepted),
            "rejected" => Some(Self::Rejected),
            "modified" => Some(Self::Modified),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SuggestionCursor {
    pub(crate) updated_at: i64,
    pub(crate) id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiSuggestion {
    pub(crate) id: i64,
    pub(crate) provider_id: Option<i64>,
    #[serde(flatten)]
    pub(crate) target: AiSuggestionTarget,
    pub(crate) dimension_id: i64,
    pub(crate) category_id: Option<i64>,
    pub(crate) suggested_category_name: Option<String>,
    pub(crate) confidence: Option<f64>,
    pub(crate) reason: String,
    pub(crate) source_model: String,
    pub(crate) input_scope: AiInputScope,
    pub(crate) status: SuggestionStatus,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

/// 审核页的单查询投影视图，避免为标题、维度与分类名称产生 N+1 查询。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiSuggestionView {
    #[serde(flatten)]
    pub(crate) suggestion: AiSuggestion,
    pub(crate) target_title: String,
    pub(crate) dimension_name: String,
    pub(crate) category: Option<AiSuggestionCategory>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiSuggestionCategory {
    pub(crate) id: i64,
    pub(crate) name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SuggestionViewPage {
    pub(crate) items: Vec<AiSuggestionView>,
    pub(crate) next_cursor: Option<SuggestionCursor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum SuggestionResolution {
    Accept,
    AcceptExisting { category_id: i64 },
    CreateCategory { name: String },
    MergeIntoExisting { category_id: i64 },
    ConvertToTag { name: String },
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SuggestionImpact {
    pub(crate) suggestion_id: i64,
    #[serde(flatten)]
    pub(crate) target: AiSuggestionTarget,
    pub(crate) dimension_id: i64,
    pub(crate) allows_multiple: bool,
    pub(crate) existing_category_ids: Vec<i64>,
}
