WITH responses AS (
    SELECT {{ variable_a }} AS {{ variable_b }}
)
SELECT
    {{ variable_c }}
FROM
    responses;
