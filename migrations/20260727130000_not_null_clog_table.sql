PRAGMA foreign_keys=0;
DROP VIEW IF EXISTS v_categories_clogs;
DROP VIEW IF EXISTS v_item_data;
CREATE TABLE "new_collection_log_items" ( --We could probably just drop the table altogether but this is "safest"
	"item_id"	INTEGER NOT NULL,
	"item_name"	TEXT NOT NULL,
	"preferred_name"	TEXT NOT NULL,
	"percentage"	TEXT NOT NULL,
	"categories"	TEXT NOT NULL,
	"whitelist"	INTEGER NOT NULL DEFAULT 0,
	UNIQUE("item_id")
);
INSERT INTO new_collection_log_items SELECT * FROM collection_log_items;
DROP TABLE collection_log_items;
ALTER TABLE new_collection_log_items RENAME TO collection_log_items;
PRAGMA foreign_key_check;
PRAGMA foreign_keys=1;

--Create views (from earlier migration, needs to be redone with the table change)
CREATE VIEW IF NOT EXISTS v_categories_clogs (item_id, category) AS WITH RECURSIVE split(id, value, rest) AS (
   SELECT item_id, '', categories||',' FROM collection_log_items
   UNION ALL SELECT
   id,
   substr(rest, 0, instr(rest, ',')),
   substr(rest, instr(rest, ',')+1)
   FROM split WHERE rest!=''
)
SELECT id as item_id, trim(value) as category
FROM split
WHERE category!='';
CREATE VIEW IF NOT EXISTS v_item_data AS WITH linkedcats as (
                SELECT item_id, v_categories_clogs.category FROM v_categories_clogs
            ),
	clampedcats as (
	SELECT linkedcats.item_id, group_concat(category_table.category, ", ") as clamped_category, clamp
	FROM
	category_table
	INNER JOIN linkedcats ON linkedcats.category=category_table.category
	WHERE clamp = 1
	GROUP BY item_id),
	clogtable as (
    SELECT collection_log_entries.item_name as item_name, count(item_name) as clog_count, points from collection_log_entries where points > 0 group by item_name order by points ASC
)
SELECT collection_log_items.item_id as item_id, collection_log_items.item_name as item_name, preferred_name, categories, percentage, coalesce(points,0) as highest_points, whitelist, coalesce(clog_count,0) as clog_count, coalesce(clamp,0) as clamp, coalesce(clamped_category," ") as clamped_category
FROM collection_log_items
LEFT JOIN clampedcats ON clampedcats.item_id=collection_log_items.item_id
LEFT JOIN clogtable ON clogtable.item_name=collection_log_items.item_name
ORDER BY item_id;