#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DatabaseStatus {
    pub(crate) schema_version: u32,
    pub(crate) migrated: bool,
    pub(crate) backup_created: bool,
}

pub(crate) const DATABASE_SCHEMA_VERSION: u32 = 10;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AccessMode {
    ReadWrite,
    ReadOnly,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackupKind {
    Full,
    Lightweight,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BackupInfo {
    pub(crate) kind: BackupKind,
    pub(crate) schema_version: u32,
    pub(crate) managed_asset_count: u64,
    pub(crate) external_asset_count: u64,
}
