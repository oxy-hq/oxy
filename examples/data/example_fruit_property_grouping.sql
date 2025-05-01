WITH responses AS (
    SELECT 'Fruit' AS property_grouping
    UNION ALL
    SELECT 'Vegetable' AS property_grouping
)
SELECT
    property_grouping
FROM responses;
