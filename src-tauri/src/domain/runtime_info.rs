#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeInfo {
    pub(crate) app_version: String,
    pub(crate) platform: String,
    pub(crate) architecture: String,
}
