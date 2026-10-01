-- M9：补充常用分类。只新增缺失项，不改写用户已有分类或关联。
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '动物', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '主体';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '建筑', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '主体';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '食物', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '主体';

INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '自然', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '场景';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '科幻', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '场景';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '赛博朋克', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '场景';

INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '插画', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '视觉风格';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '水彩', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '视觉风格';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '油画', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '视觉风格';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '像素艺术', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '视觉风格';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '极简', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '视觉风格';

INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '角色设定', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '用途与内容类型';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '场景设定', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '用途与内容类型';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '电商', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '用途与内容类型';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '社交媒体', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '用途与内容类型';
INSERT OR IGNORE INTO categories(dimension_id, name, aliases_json, description, color, icon, is_enabled, created_at, updated_at)
SELECT id, '分镜', '[]', '', NULL, NULL, 1, 0, 0 FROM dimensions WHERE name = '用途与内容类型';
