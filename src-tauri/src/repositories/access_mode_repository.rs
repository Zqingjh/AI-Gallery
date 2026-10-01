use rusqlite::{Connection, OptionalExtension, params};

use crate::domain::AccessMode;

const ACCESS_MODE_SETTING_KEY: &str = "workspace.accessMode";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AccessModeRepositoryError {
    DatabaseFailed,
    InvalidData,
}

pub(crate) struct AccessModeRepository;

impl AccessModeRepository {
    pub(crate) fn get(connection: &Connection) -> Result<AccessMode, AccessModeRepositoryError> {
        let stored: Option<String> = connection
            .query_row(
                "SELECT value_json FROM app_settings WHERE key = ?1",
                [ACCESS_MODE_SETTING_KEY],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| AccessModeRepositoryError::DatabaseFailed)?;

        match stored.as_deref() {
            None => Ok(AccessMode::ReadWrite),
            Some("\"readWrite\"") => Ok(AccessMode::ReadWrite),
            Some("\"readOnly\"") => Ok(AccessMode::ReadOnly),
            Some(_) => Err(AccessModeRepositoryError::InvalidData),
        }
    }

    pub(crate) fn set(
        connection: &mut Connection,
        mode: AccessMode,
        updated_at: i64,
    ) -> Result<(), AccessModeRepositoryError> {
        let value = match mode {
            AccessMode::ReadWrite => "\"readWrite\"",
            AccessMode::ReadOnly => "\"readOnly\"",
        };
        connection
            .execute(
                "INSERT INTO app_settings (key, value_json, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json,
                   updated_at = excluded.updated_at",
                params![ACCESS_MODE_SETTING_KEY, value, updated_at],
            )
            .map_err(|_| AccessModeRepositoryError::DatabaseFailed)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::{AccessModeRepository, AccessModeRepositoryError};
    use crate::domain::AccessMode;

    #[test]
    fn defaults_to_read_write_and_round_trips_a_strongly_typed_mode() {
        let mut connection = Connection::open_in_memory().expect("应能创建内存数据库");
        connection
            .execute_batch(
                "CREATE TABLE app_settings (
                    key TEXT PRIMARY KEY,
                    value_json TEXT NOT NULL,
                    updated_at INTEGER NOT NULL
                ) WITHOUT ROWID;",
            )
            .expect("应能创建设置表");

        assert_eq!(
            AccessModeRepository::get(&connection).expect("缺省模式应可读取"),
            AccessMode::ReadWrite
        );
        AccessModeRepository::set(&mut connection, AccessMode::ReadOnly, 7)
            .expect("应能写入只读模式");
        assert_eq!(
            AccessModeRepository::get(&connection).expect("模式应可再次读取"),
            AccessMode::ReadOnly
        );
    }

    #[test]
    fn rejects_malformed_persisted_values() {
        let connection = Connection::open_in_memory().expect("应能创建内存数据库");
        connection
            .execute_batch(
                "CREATE TABLE app_settings (
                    key TEXT PRIMARY KEY,
                    value_json TEXT NOT NULL,
                    updated_at INTEGER NOT NULL
                ) WITHOUT ROWID;
                INSERT INTO app_settings VALUES ('workspace.accessMode', '\"unsafe\"', 1);",
            )
            .expect("应能准备损坏设置");

        assert_eq!(
            AccessModeRepository::get(&connection),
            Err(AccessModeRepositoryError::InvalidData)
        );
    }
}
