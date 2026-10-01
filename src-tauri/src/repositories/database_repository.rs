use std::{path::Path, thread, time::Duration};

use rusqlite::{
    Connection,
    backup::{Backup, StepResult},
};

const V1_SCHEMA_VERSION: u32 = 1;
const V2_SCHEMA_VERSION: u32 = 2;
const V3_SCHEMA_VERSION: u32 = 3;
const V4_SCHEMA_VERSION: u32 = 4;
const V5_SCHEMA_VERSION: u32 = 5;
const V6_SCHEMA_VERSION: u32 = 6;
const V7_SCHEMA_VERSION: u32 = 7;
const V8_SCHEMA_VERSION: u32 = 8;
const V9_SCHEMA_VERSION: u32 = 9;
const V10_SCHEMA_VERSION: u32 = 10;

const REQUIRED_V1_TABLES: [&str; 15] = [
    "projects",
    "assets",
    "prompts",
    "models",
    "platforms",
    "dimensions",
    "categories",
    "project_categories",
    "asset_categories",
    "tags",
    "project_tags",
    "asset_tags",
    "ai_suggestions",
    "trash_entries",
    "app_settings",
];
const REQUIRED_V3_TABLES: [&str; 16] = [
    "projects",
    "assets",
    "prompts",
    "models",
    "platforms",
    "dimensions",
    "categories",
    "project_categories",
    "asset_categories",
    "tags",
    "project_tags",
    "asset_tags",
    "ai_suggestions",
    "ai_providers",
    "trash_entries",
    "app_settings",
];
const REQUIRED_V4_TABLES: [&str; 20] = [
    "projects",
    "assets",
    "prompts",
    "models",
    "platforms",
    "dimensions",
    "categories",
    "project_categories",
    "asset_categories",
    "tags",
    "project_tags",
    "asset_tags",
    "ai_suggestions",
    "ai_providers",
    "trash_entries",
    "app_settings",
    "saved_filters",
    "custom_fields",
    "custom_field_values",
    "prompt_versions",
];
const REQUIRED_V5_TABLES: [&str; 21] = [
    "projects",
    "assets",
    "prompts",
    "models",
    "platforms",
    "dimensions",
    "categories",
    "project_categories",
    "asset_categories",
    "tags",
    "project_tags",
    "asset_tags",
    "ai_suggestions",
    "ai_providers",
    "trash_entries",
    "app_settings",
    "saved_filters",
    "custom_fields",
    "custom_field_values",
    "prompt_versions",
    "asset_covers",
];
const BACKUP_MAX_TRANSIENT_RETRIES: u16 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DatabaseRepositoryError {
    OpenFailed,
    ReadVersionFailed,
    MigrationFailed,
    SchemaInvalid,
    BackupFailed,
}

pub(crate) struct DatabaseRepository;

impl DatabaseRepository {
    pub(crate) fn open(path: &Path) -> Result<Connection, DatabaseRepositoryError> {
        let connection = Connection::open(path).map_err(|_| DatabaseRepositoryError::OpenFailed)?;
        connection
            .busy_timeout(Duration::from_secs(2))
            .and_then(|()| connection.pragma_update(None, "foreign_keys", true))
            .map_err(|_| DatabaseRepositoryError::OpenFailed)?;
        Ok(connection)
    }

