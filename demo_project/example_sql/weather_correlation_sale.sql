/*
  oxy:
    database: local
    embed:
      - Is there a correlation between temperature and weekly sales
      - Effect of temparature on sales
      - Correlation between temparature and sales
*/
SELECT
    CASE
        WHEN Temperature < 40 THEN 'Cold (< 40°F)'
        WHEN Temperature BETWEEN 40 AND 70 THEN 'Moderate (40-70°F)'
        ELSE 'Hot (> 70°F)'
    END AS temp_range,
    AVG(Weekly_Sales) AS avg_sales,
    COUNT(*) AS num_weeks
FROM '.db/oxymart.csv'
GROUP BY temp_range
ORDER BY avg_sales DESC;
