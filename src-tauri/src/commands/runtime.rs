use serde::Serialize;

use crate::{
    commands::CommandErrorDto,
    domain::RuntimeInfo,
    services::{RuntimeService, RuntimeServiceError},
};

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeInfoDto {
    app_version: String,
    platform: String,
    architecture: String,
}

impl From<RuntimeInfo> for RuntimeInfoDto {
    fn from(info: RuntimeInfo) -> Self {
        Self {
            app_version: info.app_version,
            platform: info.platform,
            architecture: info.architecture,
        }
    }
}

impl From<RuntimeServiceError> for CommandErrorDto {
    fn from(error: RuntimeServiceError) -> Self {
        match error {
            RuntimeServiceError::RuntimeInfoUnavailable => {
                Self::new("RUNTIME_INFO_UNAVAILABLE", "无法获取运行环境信息。")
            }
        }
    }
}

#[tauri::command]
pub(crate) fn get_runtime_info() -> Result<RuntimeInfoDto, CommandErrorDto> {
    RuntimeService::new()
        .get_runtime_info()
        .map(RuntimeInfoDto::from)
        .map_err(CommandErrorDto::from)
}

#[cfg(test)]
mod tests {
    use super::get_runtime_info;
    use crate::{commands::CommandErrorDto, services::RuntimeServiceError};

    #[test]
    fn maps_service_errors_to_stable_safe_dto() {
        let dto = CommandErrorDto::from(RuntimeServiceError::RuntimeInfoUnavailable);

        assert_eq!(dto.code, "RUNTIME_INFO_UNAVAILABLE");
        assert_eq!(dto.message, "无法获取运行环境信息。");
    }

    #[test]
    fn command_returns_only_public_runtime_fields() {
        let dto = get_runtime_info().expect("编译期运行信息应始终可用");

        assert!(!dto.app_version.is_empty());
        assert!(!dto.platform.is_empty());
        assert!(!dto.architecture.is_empty());
    }

    #[test]
    fn serializes_command_contract_with_camel_case_fields() {
        let dto = get_runtime_info().expect("编译期运行信息应始终可用");
        let value = serde_json::to_value(dto).expect("运行信息 DTO 应可序列化");

        assert!(value.get("appVersion").is_some());
        assert!(value.get("platform").is_some());
        assert!(value.get("architecture").is_some());
        assert!(value.get("app_version").is_none());
    }
}
