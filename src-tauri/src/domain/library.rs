use serde::{Deserialize, Serialize};

pub(crate) const MIN_PAGE_LIMIT: u32 = 1;
pub(crate) const MAX_PAGE_LIMIT: u32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PageCursor {
    pub(crate) updated_at: i64,
    pub(crate) id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Page<T> {
    pub(crate) items: Vec<T>,
    pub(crate) next_cursor: Option<PageCursor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NumberedAssetPage {
    pub(crate) items: Vec<AssetSummary>,
    pub(crate) page: u32,
    pub(crate) page_size: u32,
    pub(crate) total_count: u64,
    pub(crate) total_pages: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PromptText {
    pub(crate) prompt_zh: String,
    pub(crate) prompt_en: String,
    pub(crate) negative_prompt: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ProjectKind {
    #[default]
    Simple,
    Canvas,
}

impl ProjectKind {
    pub(crate) fn as_db_str(self) -> &'static str {
        match self {
            Self::Simple => "simple",
            Self::Canvas => "canvas",
        }
    }

    pub(crate) fn from_db_str(value: &str) -> Option<Self> {
        match value {
            "simple" => Some(Self::Simple),
            "canvas" => Some(Self::Canvas),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectSummary {
    pub(crate) id: i64,
    /// 旧回收站快照没有该字段，缺失时必须继续按普通项目恢复。
    #[serde(default)]
    pub(crate) kind: ProjectKind,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) prompt: PromptText,
    pub(crate) rating: u8,
    pub(crate) is_favorite: bool,
    pub(crate) is_public: bool,
    pub(crate) notes: String,
    #[serde(default)]
    pub(crate) asset_count: i64,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectDetail {
    #[serde(flatten)]
    pub(crate) summary: ProjectSummary,
    pub(crate) category_ids: Vec<i64>,
    pub(crate) tag_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreateProject {
    #[serde(default)]
    pub(crate) kind: ProjectKind,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) prompt: PromptText,
    pub(crate) rating: u8,
    pub(crate) is_favorite: bool,
    pub(crate) is_public: bool,
    pub(crate) notes: String,
    pub(crate) category_ids: Vec<i64>,
    pub(crate) tag_ids: Vec<i64>,
}

pub(crate) type UpdateProject = CreateProject;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum MediaType {
    Image,
    Video,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CanvasMemberRole {
    Output,
    Reference,
}

impl CanvasMemberRole {
    pub(crate) fn as_db_str(self) -> &'static str {
        match self {
            Self::Output => "output",
            Self::Reference => "reference",
        }
    }

    pub(crate) fn from_db_str(value: &str) -> Option<Self> {
        match value {
            "output" => Some(Self::Output),
            "reference" => Some(Self::Reference),
            _ => None,
        }
    }
}

/// 画布成员 IPC 白名单；不包含存储路径、备注、生成参数或文件哈希。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CanvasProjectMember {
    pub(crate) asset_id: i64,
    pub(crate) display_order: i64,
    pub(crate) file_name: String,
    pub(crate) media_type: MediaType,
    pub(crate) model_name: Option<String>,
    pub(crate) platform_name: Option<String>,
    pub(crate) width: Option<u32>,
    pub(crate) height: Option<u32>,
    pub(crate) duration_ms: Option<i64>,
    pub(crate) updated_at: i64,
    pub(crate) role: CanvasMemberRole,
    pub(crate) reference_name: Option<String>,
    pub(crate) prompt_zh: String,
    pub(crate) prompt_en: String,
    pub(crate) negative_prompt: String,
}

impl MediaType {
    pub(crate) fn as_db_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PathKind {
    Managed,
    External,
}

impl PathKind {
    pub(crate) fn as_db_str(self) -> &'static str {
        match self {
            Self::Managed => "managed",
            Self::External => "external",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssetMetadata {
    pub(crate) project_id: Option<i64>,
    pub(crate) media_type: MediaType,
    pub(crate) path_kind: PathKind,
    pub(crate) stored_path: String,
    pub(crate) file_name: String,
    pub(crate) mime_type: Option<String>,
    pub(crate) file_size: Option<i64>,
    pub(crate) content_hash: Option<String>,
    pub(crate) width: Option<u32>,
    pub(crate) height: Option<u32>,
    pub(crate) duration_ms: Option<i64>,
    pub(crate) frame_rate: Option<f64>,
    pub(crate) has_audio: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssetSummary {
    pub(crate) id: i64,
    pub(crate) display_order: i64,
    #[serde(flatten)]
    pub(crate) media: AssetMetadata,
    pub(crate) prompt: PromptText,
    pub(crate) model: Option<String>,
    pub(crate) platform: Option<String>,
    pub(crate) generation_params: serde_json::Value,
    pub(crate) rating: u8,
    pub(crate) is_favorite: bool,
    pub(crate) is_public: bool,
    pub(crate) notes: String,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssetDetail {
    #[serde(flatten)]
    pub(crate) summary: AssetSummary,
    pub(crate) category_ids: Vec<i64>,
    pub(crate) tag_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreateAsset {
    #[serde(flatten)]
    pub(crate) media: AssetMetadata,
    pub(crate) prompt: PromptText,
    pub(crate) model: Option<String>,
    pub(crate) platform: Option<String>,
    pub(crate) generation_params: serde_json::Value,
    pub(crate) rating: u8,
    pub(crate) is_favorite: bool,
    pub(crate) is_public: bool,
    pub(crate) notes: String,
    pub(crate) category_ids: Vec<i64>,
    pub(crate) tag_ids: Vec<i64>,
}

pub(crate) type UpdateAsset = CreateAsset;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum AssetOrderChangeMode {
    Swap,
    ShiftFollowing,
}

pub(crate) const MAX_BULK_ASSET_IDS: usize = 100;

/// 可空文本字段的批量编辑动作。显式区分“保持原值”和“清空原值”。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "action", content = "value")]
pub(crate) enum BulkNullableTextEdit {
    #[default]
    Keep,
    Set(String),
    Clear,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BulkAssetEditInput {
    pub(crate) asset_ids: Vec<i64>,
    pub(crate) rating: Option<u8>,
    pub(crate) is_favorite: Option<bool>,
    pub(crate) is_public: Option<bool>,
    #[serde(default)]
    pub(crate) model: BulkNullableTextEdit,
    #[serde(default)]
    pub(crate) platform: BulkNullableTextEdit,
    pub(crate) add_category_ids: Vec<i64>,
    pub(crate) remove_category_ids: Vec<i64>,
    pub(crate) add_tag_ids: Vec<i64>,
    pub(crate) remove_tag_ids: Vec<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BulkAssetEditPreview {
    pub(crate) target_count: u32,
    pub(crate) category_relations_to_add: u32,
    pub(crate) category_relations_to_remove: u32,
    pub(crate) tag_relations_to_add: u32,
    pub(crate) tag_relations_to_remove: u32,
}

/// 模型对比 IPC 白名单；不包含路径、提示词、备注或生成参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelComparisonItem {
    pub(crate) id: i64,
    pub(crate) file_name: String,
    pub(crate) model: String,
    pub(crate) platform: String,
    pub(crate) updated_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub(crate) enum ModelComparisonScope {
    Project { project_id: i64 },
    MatchingPrompt { baseline_asset_id: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelComparisonQuery {
    pub(crate) scope: ModelComparisonScope,
    pub(crate) cursor: Option<PageCursor>,
    pub(crate) limit: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum AssetSearchField {
    #[default]
    Title,
    Prompt,
    Notes,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct AssetListQuery {
    pub(crate) project_id: Option<i64>,
    pub(crate) media_type: Option<MediaType>,
    pub(crate) keyword: Option<String>,
    pub(crate) search_field: AssetSearchField,
    pub(crate) exact_match: bool,
    pub(crate) model: Option<String>,
    pub(crate) platform: Option<String>,
    pub(crate) category_ids: Vec<i64>,
    pub(crate) rating: Option<u8>,
    pub(crate) is_favorite: Option<bool>,
    pub(crate) is_public: Option<bool>,
    pub(crate) created_after: Option<i64>,
    pub(crate) created_before: Option<i64>,
    pub(crate) min_aspect_ratio: Option<f64>,
    pub(crate) max_aspect_ratio: Option<f64>,
    pub(crate) cursor: Option<PageCursor>,
    pub(crate) limit: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum MetadataPresetKind {
    Model,
    Platform,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MetadataPreset {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) asset_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MetadataPresets {
    pub(crate) models: Vec<MetadataPreset>,
    pub(crate) platforms: Vec<MetadataPreset>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DuplicateGroupCursor {
    pub(crate) updated_at: i64,
    pub(crate) content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DuplicateAssetGroup {
    pub(crate) content_hash: String,
    pub(crate) asset_count: i64,
    pub(crate) representative_asset_id: i64,
    pub(crate) representative_file_name: String,
    pub(crate) updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DuplicateAssetGroupPage {
    pub(crate) items: Vec<DuplicateAssetGroup>,
    pub(crate) next_cursor: Option<DuplicateGroupCursor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Dimension {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) allows_multiple: bool,
    pub(crate) ai_can_suggest_new: bool,
    pub(crate) is_enabled: bool,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DimensionInput {
    pub(crate) name: String,
    pub(crate) allows_multiple: bool,
    pub(crate) ai_can_suggest_new: bool,
    pub(crate) is_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Category {
    pub(crate) id: i64,
    pub(crate) dimension_id: i64,
    pub(crate) name: String,
    pub(crate) aliases: Vec<String>,
    pub(crate) description: String,
    pub(crate) color: Option<String>,
    pub(crate) icon: Option<String>,
    pub(crate) is_enabled: bool,
    pub(crate) asset_count: i64,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CategoryInput {
    pub(crate) dimension_id: i64,
    pub(crate) name: String,
    pub(crate) aliases: Vec<String>,
    pub(crate) description: String,
    pub(crate) color: Option<String>,
    pub(crate) icon: Option<String>,
    pub(crate) is_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Tag {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) asset_count: i64,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RelationImpact {
    pub(crate) project_count: i64,
    pub(crate) asset_count: i64,
    pub(crate) suggestion_count: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CategoryDeleteAction {
    Remove,
    ReplaceWith(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum TrashEntityType {
    Project,
    Asset,
}

#[cfg(test)]
mod canvas_project_tests {
    use super::{ProjectKind, ProjectSummary};

    #[test]
    fn legacy_project_snapshot_defaults_to_simple_kind() {
        let legacy = r#"{
            "id":1,
            "title":"旧项目",
            "description":"",
            "prompt":{"promptZh":"","promptEn":"","negativePrompt":""},
            "rating":0,
            "isFavorite":false,
            "isPublic":false,
            "notes":"",
            "createdAt":1,
            "updatedAt":2
        }"#;

        let project: ProjectSummary = serde_json::from_str(legacy).expect("旧项目快照应能反序列化");
        assert_eq!(project.kind, ProjectKind::Simple);
        assert_eq!(project.asset_count, 0);
    }
}

impl TrashEntityType {
    pub(crate) fn as_db_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Asset => "asset",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrashEntry {
    pub(crate) id: i64,
    pub(crate) entity_type: TrashEntityType,
    pub(crate) entity_id: i64,
    pub(crate) display_name: String,
    pub(crate) media_type: Option<MediaType>,
    pub(crate) deleted_at: i64,
}
