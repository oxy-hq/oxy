WITH responses AS (
    SELECT {{ variable_a }} AS {{ variable_c }}
)
SELECT
    {{ variable_c }}
FROM
    responses;
