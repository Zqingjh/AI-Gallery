-- M10：在不改变旧项目行为的前提下，增加画布项目及其成员关系。
ALTER TABLE projects
ADD COLUMN kind TEXT NOT NULL DEFAULT 'simple'
    CHECK (kind IN ('simple', 'canvas'));

CREATE TABLE canvas_project_members (
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('output', 'reference')),
    reference_name TEXT COLLATE NOCASE,
    position INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL CHECK (updated_at >= created_at),
    PRIMARY KEY (project_id, asset_id),
    UNIQUE (asset_id),
    CHECK (
        (role = 'output' AND reference_name IS NULL)
        OR
        (role = 'reference'
         AND reference_name IS NOT NULL
         AND reference_name = trim(reference_name)
         AND length(reference_name) BETWEEN 1 AND 50)
    )
) WITHOUT ROWID;

CREATE UNIQUE INDEX idx_canvas_members_reference_name
    ON canvas_project_members(project_id, reference_name COLLATE NOCASE)
    WHERE role = 'reference';

CREATE INDEX idx_canvas_members_page
    ON canvas_project_members(project_id, updated_at DESC, asset_id DESC);

CREATE INDEX idx_canvas_members_role_position
    ON canvas_project_members(project_id, role, position, asset_id);

-- 作品的真实归属项目必须与画布关系一致。切回 simple 时允许保留隐藏关系。
CREATE TRIGGER canvas_members_validate_insert
BEFORE INSERT ON canvas_project_members
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1 FROM assets
         WHERE id = NEW.asset_id AND project_id = NEW.project_id
    ) THEN RAISE(ABORT, 'canvas member asset project mismatch') END;
    SELECT CASE WHEN NEW.role = 'reference' AND NOT EXISTS (
        SELECT 1 FROM assets
         WHERE id = NEW.asset_id AND media_type = 'image'
    ) THEN RAISE(ABORT, 'canvas reference must be image') END;
END;

CREATE TRIGGER canvas_members_validate_update
BEFORE UPDATE ON canvas_project_members
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1 FROM assets
         WHERE id = NEW.asset_id AND project_id = NEW.project_id
    ) THEN RAISE(ABORT, 'canvas member asset project mismatch') END;
    SELECT CASE WHEN NEW.role = 'reference' AND NOT EXISTS (
        SELECT 1 FROM assets
         WHERE id = NEW.asset_id AND media_type = 'image'
    ) THEN RAISE(ABORT, 'canvas reference must be image') END;
END;

-- 防止跳过 repository 直接修改作品归属后留下失配关系。
CREATE TRIGGER canvas_members_cleanup_before_asset_project_update
BEFORE UPDATE OF project_id ON assets
WHEN OLD.project_id IS NOT NEW.project_id
BEGIN
    DELETE FROM canvas_project_members WHERE asset_id = OLD.id;
END;
