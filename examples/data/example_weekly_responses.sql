-- Test connection
SELECT
  DATE_TRUNC(ts_submitted_at, WEEK(MONDAY)) AS week_start,
  COUNT(*) AS total_responses
FROM
  `df-warehouse-prod.dbt_prod_core.fct_typeform_responses`
GROUP BY
  week_start
ORDER BY
  week_start;
