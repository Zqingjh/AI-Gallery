-- M9：修复已升级工作区中的图片时长与媒体类型排序，并为后续写入增加数据库兜底。

DROP TRIGGER IF EXISTS asset_display_order_after_insert;
DROP TRIGGER IF EXISTS asset_display_order_after_insert_image;
DROP TRIGGER IF EXISTS asset_display_order_after_insert_video;
DROP TRIGGER IF EXISTS asset_image_duration_after_insert;
DROP TRIGGER IF EXISTS asset_image_duration_after_update;

-- 已有图片不应保留视频专属时长。
UPDATE assets
   SET duration_ms = NULL
 WHERE media_type = 'image'
   AND duration_ms IS NOT NULL;

-- 先把旧位置移入负数空间，再按“图片在前、视频在后”恢复连续正编号。
UPDATE asset_display_order SET position = -position;
WITH ordered AS (
    SELECT ao.asset_id,
           row_number() OVER (
               ORDER BY CASE a.media_type WHEN 'image' THEN 0 ELSE 1 END,
                        -ao.position ASC,
                        ao.asset_id ASC
           ) AS new_position
      FROM asset_display_order ao
      JOIN assets a ON a.id = ao.asset_id
)
UPDATE asset_display_order
   SET position = (
       SELECT ordered.new_position
         FROM ordered
        WHERE ordered.asset_id = asset_display_order.asset_id
   );

CREATE TRIGGER asset_display_order_after_insert_image
AFTER INSERT ON assets
WHEN NEW.media_type = 'image'
BEGIN
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

CREATE TRIGGER asset_image_duration_after_insert
AFTER INSERT ON assets
WHEN NEW.media_type = 'image' AND NEW.duration_ms IS NOT NULL
BEGIN
    UPDATE assets SET duration_ms = NULL WHERE id = NEW.id;
END;

CREATE TRIGGER asset_image_duration_after_update
AFTER UPDATE OF media_type, duration_ms ON assets
WHEN NEW.media_type = 'image' AND NEW.duration_ms IS NOT NULL
BEGIN
    UPDATE assets SET duration_ms = NULL WHERE id = NEW.id;
END;
