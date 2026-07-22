-- Daily sales rollup for the world-model dashboard (airhouse / DuckLake).
--
-- Reads the flattened Toast tree in `toast_pos` (the schema the
-- `toast_pos.airway` pipeline lands into) and materializes one
-- row per (restaurant_id, business_date). Mirrors the on-the-fly SALES_DAILY
-- reconstruction in world-model-app/src/hooks/airhouseQueries.ts so the
-- dashboard can read this tiny table instead of re-scanning orders⨝checks on
-- every tile.
--
-- Migrated from the old airway schema (`restaurant_analytics`):
--   • orders__checks                    -> order_checks      (join c.order_guid = o.guid)
--   • orders__checks__applied_discounts -> order_check_applied_discounts (ad.check_guid = c.guid)
--   • business_date is now a real DATE (was 'YYYYMMDD' VARCHAR)
--   • number_of_guests is BIGINT (was VARCHAR -> dropped the NULLIF/CAST)
-- Output is a single small table (23 stores x N days), so it is intentionally
-- NOT partitioned — keep it as one file to avoid the small-file fragmentation
-- the raw partitioned tables suffer from.
CREATE OR REPLACE TABLE toast_pos.sales_daily_metrics AS
SELECT
    o.restaurant_guid AS restaurant_id,
    o.business_date   AS business_date,
    SUM(ck.net)                       AS net_sales,
    SUM(ck.net) + SUM(ck.disc)        AS gross_sales,
    SUM(ck.disc)                      AS total_discounts,
    COUNT(*)                          AS order_count,
    SUM(o.number_of_guests)           AS guest_count,
    SUM(CASE WHEN dn.behavior = 'DINE_IN'  THEN ck.net ELSE 0 END) AS dine_in_net_sales,
    SUM(CASE WHEN dn.behavior = 'TAKE_OUT' THEN ck.net ELSE 0 END) AS takeout_net_sales,
    SUM(CASE WHEN dn.behavior = 'DELIVERY' THEN ck.net ELSE 0 END) AS delivery_net_sales
FROM toast_pos.orders o
JOIN (
    SELECT
        c.order_guid              AS ord_id,
        SUM(c.amount)             AS net,
        SUM(COALESCE(ad.disc, 0)) AS disc
    FROM toast_pos.order_checks c
    LEFT JOIN (
        SELECT check_guid AS chk_id, SUM(discount_amount) AS disc
        FROM toast_pos.order_check_applied_discounts
        GROUP BY check_guid
    ) ad ON ad.chk_id = c.guid
    WHERE c.voided = 0 AND c.deleted = 0
    GROUP BY c.order_guid
) ck ON ck.ord_id = o.guid
LEFT JOIN (
    SELECT guid, MAX(behavior) AS behavior
    FROM toast_pos.dining_options
    GROUP BY guid
) dn ON o.dining_option__guid = dn.guid
WHERE o.voided = 0 AND o.deleted = 0
GROUP BY o.restaurant_guid, o.business_date;
