/*
 oxy:
    embed:
        - fruit sales
        - apple sales
        - banana sales
        - cherry sales
        - kiwi sales
        - orange sales
*/


select 'apple' as name,
    325 as sales
union all
select 'banana' as name,
    2000 as sales
union all
select 'cherry' as name,
    18 as sales
union all
select 'kiwi' as name,
    120 as sales
union all
select 'orange' as name,
    1500 as sales