    pub(crate) fn schema_version(connection: &Connection) -> Result<u32, DatabaseRepositoryError> {
        connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|_| DatabaseRepositoryError::ReadVersionFailed)
    }

    /// 从任一受支持旧版本升级到 v9，整个链路只提交一次。
    /// 末级失败时活库保持原 user_version 和原结构，预迁移备份仍保留。
    #[expect(
        clippy::too_many_arguments,
        reason = "迁移脚本逐个传入，便于测试单个版本升级失败时的原子回滚"
    )]
    pub(crate) fn migrate_to_v10_atomic(
        connection: &mut Connection,
        from_version: u32,
        initial_migration_sql: &str,
        search_migration_sql: &str,
        ai_migration_sql: &str,
        p1_library_migration_sql: &str,
        media_integrity_migration_sql: &str,
        asset_order_migration_sql: &str,
        common_taxonomy_migration_sql: &str,
        metadata_presets_migration_sql: &str,
        media_order_repair_migration_sql: &str,
        canvas_projects_migration_sql: &str,
    ) -> Result<(), DatabaseRepositoryError> {
        if from_version >= V10_SCHEMA_VERSION {
            return Err(DatabaseRepositoryError::MigrationFailed);
        }
        let transaction = connection
            .transaction()
            .map_err(|_| DatabaseRepositoryError::MigrationFailed)?;
        let apply =
            |transaction: &rusqlite::Transaction<'_>,
             sql: &str,
             version: u32,
             validate: fn(&Connection) -> Result<(), DatabaseRepositoryError>| {
                transaction
                    .execute_batch(sql)
                    .and_then(|()| transaction.pragma_update(None, "user_version", version))
                    .map_err(|_| DatabaseRepositoryError::MigrationFailed)?;
                validate(transaction)
            };
        if from_version == 0 {
            apply(
                &transaction,
                initial_migration_sql,
                V1_SCHEMA_VERSION,
                Self::validate_v1,
            )?;
        }
        if from_version <= V1_SCHEMA_VERSION {
            apply(
                &transaction,
                search_migration_sql,
                V2_SCHEMA_VERSION,
                Self::validate_v2,
            )?;
        }
        if from_version <= V2_SCHEMA_VERSION {
            apply(
                &transaction,
                ai_migration_sql,
                V3_SCHEMA_VERSION,
                Self::validate_v3,
            )?;
        }
        if from_version <= V3_SCHEMA_VERSION {
            apply(
                &transaction,
                p1_library_migration_sql,
                V4_SCHEMA_VERSION,
                Self::validate_v4,
            )?;
        }
        if from_version <= V4_SCHEMA_VERSION {
            apply(
                &transaction,
                media_integrity_migration_sql,
                V5_SCHEMA_VERSION,
                Self::validate_v5,
            )?;
        }
        if from_version <= V5_SCHEMA_VERSION {
            apply(
                &transaction,
                asset_order_migration_sql,
                V6_SCHEMA_VERSION,
                Self::validate_v6,
            )?;
        }
        if from_version <= V6_SCHEMA_VERSION {
            apply(
                &transaction,
                common_taxonomy_migration_sql,
                V7_SCHEMA_VERSION,
                Self::validate_v7,
            )?;
        }
        if from_version <= V7_SCHEMA_VERSION {
            apply(
                &transaction,
                metadata_presets_migration_sql,
                V8_SCHEMA_VERSION,
                Self::validate_v8,
            )?;
        }
        if from_version <= V8_SCHEMA_VERSION {
            apply(
                &transaction,
                media_order_repair_migration_sql,
                V9_SCHEMA_VERSION,
                Self::validate_v9,
            )?;
        }
        apply(
            &transaction,
            canvas_projects_migration_sql,
            V10_SCHEMA_VERSION,
            Self::validate_v10,
        )?;
        transaction
            .commit()
            .map_err(|_| DatabaseRepositoryError::MigrationFailed)
    }

    pub(crate) fn validate_v1(connection: &Connection) -> Result<(), DatabaseRepositoryError> {
        if Self::schema_version(connection)? != V1_SCHEMA_VERSION {
            return Err(DatabaseRepositoryError::SchemaInvalid);
        }

        let mut statement = connection
            .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")
            .map_err(|_| DatabaseRepositoryError::SchemaInvalid)?;
        for table_name in REQUIRED_V1_TABLES {
            let exists = statement
                .exists([table_name])
                .map_err(|_| DatabaseRepositoryError::SchemaInvalid)?;
            if !exists {
                return Err(DatabaseRepositoryError::SchemaInvalid);
            }
        }

        let foreign_keys_enabled: bool = connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .map_err(|_| DatabaseRepositoryError::SchemaInvalid)?;
        if !foreign_keys_enabled {
            return Err(DatabaseRepositoryError::SchemaInvalid);
        }
        Ok(())
    }

    pub(crate) fn validate_v2(connection: &Connection) -> Result<(), DatabaseRepositoryError> {
        if Self::schema_version(connection)? != V2_SCHEMA_VERSION {
            return Err(DatabaseRepositoryError::SchemaInvalid);
        }
        Self::validate_required_tables(connection, &REQUIRED_V1_TABLES)?;
        let fts_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='asset_search')",
                [],
                |row| row.get(0),
            )
            .map_err(|_| DatabaseRepositoryError::SchemaInvalid)?;
        if !fts_exists {
            return Err(DatabaseRepositoryError::SchemaInvalid);
        }
        let foreign_keys_enabled: bool = connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .map_err(|_| DatabaseRepositoryError::SchemaInvalid)?;
        if !foreign_keys_enabled {
            return Err(DatabaseRepositoryError::SchemaInvalid);
        }
        Ok(())
    }

    pub(crate) fn validate_v3(connection: &Connection) -> Result<(), DatabaseRepositoryError> {
        if Self::schema_version(connection)? != V3_SCHEMA_VERSION {
            return Err(DatabaseRepositoryError::SchemaInvalid);
        }
        Self::validate_required_tables(connection, &REQUIRED_V3_TABLES)?;
        let required_schema_objects = [
            ("table", "asset_search"),
            ("index", "idx_ai_suggestions_review_page"),
            (
                "trigger",
                "ai_suggestions_reject_pending_after_project_delete",
            ),
            (
                "trigger",
                "ai_suggestions_reject_pending_after_asset_delete",
            ),
        ];
        Self::validate_schema_objects(connection, &required_schema_objects)?;
        Self::validate_provider_columns(connection)?;
        let foreign_keys_enabled: bool = connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .map_err(|_| DatabaseRepositoryError::SchemaInvalid)?;
        if !foreign_keys_enabled {
            return Err(DatabaseRepositoryError::SchemaInvalid);
        }
        Ok(())
    }

    pub(crate) fn validate_v4(connection: &Connection) -> Result<(), DatabaseRepositoryError> {
        if Self::schema_version(connection)? != V4_SCHEMA_VERSION {
            return Err(DatabaseRepositoryError::SchemaInvalid);
        }
        Self::validate_required_tables(connection, &REQUIRED_V4_TABLES)?;
        let required_schema_objects = [
            ("table", "asset_search"),
            ("index", "idx_ai_suggestions_review_page"),
            (
                "trigger",
                "ai_suggestions_reject_pending_after_project_delete",
            ),
            (
                "trigger",
                "ai_suggestions_reject_pending_after_asset_delete",
            ),
            ("table", "edit_history"),
            ("index", "idx_saved_filters_updated_page"),
            ("index", "idx_custom_field_values_target_page"),
            ("trigger", "custom_field_values_validate_insert"),
            ("trigger", "edit_history_validate_insert"),
        ];
        Self::validate_schema_objects(connection, &required_schema_objects)?;
        Self::validate_provider_columns(connection)?;
        let foreign_keys_enabled: bool = connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .map_err(|_| DatabaseRepositoryError::SchemaInvalid)?;
        if !foreign_keys_enabled {
            return Err(DatabaseRepositoryError::SchemaInvalid);
        }
        Ok(())
    }

    pub(crate) fn validate_v5(connection: &Connection) -> Result<(), DatabaseRepositoryError> {
        if Self::schema_version(connection)? != V5_SCHEMA_VERSION {
            return Err(DatabaseRepositoryError::SchemaInvalid);
        }
        Self::validate_required_tables(connection, &REQUIRED_V5_TABLES)?;
        let required_schema_objects = [
            ("table", "asset_search"),
            ("index", "idx_ai_suggestions_review_page"),
            (
                "trigger",
                "ai_suggestions_reject_pending_after_project_delete",
            ),
            (
                "trigger",
                "ai_suggestions_reject_pending_after_asset_delete",
            ),
            ("table", "edit_history"),
            ("index", "idx_saved_filters_updated_page"),
            ("index", "idx_custom_field_values_target_page"),
            ("trigger", "custom_field_values_validate_insert"),
            ("trigger", "edit_history_validate_insert"),
            ("index", "idx_asset_covers_updated"),
            ("index", "idx_asset_covers_source_page"),
            ("trigger", "asset_covers_validate_insert"),
            ("trigger", "asset_covers_validate_update"),
            ("trigger", "asset_covers_prevent_video_demotion"),
        ];
        Self::validate_schema_objects(connection, &required_schema_objects)?;
        Self::validate_provider_columns(connection)?;
        let foreign_keys_enabled: bool = connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .map_err(|_| DatabaseRepositoryError::SchemaInvalid)?;
        if !foreign_keys_enabled {
            return Err(DatabaseRepositoryError::SchemaInvalid);
        }
        Ok(())
    }

    pub(crate) fn validate_v6(connection: &Connection) -> Result<(), DatabaseRepositoryError> {
        if Self::schema_version(connection)? != V6_SCHEMA_VERSION {
            return Err(DatabaseRepositoryError::SchemaInvalid);
        }
        Self::validate_v6_schema(connection)
    }

    pub(crate) fn validate_v7(connection: &Connection) -> Result<(), DatabaseRepositoryError> {
        if Self::schema_version(connection)? != V7_SCHEMA_VERSION {
            return Err(DatabaseRepositoryError::SchemaInvalid);
        }
        Self::validate_v6_schema(connection)
    }

    pub(crate) fn validate_v8(connection: &Connection) -> Result<(), DatabaseRepositoryError> {
        if Self::schema_version(connection)? != V8_SCHEMA_VERSION {
            return Err(DatabaseRepositoryError::SchemaInvalid);
        }
        Self::validate_v8_schema(connection, false)
    }

    pub(crate) fn validate_v9(connection: &Connection) -> Result<(), DatabaseRepositoryError> {
        if Self::schema_version(connection)? != V9_SCHEMA_VERSION {
            return Err(DatabaseRepositoryError::SchemaInvalid);
        }
        Self::validate_v9_schema(connection)
    }

    pub(crate) fn validate_v10(connection: &Connection) -> Result<(), DatabaseRepositoryError> {
        if Self::schema_version(connection)? != V10_SCHEMA_VERSION {
            return Err(DatabaseRepositoryError::SchemaInvalid);
        }
        Self::validate_v9_schema(connection)?;
        Self::validate_schema_objects(
            connection,
            &[
                ("table", "canvas_project_members"),
                ("index", "idx_canvas_members_reference_name"),
                ("index", "idx_canvas_members_page"),
                ("index", "idx_canvas_members_role_position"),
                ("trigger", "canvas_members_validate_insert"),
                ("trigger", "canvas_members_validate_update"),
                (
                    "trigger",
                    "canvas_members_cleanup_before_asset_project_update",
                ),
            ],
        )?;
        let kind_column_exists: bool = connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM pragma_table_info('projects') WHERE name='kind'
                )",
                [],
                |row| row.get(0),
            )
            .map_err(|_| DatabaseRepositoryError::SchemaInvalid)?;
        let invalid_relation: bool = connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1
                      FROM canvas_project_members member
                      LEFT JOIN projects project ON project.id=member.project_id
                      LEFT JOIN assets asset ON asset.id=member.asset_id
                     WHERE project.id IS NULL
                        OR asset.project_id<>member.project_id
                        OR (member.role='reference' AND asset.media_type<>'image')
                        OR (member.role='output' AND member.reference_name IS NOT NULL)
                        OR (member.role='reference' AND member.reference_name IS NULL)
                )",
                [],
                |row| row.get(0),
            )
            .map_err(|_| DatabaseRepositoryError::SchemaInvalid)?;
        if !kind_column_exists || invalid_relation {
            return Err(DatabaseRepositoryError::SchemaInvalid);
        }
        let mut names = connection
            .prepare(
                "SELECT reference_name FROM canvas_project_members
                  WHERE role='reference' ORDER BY project_id,asset_id",
            )
            .map_err(|_| DatabaseRepositoryError::SchemaInvalid)?;
        let names = names
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|_| DatabaseRepositoryError::SchemaInvalid)?;
        for name in names {
            let name = name.map_err(|_| DatabaseRepositoryError::SchemaInvalid)?;
            if !valid_reference_name(&name) {
                return Err(DatabaseRepositoryError::SchemaInvalid);
            }
        }
        Ok(())
    }

    fn validate_v9_schema(connection: &Connection) -> Result<(), DatabaseRepositoryError> {
        Self::validate_v8_schema(connection, true)?;
        let invalid_image_duration: bool = connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM assets
                     WHERE media_type='image' AND duration_ms IS NOT NULL
                )",
                [],
                |row| row.get(0),
            )
            .map_err(|_| DatabaseRepositoryError::SchemaInvalid)?;
        let invalid_media_order: bool = connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1
                      FROM asset_display_order image_order
                      JOIN assets image_asset ON image_asset.id=image_order.asset_id
                      JOIN asset_display_order video_order ON video_order.position < image_order.position
                      JOIN assets video_asset ON video_asset.id=video_order.asset_id
                     WHERE image_asset.media_type='image' AND video_asset.media_type='video'
                )",
                [],
                |row| row.get(0),
            )
            .map_err(|_| DatabaseRepositoryError::SchemaInvalid)?;
        if invalid_image_duration || invalid_media_order {
            return Err(DatabaseRepositoryError::SchemaInvalid);
        }
        Ok(())
    }

    fn validate_v8_schema(
        connection: &Connection,
        require_media_repair_triggers: bool,
    ) -> Result<(), DatabaseRepositoryError> {
        Self::validate_required_tables(connection, &REQUIRED_V5_TABLES)?;
        let mut required_schema_objects = vec![
            ("table", "asset_search"),
            ("index", "idx_ai_suggestions_review_page"),
            (
                "trigger",
                "ai_suggestions_reject_pending_after_project_delete",
            ),
            (
                "trigger",
                "ai_suggestions_reject_pending_after_asset_delete",
            ),
            ("table", "asset_display_order"),
            ("index", "idx_asset_display_order_position"),
            ("trigger", "asset_display_order_after_insert_image"),
            ("trigger", "asset_display_order_after_insert_video"),
            ("trigger", "asset_display_order_before_delete"),
            ("table", "edit_history"),
            ("index", "idx_saved_filters_updated_page"),
            ("index", "idx_custom_field_values_target_page"),
            ("trigger", "custom_field_values_validate_insert"),
            ("trigger", "edit_history_validate_insert"),
            ("table", "asset_covers"),
            ("index", "idx_asset_covers_updated"),
            ("index", "idx_asset_covers_source_page"),
            ("trigger", "asset_covers_validate_insert"),
            ("trigger", "asset_covers_validate_update"),
            ("trigger", "asset_covers_prevent_video_demotion"),
        ];
        if require_media_repair_triggers {
            required_schema_objects.extend([
                ("trigger", "asset_image_duration_after_insert"),
                ("trigger", "asset_image_duration_after_update"),
            ]);
        }
        Self::validate_schema_objects(connection, &required_schema_objects)?;
        let invalid_positions: bool = connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM assets a
                    LEFT JOIN asset_display_order ao ON ao.asset_id = a.id
                    WHERE ao.asset_id IS NULL OR ao.position <= 0
                )",
                [],
                |row| row.get(0),
            )
            .map_err(|_| DatabaseRepositoryError::SchemaInvalid)?;
        if invalid_positions {
            return Err(DatabaseRepositoryError::SchemaInvalid);
        }
        Self::validate_provider_columns(connection)?;
        let foreign_keys_enabled: bool = connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .map_err(|_| DatabaseRepositoryError::SchemaInvalid)?;
        if !foreign_keys_enabled {
            return Err(DatabaseRepositoryError::SchemaInvalid);
        }
        Ok(())
    }

    fn validate_v6_schema(connection: &Connection) -> Result<(), DatabaseRepositoryError> {
        Self::validate_required_tables(connection, &REQUIRED_V5_TABLES)?;
        let required_schema_objects = [
            ("table", "asset_search"),
            ("index", "idx_ai_suggestions_review_page"),
            (
                "trigger",
                "ai_suggestions_reject_pending_after_project_delete",
            ),
            (
                "trigger",
                "ai_suggestions_reject_pending_after_asset_delete",
            ),
            ("table", "edit_history"),
            ("index", "idx_saved_filters_updated_page"),
            ("index", "idx_custom_field_values_target_page"),
            ("trigger", "custom_field_values_validate_insert"),
            ("trigger", "edit_history_validate_insert"),
            ("index", "idx_asset_covers_updated"),
            ("index", "idx_asset_covers_source_page"),
            ("trigger", "asset_covers_validate_insert"),
            ("trigger", "asset_covers_validate_update"),
            ("trigger", "asset_covers_prevent_video_demotion"),
            ("table", "asset_display_order"),
            ("index", "idx_asset_display_order_position"),
            ("trigger", "asset_display_order_after_insert"),
            ("trigger", "asset_display_order_before_delete"),
        ];
        Self::validate_schema_objects(connection, &required_schema_objects)?;
        let invalid_positions: bool = connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM assets a
                    LEFT JOIN asset_display_order ao ON ao.asset_id = a.id
                    WHERE ao.asset_id IS NULL OR ao.position <= 0
                )",
                [],
                |row| row.get(0),
            )
            .map_err(|_| DatabaseRepositoryError::SchemaInvalid)?;
        if invalid_positions {
            return Err(DatabaseRepositoryError::SchemaInvalid);
        }
        Self::validate_provider_columns(connection)?;
        let foreign_keys_enabled: bool = connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .map_err(|_| DatabaseRepositoryError::SchemaInvalid)?;
        if !foreign_keys_enabled {
            return Err(DatabaseRepositoryError::SchemaInvalid);
        }
        Ok(())
    }

    fn validate_schema_objects(
        connection: &Connection,
        required_schema_objects: &[(&str, &str)],
    ) -> Result<(), DatabaseRepositoryError> {
        for (kind, name) in required_schema_objects {
            let exists: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type=?1 AND name=?2)",
                    [kind, name],
                    |row| row.get(0),
                )
                .map_err(|_| DatabaseRepositoryError::SchemaInvalid)?;
            if !exists {
                return Err(DatabaseRepositoryError::SchemaInvalid);
            }
        }
        Ok(())
    }

    fn validate_provider_columns(connection: &Connection) -> Result<(), DatabaseRepositoryError> {
        let provider_columns: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('ai_providers') WHERE name='credential_id')
                 AND EXISTS(SELECT 1 FROM pragma_table_info('ai_suggestions') WHERE name='provider_id')",
                [],
                |row| row.get(0),
            )
            .map_err(|_| DatabaseRepositoryError::SchemaInvalid)?;
        if !provider_columns {
            return Err(DatabaseRepositoryError::SchemaInvalid);
        }
        Ok(())
    }

    fn validate_required_tables(
        connection: &Connection,
        tables: &[&str],
    ) -> Result<(), DatabaseRepositoryError> {
        let mut statement = connection
            .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")
            .map_err(|_| DatabaseRepositoryError::SchemaInvalid)?;
        for table_name in tables {
            let exists = statement
                .exists([table_name])
                .map_err(|_| DatabaseRepositoryError::SchemaInvalid)?;
            if !exists {
                return Err(DatabaseRepositoryError::SchemaInvalid);
            }
        }
        Ok(())
    }

    pub(crate) fn open_backup_destination(
        path: &Path,
    ) -> Result<Connection, DatabaseRepositoryError> {
        Connection::open(path).map_err(|_| DatabaseRepositoryError::BackupFailed)
    }

    pub(crate) fn backup(
        source: &Connection,
        destination: &mut Connection,
    ) -> Result<(), DatabaseRepositoryError> {
        let backup =
            Backup::new(source, destination).map_err(|_| DatabaseRepositoryError::BackupFailed)?;

        // 分页复制并限制 BUSY/LOCKED 重试，避免大库或异常锁导致命令无限等待。
        let mut transient_retries = 0_u16;
        loop {
            match backup
                .step(128)
                .map_err(|_| DatabaseRepositoryError::BackupFailed)?
            {
                StepResult::Done => break,
                StepResult::More => transient_retries = 0,
                StepResult::Busy | StepResult::Locked => {
                    transient_retries += 1;
                    if transient_retries > BACKUP_MAX_TRANSIENT_RETRIES {
                        return Err(DatabaseRepositoryError::BackupFailed);
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                _ => return Err(DatabaseRepositoryError::BackupFailed),
            }
        }
        drop(backup);
        destination
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(|_| DatabaseRepositoryError::BackupFailed)?;
        Ok(())
    }
}

fn valid_reference_name(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.chars().count() <= 50
        && value.chars().all(|character| {
            !character.is_whitespace()
                && character != '@'
                && !matches!(
                    character,
                    ',' | '.'
                        | ';'
                        | ':'
                        | '!'
                        | '?'
                        | '，'
                        | '。'
                        | '；'
                        | '：'
                        | '！'
                        | '？'
                        | '、'
                        | '/'
                        | '\\'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '<'
                        | '>'
                        | '《'
                        | '》'
                        | '"'
                        | '\''
                        | '`'
                        | '~'
                        | '#'
                        | '$'
                        | '%'
                        | '^'
                        | '&'
                        | '*'
                        | '+'
                        | '='
                        | '|'
                        | '-'
                        | '_'
                )
        })
}
