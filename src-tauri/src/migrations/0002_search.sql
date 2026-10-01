-- M3：作品全文检索索引和查询辅助索引。
-- 使用独立 FTS 表，所有同步均由本 migration 中的触发器完成。
CREATE VIRTUAL TABLE asset_search USING fts5(
    asset_id UNINDEXED,
    file_name,
    prompt,
    notes,
    tags,
    categories,
    model,
    platform,
    tokenize = 'unicode61'
);

CREATE INDEX idx_assets_search_filter_page
    ON assets(media_type, rating, is_favorite, is_public, created_at DESC, updated_at DESC, id DESC);
CREATE INDEX idx_assets_model_page
    ON assets(model_id, updated_at DESC, id DESC);
CREATE INDEX idx_assets_platform_page
    ON assets(platform_id, updated_at DESC, id DESC);
CREATE INDEX idx_assets_created_page
    ON assets(created_at DESC, updated_at DESC, id DESC);
CREATE INDEX idx_assets_hash_group_page
    ON assets(content_hash, updated_at DESC, id DESC)
    WHERE content_hash IS NOT NULL;

CREATE TRIGGER asset_search_after_insert
AFTER INSERT ON assets BEGIN
    INSERT INTO asset_search(asset_id, file_name, prompt, notes, tags, categories, model, platform)
    SELECT NEW.id, NEW.file_name,
           coalesce(p.prompt_zh, '') || ' ' || coalesce(p.prompt_en, '') || ' ' || coalesce(p.negative_prompt, ''),
           NEW.notes || ' ' || coalesce((SELECT title || ' ' || description || ' ' || notes FROM projects pr WHERE pr.id = NEW.project_id), ''),
           coalesce((SELECT group_concat(t.name, ' ') FROM asset_tags at JOIN tags t ON t.id = at.tag_id WHERE at.asset_id = NEW.id), ''),
           coalesce((SELECT group_concat(c.name, ' ') FROM asset_categories ac JOIN categories c ON c.id = ac.category_id WHERE ac.asset_id = NEW.id), ''),
           coalesce(m.name, ''), coalesce(pl.name, '')
      FROM assets a
      LEFT JOIN prompts p ON p.id = a.prompt_id
      LEFT JOIN models m ON m.id = a.model_id
      LEFT JOIN platforms pl ON pl.id = a.platform_id
     WHERE a.id = NEW.id;
END;

CREATE TRIGGER asset_search_after_update
AFTER UPDATE OF project_id, prompt_id, model_id, platform_id, file_name, notes ON assets BEGIN
    DELETE FROM asset_search WHERE asset_id = NEW.id;
    INSERT INTO asset_search(asset_id, file_name, prompt, notes, tags, categories, model, platform)
    SELECT NEW.id, NEW.file_name,
           coalesce(p.prompt_zh, '') || ' ' || coalesce(p.prompt_en, '') || ' ' || coalesce(p.negative_prompt, ''),
           NEW.notes || ' ' || coalesce((SELECT title || ' ' || description || ' ' || notes FROM projects pr WHERE pr.id = NEW.project_id), ''),
           coalesce((SELECT group_concat(t.name, ' ') FROM asset_tags at JOIN tags t ON t.id = at.tag_id WHERE at.asset_id = NEW.id), ''),
           coalesce((SELECT group_concat(c.name, ' ') FROM asset_categories ac JOIN categories c ON c.id = ac.category_id WHERE ac.asset_id = NEW.id), ''),
           coalesce(m.name, ''), coalesce(pl.name, '')
      FROM assets a
      LEFT JOIN prompts p ON p.id = a.prompt_id
      LEFT JOIN models m ON m.id = a.model_id
      LEFT JOIN platforms pl ON pl.id = a.platform_id
     WHERE a.id = NEW.id;
END;

CREATE TRIGGER asset_search_after_delete
AFTER DELETE ON assets BEGIN
    DELETE FROM asset_search WHERE asset_id = OLD.id;
END;

CREATE TRIGGER asset_search_after_prompt_update
AFTER UPDATE OF prompt_zh, prompt_en, negative_prompt ON prompts BEGIN
    DELETE FROM asset_search WHERE asset_id IN (SELECT id FROM assets WHERE prompt_id = NEW.id);
    INSERT INTO asset_search(asset_id, file_name, prompt, notes, tags, categories, model, platform)
    SELECT a.id, a.file_name,
           coalesce(NEW.prompt_zh, '') || ' ' || coalesce(NEW.prompt_en, '') || ' ' || coalesce(NEW.negative_prompt, ''),
           a.notes || ' ' || coalesce((SELECT title || ' ' || description || ' ' || notes FROM projects pr WHERE pr.id = a.project_id), ''),
           coalesce((SELECT group_concat(t.name, ' ') FROM asset_tags at JOIN tags t ON t.id = at.tag_id WHERE at.asset_id = a.id), ''),
           coalesce((SELECT group_concat(c.name, ' ') FROM asset_categories ac JOIN categories c ON c.id = ac.category_id WHERE ac.asset_id = a.id), ''),
           coalesce(m.name, ''), coalesce(pl.name, '')
      FROM assets a
      LEFT JOIN models m ON m.id = a.model_id
      LEFT JOIN platforms pl ON pl.id = a.platform_id
     WHERE a.prompt_id = NEW.id;
END;

