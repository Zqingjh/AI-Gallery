use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{
    commands::CommandErrorDto,
    domain::{BackupInfo, BackupKind},
    services::{BackupService, BackupServiceError},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CreateWorkspaceBackupRequestDto {
    root_path: String,
    destination_parent_path: String,
    kind: BackupKindDto,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RestoreWorkspaceBackupRequestDto {
    backup_root_path: String,
    target_root_path: String,
    confirmed: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum BackupKindDto {
    Full,
    Light,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceBackupDto {
    kind: BackupKindDto,
    backup_name: String,
    managed_asset_count: u64,
    external_asset_count: u64,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RestoreWorkspaceBackupDto {
    kind: BackupKindDto,
    restored: bool,
    target_database_backed_up: bool,
}

impl From<BackupKindDto> for BackupKind {
    fn from(value: BackupKindDto) -> Self {
        match value {
            BackupKindDto::Full => Self::Full,
            BackupKindDto::Light => Self::Lightweight,
        }
    }
}

impl From<BackupKind> for BackupKindDto {
    fn from(value: BackupKind) -> Self {
        match value {
            BackupKind::Full => Self::Full,
            BackupKind::Lightweight => Self::Light,
        }
    }
}

impl From<BackupServiceError> for CommandErrorDto {
    fn from(error: BackupServiceError) -> Self {
        match error {
            BackupServiceError::InvalidPath => {
                Self::new("BACKUP_INVALID_INPUT", "备份或恢复位置无效。")
            }
            BackupServiceError::DestinationExists => {
                Self::new("BACKUP_CONFLICT", "目标位置已存在或与现有内容冲突。")
            }
            BackupServiceError::Workspace(_) => Self::new("BACKUP_FAILED", "无法完成工作区备份。"),
            BackupServiceError::SnapshotMissing
            | BackupServiceError::SnapshotInvalid
            | BackupServiceError::SnapshotVersionUnsupported
            | BackupServiceError::SnapshotUnsafe
            | BackupServiceError::DatabaseInvalid => Self::new(
                "BACKUP_RESTORE_FAILED",
                "无法完成工作区恢复，目标内容未被覆盖。",
            ),
            BackupServiceError::CreateFailed
            | BackupServiceError::CopyFailed
            | BackupServiceError::CommitFailed
            | BackupServiceError::CleanupFailed => {
                Self::new("BACKUP_FAILED", "无法完成工作区备份。")
            }
        }
    }
}

#[tauri::command]
pub(crate) fn create_workspace_backup(
    request: CreateWorkspaceBackupRequestDto,
) -> Result<WorkspaceBackupDto, CommandErrorDto> {
    let kind: BackupKind = request.kind.into();
    let backup_name = reserve_backup_name(Path::new(&request.destination_parent_path), kind)?;
    let destination = Path::new(&request.destination_parent_path).join(&backup_name);
    BackupService::new()
        .create_backup(Path::new(&request.root_path), &destination, kind)
        .map(|info| backup_dto(info, backup_name))
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub(crate) fn restore_workspace_backup(
    request: RestoreWorkspaceBackupRequestDto,
) -> Result<RestoreWorkspaceBackupDto, CommandErrorDto> {
    if !request.confirmed {
        return Err(CommandErrorDto::new(
            "BACKUP_INVALID_INPUT",
            "备份或恢复位置无效。",
        ));
    }
    BackupService::new()
        .restore_backup(
            Path::new(&request.backup_root_path),
            Path::new(&request.target_root_path),
        )
        .map(|info| RestoreWorkspaceBackupDto {
            kind: info.kind.into(),
            restored: true,
            // 当前恢复仅允许新目标，因此不存在覆盖旧数据库的路径。
            target_database_backed_up: false,
        })
        .map_err(CommandErrorDto::from)
}

fn backup_dto(info: BackupInfo, backup_name: String) -> WorkspaceBackupDto {
    WorkspaceBackupDto {
        kind: info.kind.into(),
        backup_name,
        managed_asset_count: info.managed_asset_count,
        external_asset_count: info.external_asset_count,
    }
}

fn reserve_backup_name(parent: &Path, kind: BackupKind) -> Result<String, CommandErrorDto> {
    if !parent.is_absolute() {
        return Err(CommandErrorDto::new(
            "BACKUP_INVALID_INPUT",
            "备份或恢复位置无效。",
        ));
    }
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CommandErrorDto::new("BACKUP_FAILED", "无法完成工作区备份。"))?
        .as_millis();
    let kind_label = match kind {
        BackupKind::Full => "full",
        BackupKind::Lightweight => "light",
    };
    for suffix in 0_u16..100 {
        let name = if suffix == 0 {
            format!("AI-Gallery-{kind_label}-backup-{timestamp}")
        } else {
            format!("AI-Gallery-{kind_label}-backup-{timestamp}-{suffix}")
        };
        let candidate: PathBuf = parent.join(&name);
        match candidate.try_exists() {
            Ok(false) => return Ok(name),
            Ok(true) => continue,
            Err(_) => break,
        }
    }
    Err(CommandErrorDto::new(
        "BACKUP_CONFLICT",
        "目标位置已存在或与现有内容冲突。",
    ))
}

#[cfg(test)]
mod tests {
    use super::{BackupKindDto, backup_dto};
    use crate::domain::{BackupInfo, BackupKind};

    #[test]
    fn backup_dto_does_not_expose_a_path() {
        let value = serde_json::to_value(backup_dto(
            BackupInfo {
                kind: BackupKind::Full,
                schema_version: 5,
                managed_asset_count: 2,
                external_asset_count: 1,
            },
            "AI-Gallery-full-backup-1".to_owned(),
        ))
        .expect("备份 DTO 应可序列化");
        assert_eq!(value["kind"], "full");
        assert_eq!(value["managedAssetCount"], 2);
        assert!(value.get("path").is_none());
        assert_eq!(
            BackupKindDto::from(BackupKind::Lightweight),
            BackupKindDto::Light
        );
    }
}
