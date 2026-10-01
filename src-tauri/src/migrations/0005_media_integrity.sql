-- M7：视频封面与媒体完整性修复。
-- 封面始终由工作区管理，只保存可迁移相对路径；源视频不会因封面变更而被删除。
CREATE TABLE asset_covers (
    asset_id INTEGER PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
    source_type TEXT NOT NULL CHECK (source_type IN ('key_frame', 'custom')),
    stored_path TEXT NOT NULL UNIQUE CHECK (
        substr(stored_path, 1, length('media/covers/')) = 'media/covers/'
        AND length(stored_path) > length('media/covers/.png')
        AND lower(substr(stored_path, -4)) = '.png'
        AND instr(substr(stored_path, length('media/covers/') + 1), '/') = 0
        AND instr(substr(stored_path, length('media/covers/') + 1), char(92)) = 0
        AND instr(stored_path, '..') = 0
    ),
    frame_timestamp_ms INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL CHECK (updated_at >= created_at),
    CHECK (
        (source_type = 'key_frame' AND frame_timestamp_ms IS NOT NULL AND frame_timestamp_ms >= 0)
        OR (source_type = 'custom' AND frame_timestamp_ms IS NULL)
    )
);

CREATE INDEX idx_asset_covers_updated
    ON asset_covers(updated_at DESC, asset_id DESC);
CREATE INDEX idx_asset_covers_source_page
    ON asset_covers(source_type, updated_at DESC, asset_id DESC);

-- SQLite 的 CHECK 不能跨表，使用触发器保证只有视频资产可以拥有封面。
CREATE TRIGGER asset_covers_validate_insert
BEFORE INSERT ON asset_covers BEGIN
    SELECT CASE WHEN NOT EXISTS(
        SELECT 1 FROM assets WHERE id = NEW.asset_id AND media_type = 'video'
    ) THEN RAISE(ABORT, 'asset cover requires video asset') END;
END;

CREATE TRIGGER asset_covers_validate_update
BEFORE UPDATE OF asset_id ON asset_covers BEGIN
    SELECT CASE WHEN NOT EXISTS(
        SELECT 1 FROM assets WHERE id = NEW.asset_id AND media_type = 'video'
    ) THEN RAISE(ABORT, 'asset cover requires video asset') END;
END;

CREATE TRIGGER asset_covers_prevent_video_demotion
BEFORE UPDATE OF media_type ON assets
WHEN NEW.media_type <> 'video' AND EXISTS(
    SELECT 1 FROM asset_covers WHERE asset_id = OLD.id
) BEGIN
    SELECT RAISE(ABORT, 'asset with cover must remain video');
END;
