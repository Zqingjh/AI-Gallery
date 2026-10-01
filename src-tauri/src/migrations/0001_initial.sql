CREATE TABLE projects (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    description TEXT NOT NULL DEFAULT '',
    rating INTEGER NOT NULL DEFAULT 0 CHECK (rating BETWEEN 0 AND 5),
    is_favorite INTEGER NOT NULL DEFAULT 0 CHECK (is_favorite IN (0, 1)),
    is_public INTEGER NOT NULL DEFAULT 0 CHECK (is_public IN (0, 1)),
    notes TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL CHECK (updated_at >= created_at)
);

CREATE TABLE models (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL COLLATE NOCASE UNIQUE CHECK (length(trim(name)) > 0),
    provider TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL CHECK (updated_at >= created_at)
);

CREATE TABLE platforms (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL COLLATE NOCASE UNIQUE CHECK (length(trim(name)) > 0),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL CHECK (updated_at >= created_at)
);

CREATE TABLE prompts (
    id INTEGER PRIMARY KEY,
    project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL,
    title TEXT NOT NULL DEFAULT '',
    prompt_zh TEXT NOT NULL DEFAULT '',
    prompt_en TEXT NOT NULL DEFAULT '',
    negative_prompt TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL CHECK (updated_at >= created_at)
);

CREATE TABLE assets (
    id INTEGER PRIMARY KEY,
    project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL,
    prompt_id INTEGER REFERENCES prompts(id) ON DELETE SET NULL,
    model_id INTEGER REFERENCES models(id) ON DELETE SET NULL,
    platform_id INTEGER REFERENCES platforms(id) ON DELETE SET NULL,
    media_type TEXT NOT NULL CHECK (media_type IN ('image', 'video')),
    path_kind TEXT NOT NULL CHECK (path_kind IN ('managed', 'external')),
    stored_path TEXT NOT NULL CHECK (length(stored_path) > 0),
    file_name TEXT NOT NULL CHECK (length(file_name) > 0),
    mime_type TEXT,
    file_size INTEGER CHECK (file_size IS NULL OR file_size >= 0),
    content_hash TEXT,
    width INTEGER CHECK (width IS NULL OR width > 0),
    height INTEGER CHECK (height IS NULL OR height > 0),
    duration_ms INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
    frame_rate REAL CHECK (frame_rate IS NULL OR frame_rate > 0),
    has_audio INTEGER CHECK (has_audio IS NULL OR has_audio IN (0, 1)),
    generation_params_json TEXT NOT NULL DEFAULT '{}',
    rating INTEGER NOT NULL DEFAULT 0 CHECK (rating BETWEEN 0 AND 5),
    is_favorite INTEGER NOT NULL DEFAULT 0 CHECK (is_favorite IN (0, 1)),
    is_public INTEGER NOT NULL DEFAULT 0 CHECK (is_public IN (0, 1)),
    notes TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL CHECK (updated_at >= created_at),
    UNIQUE(path_kind, stored_path)
);

CREATE TABLE dimensions (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL COLLATE NOCASE UNIQUE CHECK (length(trim(name)) > 0),
    allows_multiple INTEGER NOT NULL DEFAULT 1 CHECK (allows_multiple IN (0, 1)),
    ai_can_suggest_new INTEGER NOT NULL DEFAULT 0 CHECK (ai_can_suggest_new IN (0, 1)),
    is_enabled INTEGER NOT NULL DEFAULT 1 CHECK (is_enabled IN (0, 1)),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL CHECK (updated_at >= created_at)
);

CREATE TABLE categories (
    id INTEGER PRIMARY KEY,
    dimension_id INTEGER NOT NULL REFERENCES dimensions(id) ON DELETE RESTRICT,
    name TEXT NOT NULL COLLATE NOCASE CHECK (length(trim(name)) > 0),
    aliases_json TEXT NOT NULL DEFAULT '[]',
    description TEXT NOT NULL DEFAULT '',
    color TEXT,
    icon TEXT,
    is_enabled INTEGER NOT NULL DEFAULT 1 CHECK (is_enabled IN (0, 1)),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL CHECK (updated_at >= created_at),
    UNIQUE(dimension_id, name)
);

