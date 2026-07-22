-- Daily order/check rollup for the world-model dashboard's raw-scan tiles.
--
-- Pre-aggregates toast_pos.orders ⨝ order_checks to one row per
-- (restaurant_id, business_date) so the tiles that don't read
-- sales_daily_metrics (heartRates, sparklines, ribbonSales, entityMetrics,
-- laborByStoreRevenue) stop scanning the raw order/check tree on every load.
--
-- Columns:
--   order_count   : distinct valid orders per store-day (LEFT JOIN so an order
--                   with no/voided checks still counts — matches heartRates'
--                   raw count() of valid orders).
--   check_revenue : SUM of valid check amounts — gross check $ (the SUM(c.amount)
--                   the raw tiles compute), NOT net_sales.
--   last_paid_date: max paid_date per store-day (coarse freshness fallback for
--                   ripplesNewOrders; that tile otherwise stays on a bounded raw
--                   `orders` scan to stay fresh between rollup runs).
--
-- Migrated from the old airway schema (restaurant_analytics.orders__checks,
-- joined on c._aw_parent_id = o._aw_id) to the flattened toast_pos
-- (order_checks, joined on c.order_guid = o.guid). business_date is now a real
-- DATE (was 'YYYYMMDD' VARCHAR). Output is one small table (23 stores x N days),
-- intentionally not partitioned. CREATE OR REPLACE → idempotent, safe to re-run.
CREATE OR REPLACE TABLE toast_pos.orders_daily_metrics AS
SELECT
  o.restaurant_guid AS restaurant_id,
  o.business_date   AS business_date,            -- DATE
  count(DISTINCT o.guid) AS order_count,
  SUM(c.amount) FILTER (WHERE c.voided = 0 AND c.deleted = 0) AS check_revenue,
  max(o.paid_date)  AS last_paid_date
FROM toast_pos.orders o
LEFT JOIN toast_pos.order_checks c
  ON c.order_guid = o.guid
WHERE o.voided = 0 AND o.deleted = 0 AND o.restaurant_guid <> ''
GROUP BY o.restaurant_guid, o.business_date;
