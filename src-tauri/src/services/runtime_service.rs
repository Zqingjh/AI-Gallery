use crate::domain::RuntimeInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeServiceError {
    RuntimeInfoUnavailable,
}

#[derive(Debug, Default)]
pub(crate) struct RuntimeService;

impl RuntimeService {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn get_runtime_info(&self) -> Result<RuntimeInfo, RuntimeServiceError> {
        let app_version = env!("CARGO_PKG_VERSION");
        if app_version.is_empty() {
            return Err(RuntimeServiceError::RuntimeInfoUnavailable);
        }

        Ok(RuntimeInfo {
            app_version: app_version.to_owned(),
            platform: std::env::consts::OS.to_owned(),
            architecture: std::env::consts::ARCH.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::RuntimeService;

    #[test]
    fn returns_compile_time_runtime_information_without_io() {
        let info = RuntimeService::new()
            .get_runtime_info()
            .expect("编译期运行信息应始终可用");

        assert_eq!(info.app_version, env!("CARGO_PKG_VERSION"));
        assert!(!info.platform.is_empty());
        assert!(!info.architecture.is_empty());
    }
}
