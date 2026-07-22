-- Daily labor rollup for the world-model dashboard (airhouse / DuckLake).
--
-- Reads the flattened Toast `time_entries` in `toast_pos` and materializes
-- one row per (restaurant_id, business_date). Mirrors the on-the-fly LABOR_DAILY
-- reconstruction in world-model-app/src/hooks/airhouseQueries.ts.
--
-- Migrated from the old airway schema (`restaurant_analytics`):
--   • keyed on restaurant_guid (the flat restaurant_id is null on these rows),
--     aliased back to restaurant_id on output
--   • out_date is now a TIMESTAMP, so an open shift is out_date IS NULL
--     (was `out_date IS NULL OR out_date = ''` on the old VARCHAR column)
-- time_entries.business_date is still a 'YYYYMMDD' VARCHAR, so it is parsed to a
-- real DATE here. Output is one small table — intentionally not partitioned.
CREATE OR REPLACE TABLE toast_pos.labor_daily_metrics AS
SELECT
    restaurant_guid AS restaurant_id,
    CAST(strptime(business_date, '%Y%m%d') AS DATE) AS business_date,
    SUM(COALESCE(regular_hours, 0)  * COALESCE(hourly_wage, 0))
      + SUM(COALESCE(overtime_hours, 0) * COALESCE(hourly_wage, 0) * 1.5) AS labor_cost,
    SUM(COALESCE(regular_hours, 0) + COALESCE(overtime_hours, 0)) AS labor_hours,
    SUM(COALESCE(regular_hours, 0))                              AS regular_hours,
    SUM(COALESCE(overtime_hours, 0))                            AS overtime_hours,
    SUM(COALESCE(regular_hours, 0)  * COALESCE(hourly_wage, 0))  AS regular_labor_cost,
    SUM(COALESCE(overtime_hours, 0) * COALESCE(hourly_wage, 0) * 1.5) AS overtime_labor_cost,
    SUM(CASE WHEN out_date IS NULL THEN 1 ELSE 0 END) AS open_shifts,
    COUNT(DISTINCT employee_reference__guid)                    AS employees_worked,
    COUNT(*)                                                    AS shifts_count
FROM toast_pos.time_entries
WHERE deleted = 0
GROUP BY restaurant_guid, CAST(strptime(business_date, '%Y%m%d') AS DATE);
