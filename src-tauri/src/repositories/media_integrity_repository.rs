use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};

use crate::domain::{
    AssetCover, AssetCoverRecord, AssetPathRecord, AssetPathRepairUpdate, CoverSourceType,
    MediaType, PathKind, SaveAssetCover,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MediaIntegrityRepositoryError {
    NotFound,
    Conflict,
    InvalidData,
    DatabaseFailed,
}

pub(crate) struct MediaIntegrityRepository;

impl MediaIntegrityRepository {
    pub(crate) fn get_asset_cover(
        connection: &Connection,
        asset_id: i64,
    ) -> Result<Option<AssetCoverRecord>, MediaIntegrityRepositoryError> {
        connection
            .query_row(
                "SELECT asset_id,source_type,stored_path,frame_timestamp_ms,created_at,updated_at
                   FROM asset_covers WHERE asset_id=?1",
                [asset_id],
                asset_cover_from_row,
            )
            .optional()
            .map_err(map_db_error)
    }

    /// 在一个数据库事务内替换封面元数据，并返回替换前的记录供文件层做安全清理。
    pub(crate) fn save_asset_cover(
        connection: &mut Connection,
        input: &SaveAssetCover,
        now: i64,
    ) -> Result<Option<AssetCoverRecord>, MediaIntegrityRepositoryError> {
        let transaction = connection.transaction().map_err(map_db_error)?;
        let previous = Self::get_asset_cover(&transaction, input.asset_id)?;
        Self::save_asset_cover_in_transaction(&transaction, input, now)?;
        transaction.commit().map_err(map_db_error)?;
        Ok(previous)
    }

    /// 供上层需要组合更多数据库写入时复用，调用方负责提交或回滚事务。
    pub(crate) fn save_asset_cover_in_transaction(
        transaction: &Transaction<'_>,
        input: &SaveAssetCover,
        now: i64,
    ) -> Result<(), MediaIntegrityRepositoryError> {
        transaction
            .execute(
                "INSERT INTO asset_covers
                    (asset_id,source_type,stored_path,frame_timestamp_ms,created_at,updated_at)
                 VALUES(?1,?2,?3,?4,?5,?5)
                 ON CONFLICT(asset_id) DO UPDATE SET
                    source_type=excluded.source_type,
                    stored_path=excluded.stored_path,
                    frame_timestamp_ms=excluded.frame_timestamp_ms,
                    updated_at=max(asset_covers.updated_at,excluded.updated_at)",
                params![
                    input.asset_id,
                    input.source_type.as_db_str(),
                    input.stored_path,
                    input.frame_timestamp_ms,
                    now,
                ],
            )
            .map_err(map_db_error)?;
        Ok(())
    }

    /// 删除封面数据库记录，不触碰源视频或封面文件。
    pub(crate) fn remove_asset_cover(
        connection: &mut Connection,
        asset_id: i64,
    ) -> Result<Option<AssetCoverRecord>, MediaIntegrityRepositoryError> {
        let transaction = connection.transaction().map_err(map_db_error)?;
        let previous = Self::get_asset_cover(&transaction, asset_id)?;
        if previous.is_some() {
            Self::remove_asset_cover_in_transaction(&transaction, asset_id)?;
        }
        transaction.commit().map_err(map_db_error)?;
        Ok(previous)
    }

    pub(crate) fn remove_asset_cover_in_transaction(
        transaction: &Transaction<'_>,
        asset_id: i64,
    ) -> Result<(), MediaIntegrityRepositoryError> {
        transaction
            .execute("DELETE FROM asset_covers WHERE asset_id=?1", [asset_id])
            .map_err(map_db_error)?;
        Ok(())
    }

    pub(crate) fn get_asset_for_path_repair(
        connection: &Connection,
        asset_id: i64,
    ) -> Result<AssetPathRecord, MediaIntegrityRepositoryError> {
        connection
            .query_row(
                "SELECT id,media_type,path_kind,stored_path,file_name,file_size,content_hash,updated_at
                   FROM assets WHERE id=?1",
                [asset_id],
                asset_path_from_row,
            )
            .optional()
            .map_err(map_db_error)?
            .ok_or(MediaIntegrityRepositoryError::NotFound)
    }

    /// 使用资产更新时间与路径模式作为乐观锁，原子更新路径、文件摘要与脱敏编辑历史。
    pub(crate) fn repair_asset_path(
        connection: &mut Connection,
        input: &AssetPathRepairUpdate,
        now: i64,
    ) -> Result<AssetPathRecord, MediaIntegrityRepositoryError> {
        let transaction = connection.transaction().map_err(map_db_error)?;
        let previous = Self::get_asset_for_path_repair(&transaction, input.asset_id)?;
        if previous.updated_at != input.expected_updated_at
            || previous.path_kind != input.expected_path_kind
        {
            return Err(MediaIntegrityRepositoryError::Conflict);
        }

        let mut changed_fields = Vec::new();
        if previous.stored_path != input.stored_path {
            changed_fields.push("stored_path");
        }
        if previous.file_name != input.file_name {
            changed_fields.push("file_name");
        }
        if previous.file_size != input.file_size {
            changed_fields.push("file_size");
        }
        if previous.content_hash.as_deref() != Some(input.content_hash.as_str()) {
            changed_fields.push("content_hash");
        }
        if changed_fields.is_empty() {
            return Err(MediaIntegrityRepositoryError::InvalidData);
        }

        let updated = transaction
            .execute(
                "UPDATE assets SET stored_path=?1,file_name=?2,file_size=?3,content_hash=?4,
                                  updated_at=max(updated_at+1,?5)
                   WHERE id=?6 AND updated_at=?7 AND path_kind=?8",
                params![
                    input.stored_path,
                    input.file_name,
                    input.file_size,
                    input.content_hash,
                    now,
                    input.asset_id,
                    input.expected_updated_at,
                    input.expected_path_kind.as_db_str(),
                ],
            )
            .map_err(map_db_error)?;
        if updated != 1 {
            return Err(MediaIntegrityRepositoryError::Conflict);
        }

        let changed_fields_json = serde_json::to_string(&changed_fields)
            .map_err(|_| MediaIntegrityRepositoryError::InvalidData)?;
        transaction
            .execute(
                "INSERT INTO edit_history
                    (target_type,target_id,action,changed_fields_json,source,status,created_at,updated_at)
                 VALUES('asset',?1,'update',?2,'manual','confirmed',?3,?3)",
                params![input.asset_id, changed_fields_json, now],
            )
            .map_err(map_db_error)?;

        let repaired = Self::get_asset_for_path_repair(&transaction, input.asset_id)?;
        transaction.commit().map_err(map_db_error)?;
        Ok(repaired)
    }
}

fn asset_cover_from_row(row: &Row<'_>) -> rusqlite::Result<AssetCoverRecord> {
    let source_type = match row.get::<_, String>(1)?.as_str() {
        "key_frame" => CoverSourceType::KeyFrame,
        "custom" => CoverSourceType::Custom,
        value => {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("未知封面来源：{value}"),
                )),
            ));
        }
    };
    Ok(AssetCoverRecord {
        cover: AssetCover {
            asset_id: row.get(0)?,
            source_type,
            frame_timestamp_ms: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        },
        stored_path: row.get(2)?,
    })
}

