/*
  oxy:
    database: local
    embed:
      - How do average weekly sales compare across stores
*/
SELECT Store, AVG(Weekly_Sales) AS avg_weekly_sales
FROM 'oxymart.csv'
GROUP BY Store
ORDER BY avg_weekly_sales DESC;