CREATE TRIGGER asset_search_after_model_update
AFTER UPDATE OF name ON models BEGIN
    DELETE FROM asset_search WHERE asset_id IN (SELECT id FROM assets WHERE model_id = NEW.id);
    INSERT INTO asset_search(asset_id, file_name, prompt, notes, tags, categories, model, platform)
    SELECT a.id, a.file_name,
           coalesce(p.prompt_zh, '') || ' ' || coalesce(p.prompt_en, '') || ' ' || coalesce(p.negative_prompt, ''),
           a.notes || ' ' || coalesce((SELECT title || ' ' || description || ' ' || notes FROM projects pr WHERE pr.id = a.project_id), ''),
           coalesce((SELECT group_concat(t.name, ' ') FROM asset_tags at JOIN tags t ON t.id = at.tag_id WHERE at.asset_id = a.id), ''),
           coalesce((SELECT group_concat(c.name, ' ') FROM asset_categories ac JOIN categories c ON c.id = ac.category_id WHERE ac.asset_id = a.id), ''),
           NEW.name, coalesce(pl.name, '')
      FROM assets a LEFT JOIN prompts p ON p.id = a.prompt_id LEFT JOIN platforms pl ON pl.id = a.platform_id
     WHERE a.model_id = NEW.id;
END;

CREATE TRIGGER asset_search_after_platform_update
AFTER UPDATE OF name ON platforms BEGIN
    DELETE FROM asset_search WHERE asset_id IN (SELECT id FROM assets WHERE platform_id = NEW.id);
    INSERT INTO asset_search(asset_id, file_name, prompt, notes, tags, categories, model, platform)
    SELECT a.id, a.file_name,
           coalesce(p.prompt_zh, '') || ' ' || coalesce(p.prompt_en, '') || ' ' || coalesce(p.negative_prompt, ''),
           a.notes || ' ' || coalesce((SELECT title || ' ' || description || ' ' || notes FROM projects pr WHERE pr.id = a.project_id), ''),
           coalesce((SELECT group_concat(t.name, ' ') FROM asset_tags at JOIN tags t ON t.id = at.tag_id WHERE at.asset_id = a.id), ''),
           coalesce((SELECT group_concat(c.name, ' ') FROM asset_categories ac JOIN categories c ON c.id = ac.category_id WHERE ac.asset_id = a.id), ''),
           coalesce(m.name, ''), NEW.name
      FROM assets a LEFT JOIN prompts p ON p.id = a.prompt_id LEFT JOIN models m ON m.id = a.model_id
     WHERE a.platform_id = NEW.id;
END;

CREATE TRIGGER asset_search_after_asset_tag_insert
AFTER INSERT ON asset_tags BEGIN
    UPDATE assets SET notes = notes WHERE id = NEW.asset_id;
END;
CREATE TRIGGER asset_search_after_asset_tag_delete
AFTER DELETE ON asset_tags BEGIN
    UPDATE assets SET notes = notes WHERE id = OLD.asset_id;
END;
CREATE TRIGGER asset_search_after_asset_category_insert
AFTER INSERT ON asset_categories BEGIN
    UPDATE assets SET notes = notes WHERE id = NEW.asset_id;
END;
CREATE TRIGGER asset_search_after_asset_category_delete
AFTER DELETE ON asset_categories BEGIN
    UPDATE assets SET notes = notes WHERE id = OLD.asset_id;
END;
CREATE TRIGGER asset_search_after_tag_update
AFTER UPDATE OF name ON tags BEGIN
    UPDATE assets SET notes = notes WHERE id IN (SELECT asset_id FROM asset_tags WHERE tag_id = NEW.id);
END;
CREATE TRIGGER asset_search_after_category_update
AFTER UPDATE OF name ON categories BEGIN
    UPDATE assets SET notes = notes WHERE id IN (SELECT asset_id FROM asset_categories WHERE category_id = NEW.id);
END;
CREATE TRIGGER asset_search_after_project_update
AFTER UPDATE OF title, description, notes ON projects BEGIN
    UPDATE assets SET notes = notes WHERE project_id = NEW.id;
END;
CREATE TRIGGER asset_search_before_project_delete
BEFORE DELETE ON projects BEGIN
    -- 先令作品失去归属，确保资产更新触发器在项目尚未删除前按空归属重建索引。
    -- 删除失败时该事务会整体回滚，不会留下索引与关系不一致的状态。
    UPDATE assets SET project_id = NULL WHERE project_id = OLD.id;
END;

-- 为现有 v1 数据建立初始索引内容。
INSERT INTO asset_search(asset_id, file_name, prompt, notes, tags, categories, model, platform)
SELECT a.id, a.file_name,
       coalesce(p.prompt_zh, '') || ' ' || coalesce(p.prompt_en, '') || ' ' || coalesce(p.negative_prompt, ''),
       a.notes || ' ' || coalesce((SELECT title || ' ' || description || ' ' || notes FROM projects pr WHERE pr.id = a.project_id), ''),
       coalesce((SELECT group_concat(t.name, ' ') FROM asset_tags at JOIN tags t ON t.id = at.tag_id WHERE at.asset_id = a.id), ''),
       coalesce((SELECT group_concat(c.name, ' ') FROM asset_categories ac JOIN categories c ON c.id = ac.category_id WHERE ac.asset_id = a.id), ''),
       coalesce(m.name, ''), coalesce(pl.name, '')
  FROM assets a
  LEFT JOIN prompts p ON p.id = a.prompt_id
  LEFT JOIN models m ON m.id = a.model_id
  LEFT JOIN platforms pl ON pl.id = a.platform_id;
