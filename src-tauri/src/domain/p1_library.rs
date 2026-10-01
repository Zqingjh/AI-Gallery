use serde::{Deserialize, Serialize};

use super::{MediaType, Page, PageCursor, PromptText};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SavedAssetFilter {
    pub(crate) version: u32,
    #[serde(default)]
    pub(crate) keyword: Option<String>,
    #[serde(default)]
    pub(crate) media_type: Option<MediaType>,
    #[serde(default)]
    pub(crate) model: Option<String>,
    #[serde(default)]
    pub(crate) platform: Option<String>,
    #[serde(default)]
    pub(crate) category_ids: Vec<i64>,
    #[serde(default)]
    pub(crate) rating: Option<u8>,
    #[serde(default)]
    pub(crate) is_favorite: Option<bool>,
    #[serde(default)]
    pub(crate) is_public: Option<bool>,
    #[serde(default)]
    pub(crate) created_after: Option<i64>,
    #[serde(default)]
    pub(crate) created_before: Option<i64>,
    #[serde(default)]
    pub(crate) min_aspect_ratio: Option<f64>,
    #[serde(default)]
    pub(crate) max_aspect_ratio: Option<f64>,
}

impl Default for SavedAssetFilter {
    fn default() -> Self {
        Self {
            version: 1,
            keyword: None,
            media_type: None,
            model: None,
            platform: None,
            category_ids: Vec::new(),
            rating: None,
            is_favorite: None,
            is_public: None,
            created_after: None,
            created_before: None,
            min_aspect_ratio: None,
            max_aspect_ratio: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SavedFilter {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) filter: SavedAssetFilter,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SaveSavedFilter {
    pub(crate) id: Option<i64>,
    pub(crate) name: String,
    pub(crate) filter: SavedAssetFilter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CustomFieldTargetType {
    Project,
    Asset,
}

impl CustomFieldTargetType {
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
#[serde(rename_all = "snake_case")]
pub(crate) enum CustomFieldValueType {
    Text,
    Number,
    Boolean,
    Date,
    Json,
}

impl CustomFieldValueType {
    pub(crate) fn as_db_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Date => "date",
            Self::Json => "json",
        }
    }

    pub(crate) fn from_db_str(value: &str) -> Option<Self> {
        match value {
            "text" => Some(Self::Text),
            "number" => Some(Self::Number),
            "boolean" => Some(Self::Boolean),
            "date" => Some(Self::Date),
            "json" => Some(Self::Json),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CustomField {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) target_type: CustomFieldTargetType,
    pub(crate) value_type: CustomFieldValueType,
    pub(crate) options: serde_json::Value,
    pub(crate) is_enabled: bool,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SaveCustomField {
    pub(crate) id: Option<i64>,
    pub(crate) name: String,
    pub(crate) target_type: CustomFieldTargetType,
    pub(crate) value_type: CustomFieldValueType,
    pub(crate) options: serde_json::Value,
    pub(crate) is_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CustomFieldValueStatus {
    Confirmed,
    Pending,
}

impl CustomFieldValueStatus {
    pub(crate) fn from_db_str(value: &str) -> Option<Self> {
        match value {
            "confirmed" => Some(Self::Confirmed),
            "pending" => Some(Self::Pending),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CustomFieldValue {
    pub(crate) id: i64,
    pub(crate) field_id: i64,
    pub(crate) target_type: CustomFieldTargetType,
    pub(crate) target_id: i64,
    pub(crate) value: serde_json::Value,
    pub(crate) status: CustomFieldValueStatus,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CustomFieldValueInput {
    pub(crate) field_id: i64,
    pub(crate) target_type: CustomFieldTargetType,
    pub(crate) target_id: i64,
    pub(crate) value: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PromptVersion {
    pub(crate) id: i64,
    pub(crate) prompt_id: i64,
    pub(crate) version: i64,
    #[serde(flatten)]
    pub(crate) prompt: PromptText,
    pub(crate) created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PromptVersionCursor {
    pub(crate) version: i64,
    pub(crate) id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PromptVersionPage {
    pub(crate) items: Vec<PromptVersion>,
    pub(crate) next_cursor: Option<PromptVersionCursor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EditHistoryTargetType {
    Project,
    Asset,
    Prompt,
    CustomFieldValue,
}

impl EditHistoryTargetType {
    pub(crate) fn as_db_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Asset => "asset",
            Self::Prompt => "prompt",
            Self::CustomFieldValue => "custom_field_value",
        }
    }

    pub(crate) fn from_db_str(value: &str) -> Option<Self> {
        match value {
            "project" => Some(Self::Project),
            "asset" => Some(Self::Asset),
            "prompt" => Some(Self::Prompt),
            "custom_field_value" => Some(Self::CustomFieldValue),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EditAction {
    Create,
    Update,
    Delete,
    Restore,
    BulkUpdate,
}

impl EditAction {
    pub(crate) fn as_db_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
            Self::Restore => "restore",
            Self::BulkUpdate => "bulk_update",
        }
    }

    pub(crate) fn from_db_str(value: &str) -> Option<Self> {
        match value {
            "create" => Some(Self::Create),
            "update" => Some(Self::Update),
            "delete" => Some(Self::Delete),
            "restore" => Some(Self::Restore),
            "bulk_update" => Some(Self::BulkUpdate),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EditHistoryStatus {
    Confirmed,
    Pending,
}

impl EditHistoryStatus {
    pub(crate) fn from_db_str(value: &str) -> Option<Self> {
        match value {
            "confirmed" => Some(Self::Confirmed),
            "pending" => Some(Self::Pending),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EditHistoryInput {
    pub(crate) target_type: EditHistoryTargetType,
    pub(crate) target_id: i64,
    pub(crate) action: EditAction,
    /// 仅允许字段名；不得把完整提示词或用户路径写入编辑历史。
    pub(crate) changed_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EditHistoryEntry {
    pub(crate) id: i64,
    #[serde(flatten)]
    pub(crate) input: EditHistoryInput,
    pub(crate) status: EditHistoryStatus,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

pub(crate) type SavedFilterPage = Page<SavedFilter>;
pub(crate) type CustomFieldPage = Page<CustomField>;
pub(crate) type CustomFieldValuePage = Page<CustomFieldValue>;
pub(crate) type EditHistoryPage = Page<EditHistoryEntry>;
pub(crate) type P1PageCursor = PageCursor;
