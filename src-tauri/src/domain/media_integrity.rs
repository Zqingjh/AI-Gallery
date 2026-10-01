use serde::{Deserialize, Serialize};

use super::{MediaType, PathKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CoverSourceType {
    KeyFrame,
    Custom,
}

impl CoverSourceType {
    pub(crate) fn as_db_str(self) -> &'static str {
        match self {
            Self::KeyFrame => "key_frame",
            Self::Custom => "custom",
        }
    }
}

/// 可跨 IPC 返回的封面元数据。内部相对路径不属于公开字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AssetCover {
    pub(crate) asset_id: i64,
    pub(crate) source_type: CoverSourceType,
    pub(crate) frame_timestamp_ms: Option<i64>,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

/// 数据层内部封面记录；`stored_path` 必须是工作区内的规范相对路径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AssetCoverRecord {
    pub(crate) cover: AssetCover,
    pub(crate) stored_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SaveAssetCover {
    pub(crate) asset_id: i64,
    pub(crate) source_type: CoverSourceType,
    pub(crate) stored_path: String,
    pub(crate) frame_timestamp_ms: Option<i64>,
}

/// 路径修复读取的内部快照。该类型不得直接序列化到 IPC 或日志。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AssetPathRecord {
    pub(crate) asset_id: i64,
    pub(crate) media_type: MediaType,
    pub(crate) path_kind: PathKind,
    pub(crate) stored_path: String,
    pub(crate) file_name: String,
    pub(crate) file_size: Option<i64>,
    pub(crate) content_hash: Option<String>,
    pub(crate) updated_at: i64,
}

/// 执行路径修复时使用的乐观锁输入；路径模式不可在修复过程中改变。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AssetPathRepairUpdate {
    pub(crate) asset_id: i64,
    pub(crate) expected_updated_at: i64,
    pub(crate) expected_path_kind: PathKind,
    pub(crate) stored_path: String,
    pub(crate) file_name: String,
    pub(crate) file_size: Option<i64>,
    pub(crate) content_hash: String,
}
