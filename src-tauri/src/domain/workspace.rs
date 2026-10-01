pub(crate) const WORKSPACE_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkspaceInfo {
    pub(crate) display_name: String,
    pub(crate) format_version: u32,
    pub(crate) ready: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StoredPathKind {
    Managed,
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PathAvailability {
    Available,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredPathStatus {
    pub(crate) kind: StoredPathKind,
    pub(crate) availability: PathAvailability,
    pub(crate) portable_path: Option<String>,
}
