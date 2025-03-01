/* calculates sleep over the last week compared to the previous year */
WITH recent_week AS (
    SELECT "Asleep duration (min)", "In bed duration (min)", "Sleep efficiency %" 
    FROM "sleeps.csv"
    WHERE "Cycle start time" >= (SELECT MAX("Cycle start time") FROM "sleeps.csv") - INTERVAL '7 days'
),
recent_year AS (
    SELECT "Asleep duration (min)", "In bed duration (min)", "Sleep efficiency %" 
    FROM "sleeps.csv"
    WHERE "Cycle start time" >= (SELECT MAX("Cycle start time") FROM "sleeps.csv") - INTERVAL '1 year'
)
SELECT 
    AVG("Asleep duration (min)") AS avg_asleep_duration_week,
    AVG("In bed duration (min)") AS avg_in_bed_duration_week,
    AVG("Sleep efficiency %") AS avg_sleep_efficiency_week
FROM recent_week
UNION ALL
SELECT 
    AVG("Asleep duration (min)") AS avg_asleep_duration_year,
    AVG("In bed duration (min)") AS avg_in_bed_duration_year,
    AVG("Sleep efficiency %") AS avg_sleep_efficiency_year
FROM recent_year;

