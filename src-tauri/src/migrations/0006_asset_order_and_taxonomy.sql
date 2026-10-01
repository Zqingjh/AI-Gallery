-- M8：作品稳定编号与常用分类。编号独立于 updated_at，编辑元数据不会改变展示位置。
CREATE TABLE asset_display_order (
    asset_id INTEGER PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
    position INTEGER NOT NULL UNIQUE
);

INSERT INTO asset_display_order(asset_id, position)
SELECT id,
       row_number() OVER (
           ORDER BY CASE media_type WHEN 'image' THEN 0 ELSE 1 END,
                    updated_at ASC,
                    id ASC
       )
FROM assets;

CREATE INDEX idx_asset_display_order_position
    ON asset_display_order(position ASC);

-- 新导入和从回收站恢复的作品默认追加到末尾，避免编辑或恢复挤占已有编号。
CREATE TRIGGER asset_display_order_after_insert
AFTER INSERT ON assets BEGIN
    INSERT INTO asset_display_order(asset_id, position)
    VALUES (NEW.id, COALESCE((SELECT MAX(position) FROM asset_display_order), 0) + 1);
END;

-- 删除前先压缩后续编号；先转成负值避免 UNIQUE 约束的中间冲突。
CREATE TRIGGER asset_display_order_before_delete
BEFORE DELETE ON assets BEGIN
    UPDATE asset_display_order
    SET position = -position
    WHERE position > (SELECT position FROM asset_display_order WHERE asset_id = OLD.id);
    UPDATE asset_display_order
    SET position = 0
    WHERE asset_id = OLD.id;
    UPDATE asset_display_order
    SET position = -position - 1
    WHERE position < 0;
END;

-- 新工作区提供可直接使用的常用分类；已有同名维度或分类保持不变。
INSERT OR IGNORE INTO dimensions(name, allows_multiple, ai_can_suggest_new, is_enabled, created_at, updated_at)
VALUES
    ('主体', 1, 1, 1, 0, 0),
    ('场景', 1, 1, 1, 0, 0),
    ('视觉风格', 1, 1, 1, 0, 0),
    ('用途与内容类型', 1, 1, 1, 0, 0);

INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '人像', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '主体';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '角色', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '主体';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '产品', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '主体';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '风景', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '场景';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '城市', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '场景';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '室内', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '场景';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '写实', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '视觉风格';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '动漫', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '视觉风格';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '3D 渲染', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '视觉风格';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '国风', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '视觉风格';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '游戏 CG', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '用途与内容类型';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '概念设计', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '用途与内容类型';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '海报', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '用途与内容类型';
