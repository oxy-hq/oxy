WITH responses AS (
    SELECT 'monthly' as intervals
    UNION ALL
    SELECT 'weekly' as intervals
)
SELECT
  intervals
FROM responses;