fn asset_path_from_row(row: &Row<'_>) -> rusqlite::Result<AssetPathRecord> {
    let media_type = match row.get::<_, String>(1)?.as_str() {
        "image" => MediaType::Image,
        "video" => MediaType::Video,
        value => return Err(invalid_text(1, "媒体类型", value)),
    };
    let path_kind = match row.get::<_, String>(2)?.as_str() {
        "managed" => PathKind::Managed,
        "external" => PathKind::External,
        value => return Err(invalid_text(2, "路径模式", value)),
    };
    Ok(AssetPathRecord {
        asset_id: row.get(0)?,
        media_type,
        path_kind,
        stored_path: row.get(3)?,
        file_name: row.get(4)?,
        file_size: row.get(5)?,
        content_hash: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn invalid_text(index: usize, field: &str, value: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("未知{field}：{value}"),
        )),
    )
}

fn map_db_error(error: rusqlite::Error) -> MediaIntegrityRepositoryError {
    match error {
        rusqlite::Error::SqliteFailure(code, _)
            if code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
                || code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY
                || code.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            MediaIntegrityRepositoryError::Conflict
        }
        rusqlite::Error::FromSqlConversionFailure(..)
        | rusqlite::Error::IntegralValueOutOfRange(..) => {
            MediaIntegrityRepositoryError::InvalidData
        }
        _ => MediaIntegrityRepositoryError::DatabaseFailed,
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::{Connection, params};

    use super::{MediaIntegrityRepository, MediaIntegrityRepositoryError};
    use crate::domain::{AssetPathRepairUpdate, CoverSourceType, PathKind, SaveAssetCover};

    fn database() -> Connection {
        let connection = Connection::open_in_memory().expect("应能打开内存数据库");
        connection
            .pragma_update(None, "foreign_keys", true)
            .expect("应能启用外键");
        for sql in [
            include_str!("../migrations/0001_initial.sql"),
            include_str!("../migrations/0002_search.sql"),
            include_str!("../migrations/0003_ai.sql"),
            include_str!("../migrations/0004_p1_library.sql"),
            include_str!("../migrations/0005_media_integrity.sql"),
        ] {
            connection.execute_batch(sql).expect("应能顺序执行迁移");
        }
        connection
    }

    fn asset(connection: &Connection, media_type: &str, suffix: &str) -> i64 {
        connection
            .execute(
                "INSERT INTO assets
                    (media_type,path_kind,stored_path,file_name,file_size,created_at,updated_at)
                 VALUES(?1,'managed',?2,?3,100,1,1)",
                params![
                    media_type,
                    format!("media/{suffix}"),
                    format!("{suffix}.bin")
                ],
            )
            .expect("应能创建测试资产");
        connection.last_insert_rowid()
    }

    #[test]
    fn migration_rejects_image_and_invalid_cover_metadata() {
        let connection = database();
        let image_id = asset(&connection, "image", "image-source");
        let video_id = asset(&connection, "video", "video-source");

        let image_result = connection.execute(
            "INSERT INTO asset_covers
                (asset_id,source_type,stored_path,frame_timestamp_ms,created_at,updated_at)
             VALUES(?1,'custom','media/covers/image.png',NULL,1,1)",
            [image_id],
        );
        assert!(image_result.is_err(), "图片资产不得拥有视频封面");

        let missing_timestamp = connection.execute(
            "INSERT INTO asset_covers
                (asset_id,source_type,stored_path,frame_timestamp_ms,created_at,updated_at)
             VALUES(?1,'key_frame','media/covers/frame.png',NULL,1,1)",
            [video_id],
        );
        assert!(missing_timestamp.is_err(), "关键帧封面必须记录时间戳");

        let escaping_path = connection.execute(
            "INSERT INTO asset_covers
                (asset_id,source_type,stored_path,frame_timestamp_ms,created_at,updated_at)
             VALUES(?1,'custom','media/covers/../outside.png',NULL,1,1)",
            [video_id],
        );
        assert!(escaping_path.is_err(), "封面路径不得逃逸工作区目录");
    }

    #[test]
    fn cover_crud_returns_previous_path_without_exposing_it_in_dto() {
        let mut connection = database();
        let video_id = asset(&connection, "video", "video-cover");
        let first = SaveAssetCover {
            asset_id: video_id,
            source_type: CoverSourceType::KeyFrame,
            stored_path: "media/covers/first.png".to_owned(),
            frame_timestamp_ms: Some(2_000),
        };
        assert_eq!(
            MediaIntegrityRepository::save_asset_cover(&mut connection, &first, 10)
                .expect("应能保存首个封面"),
            None
        );

        let second = SaveAssetCover {
            asset_id: video_id,
            source_type: CoverSourceType::Custom,
            stored_path: "media/covers/second.png".to_owned(),
            frame_timestamp_ms: None,
        };
        let previous = MediaIntegrityRepository::save_asset_cover(&mut connection, &second, 11)
            .expect("应能替换封面")
            .expect("应返回旧封面");
        assert_eq!(previous.stored_path, "media/covers/first.png");

        let current = MediaIntegrityRepository::get_asset_cover(&connection, video_id)
            .expect("应能读取封面")
            .expect("封面应存在");
        let dto = serde_json::to_value(&current.cover).expect("公开 DTO 应可序列化");
        assert!(dto.get("storedPath").is_none(), "公开 DTO 不得包含内部路径");
        assert_eq!(current.stored_path, "media/covers/second.png");

        let removed = MediaIntegrityRepository::remove_asset_cover(&mut connection, video_id)
            .expect("应能移除封面")
            .expect("应返回被移除的封面");
        assert_eq!(removed.stored_path, "media/covers/second.png");
        assert!(
            MediaIntegrityRepository::get_asset_cover(&connection, video_id)
                .expect("应能检查封面")
                .is_none()
        );
    }

    #[test]
    fn path_repair_is_atomic_and_uses_optimistic_lock() {
        let mut connection = database();
        let asset_id = asset(&connection, "video", "missing-video");
        let update = AssetPathRepairUpdate {
            asset_id,
            expected_updated_at: 1,
            expected_path_kind: PathKind::Managed,
            stored_path: "media/repaired-video.mp4".to_owned(),
            file_name: "repaired-video.mp4".to_owned(),
            file_size: Some(321),
            content_hash: "a".repeat(64),
        };
        let repaired = MediaIntegrityRepository::repair_asset_path(&mut connection, &update, 20)
            .expect("应能原子修复路径");
        assert_eq!(repaired.stored_path, "media/repaired-video.mp4");
        assert_eq!(repaired.updated_at, 20);
        let changed_fields: String = connection
            .query_row(
                "SELECT changed_fields_json FROM edit_history
                 WHERE target_type='asset' AND target_id=?1 ORDER BY id DESC LIMIT 1",
                [asset_id],
                |row| row.get(0),
            )
            .expect("应写入不含路径值的编辑历史");
        assert!(!changed_fields.contains("repaired-video"));

        let stale = MediaIntegrityRepository::repair_asset_path(&mut connection, &update, 21);
        assert_eq!(stale, Err(MediaIntegrityRepositoryError::Conflict));

        let stored_path: String = connection
            .query_row(
                "SELECT stored_path FROM assets WHERE id=?1",
                [asset_id],
                |row| row.get(0),
            )
            .expect("资产应仍存在");
        assert_eq!(stored_path, "media/repaired-video.mp4");
    }
}
