use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{
    commands::CommandErrorDto,
    domain::{AccessMode, PathAvailability, StoredPathKind, StoredPathStatus, WorkspaceInfo},
    services::{
        AccessModeService, AccessModeServiceError, WorkspaceService, WorkspaceServiceError,
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceRequestDto {
    root_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct IsolatedWorkspaceRequestDto {
    primary_root_path: String,
    isolated_root_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SetWorkspaceAccessModeRequestDto {
    root_path: String,
    mode: AccessModeDto,
    confirmed: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum AccessModeDto {
    ReadWrite,
    ReadOnly,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceAccessModeDto {
    mode: AccessModeDto,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceInfoDto {
    display_name: String,
    format_version: u32,
    ready: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum StoredPathKindDto {
    Managed,
    External,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoredPathRequestDto {
    workspace_root: String,
    kind: StoredPathKindDto,
    stored_path: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredPathStatusDto {
    kind: StoredPathKindDto,
    availability: &'static str,
    portable_path: Option<String>,
}

impl From<WorkspaceInfo> for WorkspaceInfoDto {
    fn from(info: WorkspaceInfo) -> Self {
        Self {
            display_name: info.display_name,
            format_version: info.format_version,
            ready: info.ready,
        }
    }
}

impl From<StoredPathKindDto> for StoredPathKind {
    fn from(kind: StoredPathKindDto) -> Self {
        match kind {
            StoredPathKindDto::Managed => Self::Managed,
            StoredPathKindDto::External => Self::External,
        }
    }
}

impl From<AccessMode> for AccessModeDto {
    fn from(value: AccessMode) -> Self {
        match value {
            AccessMode::ReadWrite => Self::ReadWrite,
            AccessMode::ReadOnly => Self::ReadOnly,
        }
    }
}

impl From<AccessModeDto> for AccessMode {
    fn from(value: AccessModeDto) -> Self {
        match value {
            AccessModeDto::ReadWrite => Self::ReadWrite,
            AccessModeDto::ReadOnly => Self::ReadOnly,
        }
    }
}

impl From<AccessModeServiceError> for CommandErrorDto {
    fn from(error: AccessModeServiceError) -> Self {
        match error {
            AccessModeServiceError::ReadOnly => Self::new(
                "WORKSPACE_READ_ONLY",
                "工作区正处于只读展示模式，无法修改内容。",
            ),
            AccessModeServiceError::Workspace(error) => Self::from(error),
            AccessModeServiceError::DatabaseUnavailable
            | AccessModeServiceError::InvalidSetting
            | AccessModeServiceError::ClockUnavailable => {
                Self::new("WORKSPACE_OPEN_FAILED", "工作区打开失败。")
            }
        }
    }
}

impl From<StoredPathStatus> for StoredPathStatusDto {
    fn from(status: StoredPathStatus) -> Self {
        Self {
            kind: match status.kind {
                StoredPathKind::Managed => StoredPathKindDto::Managed,
                StoredPathKind::External => StoredPathKindDto::External,
            },
            availability: match status.availability {
                PathAvailability::Available => "available",
                PathAvailability::Missing => "missing",
            },
            portable_path: status.portable_path,
        }
    }
}

impl From<WorkspaceServiceError> for CommandErrorDto {
    fn from(error: WorkspaceServiceError) -> Self {
        match error {
            WorkspaceServiceError::InvalidPath => {
                Self::new("INVALID_WORKSPACE_PATH", "请选择有效的工作区位置。")
            }
            WorkspaceServiceError::AlreadyExists => Self::new(
                "WORKSPACE_ALREADY_EXISTS",
                "所选位置已存在，无法创建工作区。",
            ),
            WorkspaceServiceError::NotFound => Self::new("WORKSPACE_NOT_FOUND", "未找到工作区。"),
            WorkspaceServiceError::ManifestMissing => {
                Self::new("WORKSPACE_MANIFEST_MISSING", "工作区标记文件缺失。")
            }
            WorkspaceServiceError::ManifestTooLarge => Self::new(
                "WORKSPACE_MANIFEST_TOO_LARGE",
                "工作区标记文件超出大小限制。",
            ),
            WorkspaceServiceError::ManifestInvalid => {
                Self::new("WORKSPACE_MANIFEST_INVALID", "工作区标记文件无法识别。")
            }
            WorkspaceServiceError::VersionUnsupported => Self::new(
                "WORKSPACE_VERSION_UNSUPPORTED",
                "当前版本无法打开此工作区。",
            ),
            WorkspaceServiceError::StructureInvalid => {
                Self::new("WORKSPACE_STRUCTURE_INVALID", "工作区目录结构不完整。")
            }
            WorkspaceServiceError::CreateFailed => {
                Self::new("WORKSPACE_CREATE_FAILED", "工作区创建失败。")
            }
            WorkspaceServiceError::OpenFailed => {
                Self::new("WORKSPACE_OPEN_FAILED", "工作区打开失败。")
            }
            WorkspaceServiceError::RollbackFailed => Self::new(
                "WORKSPACE_ROLLBACK_FAILED",
                "工作区创建失败，且未能完整清理临时内容。",
            ),
            WorkspaceServiceError::WorkspaceOverlap => Self::new(
                "WORKSPACE_PATH_CONFLICT",
                "私密工作区必须与正常工作区使用互不嵌套的独立目录。",
            ),
        }
    }
}

#[tauri::command]
pub(crate) fn create_workspace(
    request: WorkspaceRequestDto,
) -> Result<WorkspaceInfoDto, CommandErrorDto> {
    WorkspaceService::new()
        .create_workspace(Path::new(&request.root_path))
        .map(WorkspaceInfoDto::from)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn open_workspace(
    request: WorkspaceRequestDto,
) -> Result<WorkspaceInfoDto, CommandErrorDto> {
    WorkspaceService::new()
        .open_workspace(Path::new(&request.root_path))
        .map(WorkspaceInfoDto::from)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn validate_isolated_workspace(
    request: IsolatedWorkspaceRequestDto,
) -> Result<(), CommandErrorDto> {
    WorkspaceService::new()
        .validate_isolated_workspace(
            Path::new(&request.primary_root_path),
            Path::new(&request.isolated_root_path),
        )
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn get_workspace_access_mode(
    request: WorkspaceRequestDto,
) -> Result<WorkspaceAccessModeDto, CommandErrorDto> {
    AccessModeService::new()
        .get_mode(Path::new(&request.root_path))
        .map(|mode| WorkspaceAccessModeDto { mode: mode.into() })
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn set_workspace_access_mode(
    request: SetWorkspaceAccessModeRequestDto,
) -> Result<WorkspaceAccessModeDto, CommandErrorDto> {
    if !request.confirmed {
        return Err(CommandErrorDto::new(
            "WORKSPACE_READ_ONLY",
            "工作区正处于只读展示模式，无法修改内容。",
        ));
    }
    AccessModeService::new()
        .set_mode(Path::new(&request.root_path), request.mode.into())
        .map(|mode| WorkspaceAccessModeDto { mode: mode.into() })
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn check_stored_path(
    request: StoredPathRequestDto,
) -> Result<StoredPathStatusDto, CommandErrorDto> {
    WorkspaceService::new()
        .check_stored_path(
            Path::new(&request.workspace_root),
            request.kind.into(),
            Path::new(&request.stored_path),
        )
        .map(StoredPathStatusDto::from)
        .map_err(CommandErrorDto::from)
}

#[cfg(test)]
mod tests {
    use super::{
        AccessModeDto, StoredPathStatusDto, WorkspaceAccessModeDto, WorkspaceInfoDto,
        WorkspaceRequestDto,
    };
    use crate::{
        commands::CommandErrorDto,
        domain::{PathAvailability, StoredPathKind, StoredPathStatus, WorkspaceInfo},
        services::WorkspaceServiceError,
    };

    #[test]
    fn serializes_workspace_contract_with_camel_case_fields() {
        let dto = WorkspaceInfoDto::from(WorkspaceInfo {
            display_name: "作品库".to_owned(),
            format_version: 1,
            ready: true,
        });
        let value = serde_json::to_value(dto).expect("工作区 DTO 应可序列化");

        assert_eq!(value["displayName"], "作品库");
        assert_eq!(value["formatVersion"], 1);
        assert_eq!(value["ready"], true);
        assert!(value.get("display_name").is_none());
    }

    #[test]
    fn maps_workspace_errors_to_stable_safe_dto() {
        let dto = CommandErrorDto::from(WorkspaceServiceError::OpenFailed);

        assert_eq!(dto.code, "WORKSPACE_OPEN_FAILED");
        assert_eq!(dto.message, "工作区打开失败。");
        assert!(!dto.message.contains(':'));
    }

    #[test]
    fn rejects_missing_and_unknown_workspace_request_fields() {
        let missing_root = serde_json::from_value::<WorkspaceRequestDto>(serde_json::json!({}));
        let unknown_field = serde_json::from_value::<WorkspaceRequestDto>(serde_json::json!({
            "rootPath": "D:\\Gallery",
            "debugPath": "D:\\private\\secret.db"
        }));

        assert!(missing_root.is_err());
        assert!(unknown_field.is_err());
    }

    #[test]
    fn every_workspace_error_dto_contains_only_stable_safe_fields() {
        let errors = [
            WorkspaceServiceError::InvalidPath,
            WorkspaceServiceError::AlreadyExists,
            WorkspaceServiceError::NotFound,
            WorkspaceServiceError::ManifestMissing,
            WorkspaceServiceError::ManifestTooLarge,
            WorkspaceServiceError::ManifestInvalid,
            WorkspaceServiceError::VersionUnsupported,
            WorkspaceServiceError::StructureInvalid,
            WorkspaceServiceError::CreateFailed,
            WorkspaceServiceError::OpenFailed,
            WorkspaceServiceError::RollbackFailed,
            WorkspaceServiceError::WorkspaceOverlap,
        ];

        for error in errors {
            let value =
                serde_json::to_value(CommandErrorDto::from(error)).expect("错误 DTO 应可序列化");
            let object = value.as_object().expect("错误 DTO 应为对象");
            let message = value["message"].as_str().expect("应包含安全消息");

            assert_eq!(object.len(), 2);
            assert!(object.contains_key("code"));
            assert!(object.contains_key("message"));
            assert!(!message.contains("D:\\"));
            assert!(!message.contains("/Users/"));
        }
    }

    #[test]
    fn external_path_status_never_serializes_the_absolute_path() {
        let dto = StoredPathStatusDto::from(StoredPathStatus {
            kind: StoredPathKind::External,
            availability: PathAvailability::Missing,
            portable_path: None,
        });
        let value = serde_json::to_value(dto).expect("路径状态 DTO 应可序列化");

        assert_eq!(value["kind"], "external");
        assert_eq!(value["availability"], "missing");
        assert!(value["portablePath"].is_null());
    }

    #[test]
    fn access_mode_dto_uses_only_the_stable_mode_field() {
        let value = serde_json::to_value(WorkspaceAccessModeDto {
            mode: AccessModeDto::ReadOnly,
        })
        .expect("访问模式 DTO 应可序列化");
        assert_eq!(value["mode"], "readOnly");
        assert_eq!(value.as_object().expect("应为对象").len(), 1);
    }
}
