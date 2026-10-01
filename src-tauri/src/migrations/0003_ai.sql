-- M4：AI Provider 配置（仅凭据引用）与建议审核查询优化。
CREATE TABLE ai_providers (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('openai_compatible', 'gemini', 'ollama')),
    display_name TEXT NOT NULL COLLATE NOCASE UNIQUE CHECK (length(trim(display_name)) > 0),
    endpoint TEXT NOT NULL CHECK (length(trim(endpoint)) > 0),
    model TEXT NOT NULL DEFAULT '',
    capabilities_json TEXT NOT NULL DEFAULT '{"classification":true}' CHECK (json_valid(capabilities_json)),
    timeout_ms INTEGER NOT NULL DEFAULT 30000 CHECK (timeout_ms BETWEEN 1000 AND 60000),
    -- 仅安全存储的随机引用；严禁写入 API Key 或其他凭据明文。
    credential_id TEXT UNIQUE CHECK (credential_id IS NULL OR length(trim(credential_id)) > 0),
    is_enabled INTEGER NOT NULL DEFAULT 1 CHECK (is_enabled IN (0, 1)),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL CHECK (updated_at >= created_at)
);

ALTER TABLE ai_suggestions
    ADD COLUMN provider_id INTEGER REFERENCES ai_providers(id) ON DELETE SET NULL;

-- 审核列表按状态与稳定游标分页；原有 created_at 索引不适合状态更新时间排序。
CREATE INDEX idx_ai_suggestions_review_page
    ON ai_suggestions(status, updated_at DESC, id DESC);
CREATE INDEX idx_ai_suggestions_provider
    ON ai_suggestions(provider_id, created_at DESC, id DESC)
    WHERE provider_id IS NOT NULL;

-- 多态目标无法使用外键。删除目标时，在同一 SQLite 事务中关闭尚未审核的建议。
CREATE TRIGGER ai_suggestions_reject_pending_after_project_delete
AFTER DELETE ON projects BEGIN
    UPDATE ai_suggestions
       SET status = 'rejected',
           updated_at = max(updated_at, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
     WHERE target_type = 'project' AND target_id = OLD.id AND status = 'pending';
END;

CREATE TRIGGER ai_suggestions_reject_pending_after_asset_delete
AFTER DELETE ON assets BEGIN
    UPDATE ai_suggestions
       SET status = 'rejected',
           updated_at = max(updated_at, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
     WHERE target_type = 'asset' AND target_id = OLD.id AND status = 'pending';
END;
