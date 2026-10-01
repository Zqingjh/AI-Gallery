-- M10：元数据预设默认值，以及“图片在前、视频在后”的持续导入编号规则。

-- 历史触发器统一把新作品追加到末尾；升级后改为按媒体类型插入。
DROP TRIGGER asset_display_order_after_insert;

CREATE TRIGGER asset_display_order_after_insert_image
AFTER INSERT ON assets
WHEN NEW.media_type = 'image'
BEGIN
    -- 新图片插入到现有图片之后；先使用负值规避 position UNIQUE 的中间冲突。
    UPDATE asset_display_order
       SET position = -position
     WHERE position > COALESCE((
         SELECT MAX(ao.position)
           FROM asset_display_order ao
           JOIN assets a ON a.id = ao.asset_id
          WHERE a.media_type = 'image'
     ), 0);
    UPDATE asset_display_order
       SET position = -position + 1
     WHERE position < 0;
    INSERT INTO asset_display_order(asset_id, position)
    VALUES (
        NEW.id,
        COALESCE((
            SELECT MAX(ao.position)
              FROM asset_display_order ao
              JOIN assets a ON a.id = ao.asset_id
             WHERE a.media_type = 'image'
        ), 0) + 1
    );
END;

CREATE TRIGGER asset_display_order_after_insert_video
AFTER INSERT ON assets
WHEN NEW.media_type = 'video'
BEGIN
    INSERT INTO asset_display_order(asset_id, position)
    VALUES (NEW.id, COALESCE((SELECT MAX(position) FROM asset_display_order), 0) + 1);
END;

-- 默认预设只补充缺失名称，不覆盖用户已经创建或修改的记录。
INSERT OR IGNORE INTO platforms(name, created_at, updated_at)
VALUES
    ('豆包', 0, 0),
    ('千问', 0, 0),
    ('Gemini', 0, 0),
    ('Google Flow', 0, 0),
    ('即梦', 0, 0),
    ('ChatGPT', 0, 0),
    ('Midjourney', 0, 0),
    ('ComfyUI', 0, 0);

INSERT OR IGNORE INTO models(name, provider, created_at, updated_at)
VALUES
    ('Seedream 5.0 Lite', '豆包', 0, 0),
    ('Seedance 2.0', '豆包', 0, 0),
    ('GPT-Image 2', 'OpenAI', 0, 0),
    ('Imagen 4', 'Google', 0, 0),
    ('Midjourney V7', 'Midjourney', 0, 0),
    ('FLUX.1', 'Black Forest Labs', 0, 0),
    ('Stable Diffusion XL', 'Stability AI', 0, 0);
