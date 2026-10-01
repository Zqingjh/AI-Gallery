use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::Connection;

use crate::{
    domain::{
        CustomField, CustomFieldPage, CustomFieldTargetType, CustomFieldValue,
        CustomFieldValueInput, CustomFieldValuePage, EditHistoryPage, EditHistoryTargetType,
        P1PageCursor, PromptText, PromptVersion, PromptVersionCursor, PromptVersionPage,
        SaveCustomField, SaveSavedFilter, SavedFilter, SavedFilterPage,
    },
    repositories::{P1LibraryRepository, P1LibraryRepositoryError},
    services::{
        AccessModeServiceError, DatabaseService, DatabaseServiceError, WriteAccessGuard,
        WriteAccessLease,
    },
};

const MAX_PAGE_LIMIT: u32 = 100;
const MAX_CUSTOM_FIELD_TARGET_IDS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum P1LibraryServiceError {
    InvalidInput,
    NotFound,
    Conflict,
    DatabaseUnavailable,
    DataInvalid,
    ReadOnly,
}

/// P1 工作流编排层：所有写入先取得只读保护租约，再交给事务化仓储。
pub(crate) struct P1LibraryService;

struct WritableConnection {
    connection: Connection,
    _lease: WriteAccessLease,
}

impl std::ops::Deref for WritableConnection {
    type Target = Connection;
    fn deref(&self) -> &Self::Target {
        &self.connection
    }
}
impl std::ops::DerefMut for WritableConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.connection
    }
}

impl P1LibraryService {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn save_saved_filter(
        &self,
        root: &Path,
        input: &SaveSavedFilter,
    ) -> Result<SavedFilter, P1LibraryServiceError> {
        validate_name(&input.name)?;
        let mut connection = self.open_writable(root)?;
        P1LibraryRepository::save_saved_filter(&mut connection, input, now()?)
            .map_err(map_repository_error)
    }
    pub(crate) fn list_saved_filters(
        &self,
        root: &Path,
        cursor: Option<P1PageCursor>,
        limit: u32,
    ) -> Result<SavedFilterPage, P1LibraryServiceError> {
        validate_page(cursor, limit)?;
        P1LibraryRepository::list_saved_filters(&self.open(root)?, cursor, limit)
            .map_err(map_repository_error)
    }
    pub(crate) fn delete_saved_filter(
        &self,
        root: &Path,
        id: i64,
    ) -> Result<(), P1LibraryServiceError> {
        validate_id(id)?;
        let mut connection = self.open_writable(root)?;
        P1LibraryRepository::delete_saved_filter(&mut connection, id).map_err(map_repository_error)
    }
    pub(crate) fn save_custom_field(
        &self,
        root: &Path,
        input: &SaveCustomField,
    ) -> Result<CustomField, P1LibraryServiceError> {
        validate_name(&input.name)?;
        if !input.options.is_object() {
            return Err(P1LibraryServiceError::InvalidInput);
        }
        let mut connection = self.open_writable(root)?;
        P1LibraryRepository::save_custom_field(&mut connection, input, now()?)
            .map_err(map_repository_error)
    }
    pub(crate) fn list_custom_fields(
        &self,
        root: &Path,
        target: CustomFieldTargetType,
        cursor: Option<P1PageCursor>,
        limit: u32,
    ) -> Result<CustomFieldPage, P1LibraryServiceError> {
        validate_page(cursor, limit)?;
        P1LibraryRepository::list_custom_fields(&self.open(root)?, target, cursor, limit)
            .map_err(map_repository_error)
    }
    pub(crate) fn save_manual_custom_field_value(
        &self,
        root: &Path,
        input: &CustomFieldValueInput,
    ) -> Result<CustomFieldValue, P1LibraryServiceError> {
        validate_id(input.field_id)?;
        validate_id(input.target_id)?;
        let mut connection = self.open_writable(root)?;
        P1LibraryRepository::save_manual_custom_field_value(&mut connection, input, now()?)
            .map_err(map_repository_error)
    }
    pub(crate) fn confirm_pending_custom_field_value(
        &self,
        root: &Path,
        id: i64,
    ) -> Result<CustomFieldValue, P1LibraryServiceError> {
        validate_id(id)?;
        let mut connection = self.open_writable(root)?;
        P1LibraryRepository::confirm_pending_custom_field_value(&mut connection, id, now()?)
            .map_err(map_repository_error)
    }
    pub(crate) fn list_custom_field_values(
        &self,
        root: &Path,
        target: CustomFieldTargetType,
        target_id: i64,
        cursor: Option<P1PageCursor>,
        limit: u32,
    ) -> Result<CustomFieldValuePage, P1LibraryServiceError> {
        validate_id(target_id)?;
        validate_page(cursor, limit)?;
        P1LibraryRepository::list_custom_field_values(
            &self.open(root)?,
            target,
            target_id,
            cursor,
            limit,
        )
        .map_err(map_repository_error)
    }
    pub(crate) fn list_custom_field_values_for_targets(
        &self,
        root: &Path,
        target: CustomFieldTargetType,
        target_ids: &[i64],
        cursor: Option<P1PageCursor>,
        limit: u32,
    ) -> Result<CustomFieldValuePage, P1LibraryServiceError> {
        validate_page(cursor, limit)?;
        if target_ids.len() > MAX_CUSTOM_FIELD_TARGET_IDS || target_ids.iter().any(|id| *id <= 0) {
            return Err(P1LibraryServiceError::InvalidInput);
        }
        P1LibraryRepository::list_custom_field_values_for_targets(
            &self.open(root)?,
            target,
            target_ids,
            cursor,
            limit,
        )
        .map_err(map_repository_error)
    }
    pub(crate) fn create_prompt_version(
        &self,
        root: &Path,
        prompt_id: i64,
        prompt: &PromptText,
    ) -> Result<PromptVersion, P1LibraryServiceError> {
        validate_id(prompt_id)?;
        let mut connection = self.open_writable(root)?;
        P1LibraryRepository::create_prompt_version(&mut connection, prompt_id, prompt, now()?)
            .map_err(map_repository_error)
    }
    pub(crate) fn list_prompt_versions(
        &self,
        root: &Path,
        prompt_id: i64,
        cursor: Option<PromptVersionCursor>,
        limit: u32,
    ) -> Result<PromptVersionPage, P1LibraryServiceError> {
        validate_id(prompt_id)?;
        if limit == 0 || limit > MAX_PAGE_LIMIT {
            return Err(P1LibraryServiceError::InvalidInput);
        }
        P1LibraryRepository::list_prompt_versions(&self.open(root)?, prompt_id, cursor, limit)
            .map_err(map_repository_error)
    }
    pub(crate) fn list_edit_history(
        &self,
        root: &Path,
        target: EditHistoryTargetType,
        target_id: i64,
        cursor: Option<P1PageCursor>,
        limit: u32,
    ) -> Result<EditHistoryPage, P1LibraryServiceError> {
        validate_id(target_id)?;
        validate_page(cursor, limit)?;
        P1LibraryRepository::list_edit_history(&self.open(root)?, target, target_id, cursor, limit)
            .map_err(map_repository_error)
    }

