-- Hourly order/check revenue rollup for the world-model dashboard's hour tiles.
--
-- Pre-aggregates toast_pos.orders ⨝ order_checks to one row per
-- (restaurant_id, business_date, hour_utc) so dashHourly / storeHourly stop
-- scanning the raw order/check tree on every load.
--
--   hour_utc : EXTRACT(hour FROM opened_date) in UTC. The dashboard applies the
--              America/Los_Angeles offset (- D.hourOffset) in JS
--              (airhouseDates.ts), so store the raw UTC hour here. This is the
--              exact EXTRACT(hour FROM CAST(... AS TIMESTAMP)) form that
--              airhouse's PG-compat binder handles (docs/airhouse-dialect.md §3).
--   revenue  : SUM of valid check amounts. INNER JOIN — an order with no valid
--              check contributes no hourly revenue anyway.
--
-- Migrated from the old airway schema (orders__checks, c._aw_parent_id = o._aw_id)
-- to flattened toast_pos (order_checks, c.order_guid = o.guid). business_date
-- is now a real DATE. Small output, not partitioned; CREATE OR REPLACE is
-- idempotent and safe to re-run.
CREATE OR REPLACE TABLE toast_pos.orders_hourly_metrics AS
SELECT
  o.restaurant_guid AS restaurant_id,
  o.business_date   AS business_date,            -- DATE
  EXTRACT(hour FROM CAST(o.opened_date AS TIMESTAMP)) AS hour_utc,
  SUM(c.amount) AS revenue
FROM toast_pos.orders o
INNER JOIN toast_pos.order_checks c
  ON c.order_guid = o.guid
WHERE o.voided = 0 AND o.deleted = 0
  AND c.voided = 0 AND c.deleted = 0
  AND o.restaurant_guid <> ''
GROUP BY o.restaurant_guid, o.business_date, hour_utc;