CREATE TABLE project_categories (
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    category_id INTEGER NOT NULL REFERENCES categories(id) ON DELETE RESTRICT,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (project_id, category_id)
) WITHOUT ROWID;

CREATE TABLE asset_categories (
    asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    category_id INTEGER NOT NULL REFERENCES categories(id) ON DELETE RESTRICT,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (asset_id, category_id)
) WITHOUT ROWID;

CREATE TABLE tags (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL COLLATE NOCASE UNIQUE CHECK (length(trim(name)) > 0),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL CHECK (updated_at >= created_at)
);

CREATE TABLE project_tags (
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE RESTRICT,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (project_id, tag_id)
) WITHOUT ROWID;

CREATE TABLE asset_tags (
    asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE RESTRICT,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (asset_id, tag_id)
) WITHOUT ROWID;

CREATE TABLE ai_suggestions (
    id INTEGER PRIMARY KEY,
    target_type TEXT NOT NULL CHECK (target_type IN ('project', 'asset')),
    target_id INTEGER NOT NULL,
    dimension_id INTEGER NOT NULL REFERENCES dimensions(id) ON DELETE RESTRICT,
    category_id INTEGER REFERENCES categories(id) ON DELETE RESTRICT,
    suggested_category_name TEXT,
    confidence REAL CHECK (confidence IS NULL OR confidence BETWEEN 0.0 AND 1.0),
    reason TEXT NOT NULL DEFAULT '',
    source_model TEXT NOT NULL DEFAULT '',
    input_scope_json TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'accepted', 'rejected', 'modified')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL CHECK (updated_at >= created_at),
    CHECK (
        category_id IS NOT NULL
        OR coalesce(length(trim(suggested_category_name)), 0) > 0
    )
);

CREATE TABLE trash_entries (
    id INTEGER PRIMARY KEY,
    entity_type TEXT NOT NULL CHECK (entity_type IN ('project', 'asset')),
    entity_id INTEGER NOT NULL,
    snapshot_json TEXT NOT NULL,
    media_action TEXT NOT NULL DEFAULT 'keep'
        CHECK (media_action IN ('keep', 'trash', 'delete')),
    deleted_at INTEGER NOT NULL,
    UNIQUE(entity_type, entity_id)
);

CREATE TABLE app_settings (
    key TEXT PRIMARY KEY CHECK (length(trim(key)) > 0),
    value_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
) WITHOUT ROWID;

CREATE INDEX idx_projects_updated_page ON projects(updated_at DESC, id DESC);
CREATE INDEX idx_assets_updated_page ON assets(updated_at DESC, id DESC);
CREATE INDEX idx_assets_project_page ON assets(project_id, updated_at DESC, id DESC);
CREATE INDEX idx_assets_media_type_page ON assets(media_type, updated_at DESC, id DESC);
CREATE INDEX idx_assets_hash ON assets(content_hash) WHERE content_hash IS NOT NULL;
CREATE INDEX idx_prompts_project ON prompts(project_id, id);
CREATE INDEX idx_categories_dimension ON categories(dimension_id, is_enabled, id);
CREATE INDEX idx_project_categories_category ON project_categories(category_id, project_id);
CREATE INDEX idx_asset_categories_category ON asset_categories(category_id, asset_id);
CREATE INDEX idx_project_tags_tag ON project_tags(tag_id, project_id);
CREATE INDEX idx_asset_tags_tag ON asset_tags(tag_id, asset_id);
CREATE INDEX idx_ai_suggestions_status ON ai_suggestions(status, created_at, id);
CREATE INDEX idx_ai_suggestions_target ON ai_suggestions(target_type, target_id, status);
CREATE INDEX idx_trash_entries_deleted ON trash_entries(deleted_at DESC, id DESC);
