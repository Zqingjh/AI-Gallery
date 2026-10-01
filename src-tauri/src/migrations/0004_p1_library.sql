-- M6：P1 作品库效率数据。所有自动化结果均以 pending 保存，等待用户明确确认。

CREATE TABLE saved_filters (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL COLLATE NOCASE UNIQUE CHECK (length(trim(name)) > 0),
    filter_json TEXT NOT NULL CHECK (json_valid(filter_json) AND json_type(filter_json) = 'object'),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL CHECK (updated_at >= created_at)
);

CREATE TABLE custom_fields (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL COLLATE NOCASE CHECK (length(trim(name)) > 0),
    target_type TEXT NOT NULL CHECK (target_type IN ('project', 'asset')),
    value_type TEXT NOT NULL CHECK (value_type IN ('text', 'number', 'boolean', 'date', 'json')),
    options_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(options_json)),
    is_enabled INTEGER NOT NULL DEFAULT 1 CHECK (is_enabled IN (0, 1)),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL CHECK (updated_at >= created_at),
    UNIQUE(target_type, name)
);

CREATE TABLE custom_field_values (
    id INTEGER PRIMARY KEY,
    field_id INTEGER NOT NULL REFERENCES custom_fields(id) ON DELETE CASCADE,
    target_type TEXT NOT NULL CHECK (target_type IN ('project', 'asset')),
    target_id INTEGER NOT NULL,
    value_json TEXT NOT NULL CHECK (json_valid(value_json)),
    source TEXT NOT NULL CHECK (source IN ('manual', 'automation')),
    status TEXT NOT NULL CHECK (status IN ('confirmed', 'pending')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL CHECK (updated_at >= created_at),
    -- 自动化不得覆盖正式值；确认时由仓储在同一事务中替换旧正式值。
    CHECK ((source = 'manual' AND status = 'confirmed') OR (source = 'automation' AND status = 'pending')),
    UNIQUE(field_id, target_type, target_id, status)
);

CREATE TABLE prompt_versions (
    id INTEGER PRIMARY KEY,
    prompt_id INTEGER NOT NULL REFERENCES prompts(id) ON DELETE CASCADE,
    version INTEGER NOT NULL CHECK (version > 0),
    prompt_zh TEXT NOT NULL DEFAULT '',
    prompt_en TEXT NOT NULL DEFAULT '',
    negative_prompt TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    UNIQUE(prompt_id, version)
);

CREATE TABLE edit_history (
    id INTEGER PRIMARY KEY,
    target_type TEXT NOT NULL CHECK (target_type IN ('project', 'asset', 'prompt', 'custom_field_value')),
    target_id INTEGER NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create', 'update', 'delete', 'restore', 'bulk_update')),
    -- 仅保存发生变化的字段名，提示词正文由 prompt_versions 管理，避免历史记录泄露完整提示词。
    changed_fields_json TEXT NOT NULL CHECK (json_valid(changed_fields_json) AND json_type(changed_fields_json) = 'array'),
    source TEXT NOT NULL CHECK (source IN ('manual', 'automation')),
    status TEXT NOT NULL CHECK (status IN ('confirmed', 'pending')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL CHECK (updated_at >= created_at),
    CHECK ((source = 'manual' AND status = 'confirmed') OR (source = 'automation' AND status = 'pending'))
);

CREATE INDEX idx_saved_filters_updated_page ON saved_filters(updated_at DESC, id DESC);
CREATE INDEX idx_custom_fields_target_page ON custom_fields(target_type, is_enabled, updated_at DESC, id DESC);
CREATE INDEX idx_custom_field_values_target_page ON custom_field_values(target_type, target_id, status, updated_at DESC, id DESC);
CREATE INDEX idx_prompt_versions_prompt_page ON prompt_versions(prompt_id, version DESC, id DESC);
CREATE INDEX idx_edit_history_target_page ON edit_history(target_type, target_id, updated_at DESC, id DESC);
CREATE INDEX idx_edit_history_pending_page ON edit_history(status, updated_at DESC, id DESC) WHERE status = 'pending';

-- SQLite 不支持多态外键，以下触发器保证字段定义与目标实体匹配。
CREATE TRIGGER custom_field_values_validate_insert
BEFORE INSERT ON custom_field_values BEGIN
    SELECT CASE WHEN NOT EXISTS(
        SELECT 1 FROM custom_fields f WHERE f.id = NEW.field_id AND f.target_type = NEW.target_type
    ) THEN RAISE(ABORT, 'custom field target type mismatch') END;
    SELECT CASE WHEN NEW.target_type = 'project' AND NOT EXISTS(
        SELECT 1 FROM projects WHERE id = NEW.target_id
    ) THEN RAISE(ABORT, 'custom field project target missing') END;
    SELECT CASE WHEN NEW.target_type = 'asset' AND NOT EXISTS(
        SELECT 1 FROM assets WHERE id = NEW.target_id
    ) THEN RAISE(ABORT, 'custom field asset target missing') END;
    SELECT CASE WHEN (SELECT value_type FROM custom_fields WHERE id = NEW.field_id) IN ('text', 'date')
                      AND json_type(NEW.value_json) <> 'text'
                THEN RAISE(ABORT, 'custom field text value expected') END;
    SELECT CASE WHEN (SELECT value_type FROM custom_fields WHERE id = NEW.field_id) = 'number'
                      AND json_type(NEW.value_json) NOT IN ('integer', 'real')
                THEN RAISE(ABORT, 'custom field number value expected') END;
    SELECT CASE WHEN (SELECT value_type FROM custom_fields WHERE id = NEW.field_id) = 'boolean'
                      AND json_type(NEW.value_json) NOT IN ('true', 'false')
                THEN RAISE(ABORT, 'custom field boolean value expected') END;
END;

CREATE TRIGGER custom_field_values_validate_update
BEFORE UPDATE OF field_id, target_type, target_id, value_json ON custom_field_values BEGIN
    SELECT CASE WHEN NOT EXISTS(
        SELECT 1 FROM custom_fields f WHERE f.id = NEW.field_id AND f.target_type = NEW.target_type
    ) THEN RAISE(ABORT, 'custom field target type mismatch') END;
    SELECT CASE WHEN NEW.target_type = 'project' AND NOT EXISTS(
        SELECT 1 FROM projects WHERE id = NEW.target_id
    ) THEN RAISE(ABORT, 'custom field project target missing') END;
    SELECT CASE WHEN NEW.target_type = 'asset' AND NOT EXISTS(
        SELECT 1 FROM assets WHERE id = NEW.target_id
    ) THEN RAISE(ABORT, 'custom field asset target missing') END;
    SELECT CASE WHEN (SELECT value_type FROM custom_fields WHERE id = NEW.field_id) IN ('text', 'date')
                      AND json_type(NEW.value_json) <> 'text'
                THEN RAISE(ABORT, 'custom field text value expected') END;
    SELECT CASE WHEN (SELECT value_type FROM custom_fields WHERE id = NEW.field_id) = 'number'
                      AND json_type(NEW.value_json) NOT IN ('integer', 'real')
                THEN RAISE(ABORT, 'custom field number value expected') END;
    SELECT CASE WHEN (SELECT value_type FROM custom_fields WHERE id = NEW.field_id) = 'boolean'
                      AND json_type(NEW.value_json) NOT IN ('true', 'false')
                THEN RAISE(ABORT, 'custom field boolean value expected') END;
END;

CREATE TRIGGER edit_history_validate_insert
BEFORE INSERT ON edit_history BEGIN
    SELECT CASE WHEN NEW.target_type = 'project' AND NOT EXISTS(
        SELECT 1 FROM projects WHERE id = NEW.target_id
    ) THEN RAISE(ABORT, 'edit history project target missing') END;
    SELECT CASE WHEN NEW.target_type = 'asset' AND NOT EXISTS(
        SELECT 1 FROM assets WHERE id = NEW.target_id
    ) THEN RAISE(ABORT, 'edit history asset target missing') END;
    SELECT CASE WHEN NEW.target_type = 'prompt' AND NOT EXISTS(
        SELECT 1 FROM prompts WHERE id = NEW.target_id
    ) THEN RAISE(ABORT, 'edit history prompt target missing') END;
    SELECT CASE WHEN NEW.target_type = 'custom_field_value' AND NOT EXISTS(
        SELECT 1 FROM custom_field_values WHERE id = NEW.target_id
    ) THEN RAISE(ABORT, 'edit history custom field value target missing') END;
END;
