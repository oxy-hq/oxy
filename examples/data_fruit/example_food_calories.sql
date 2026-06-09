/*
 oxy:
    database: local
    embed: |
        Calories in fruit
*/


select 'apple' as name,
    80 as calories
union all
select 'banana' as name,
    106 as calories
union all
select 'cherry' as name,
    50 as calories
union all
select 'kiwi' as name,
    42 as calories
union all
select 'orange' as name,
    62 as calories
union all
select 'pear' as name,
    102 as calories