    fn open(&self, root: &Path) -> Result<Connection, P1LibraryServiceError> {
        DatabaseService::new()
            .open_current_workspace_database(root)
            .map_err(map_database_error)
    }
    fn open_writable(&self, root: &Path) -> Result<WritableConnection, P1LibraryServiceError> {
        let lease = WriteAccessGuard::acquire_writable(root).map_err(map_access_error)?;
        Ok(WritableConnection {
            connection: self.open(root)?,
            _lease: lease,
        })
    }
}

fn validate_id(id: i64) -> Result<(), P1LibraryServiceError> {
    if id > 0 {
        Ok(())
    } else {
        Err(P1LibraryServiceError::InvalidInput)
    }
}
fn validate_name(name: &str) -> Result<(), P1LibraryServiceError> {
    if name.trim().is_empty() || name.chars().count() > 200 {
        Err(P1LibraryServiceError::InvalidInput)
    } else {
        Ok(())
    }
}
fn validate_page(cursor: Option<P1PageCursor>, limit: u32) -> Result<(), P1LibraryServiceError> {
    if limit == 0
        || limit > MAX_PAGE_LIMIT
        || cursor.is_some_and(|value| value.id <= 0 || value.updated_at < 0)
    {
        Err(P1LibraryServiceError::InvalidInput)
    } else {
        Ok(())
    }
}
fn now() -> Result<i64, P1LibraryServiceError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| P1LibraryServiceError::DatabaseUnavailable)
        .and_then(|value| {
            i64::try_from(value.as_millis()).map_err(|_| P1LibraryServiceError::DatabaseUnavailable)
        })
}
fn map_repository_error(error: P1LibraryRepositoryError) -> P1LibraryServiceError {
    match error {
        P1LibraryRepositoryError::NotFound => P1LibraryServiceError::NotFound,
        P1LibraryRepositoryError::Conflict => P1LibraryServiceError::Conflict,
        P1LibraryRepositoryError::InvalidData => P1LibraryServiceError::DataInvalid,
        P1LibraryRepositoryError::DatabaseFailed => P1LibraryServiceError::DatabaseUnavailable,
    }
}
fn map_database_error(_: DatabaseServiceError) -> P1LibraryServiceError {
    P1LibraryServiceError::DatabaseUnavailable
}
fn map_access_error(error: AccessModeServiceError) -> P1LibraryServiceError {
    match error {
        AccessModeServiceError::ReadOnly => P1LibraryServiceError::ReadOnly,
        _ => P1LibraryServiceError::DatabaseUnavailable,
    }
}
