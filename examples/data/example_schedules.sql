WITH
    responses AS (
        SELECT 5 as schedules
        UNION ALL
        SELECT 15 as schedules
    )
SELECT schedules
FROM responses;