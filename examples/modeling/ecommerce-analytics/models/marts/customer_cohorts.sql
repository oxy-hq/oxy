{{
    config(
        materialized='table'
    )
}}

-- Monthly cohort analysis based on first order date

with customers as (

    select * from {{ ref('customers') }}

),

orders as (

    select * from {{ ref('orders') }}

),

cohort_base as (

    select
        customers.user_id,
        -- Truncate to month for cohort assignment
        cast(customers.first_order_date as varchar) as raw_date,
        customers.first_order_date as cohort_date,
        customers.acquisition_channel,
        customers.country

    from customers

    where customers.first_order_date is not null

),

cohort_orders as (

    select
        cohort_base.cohort_date,
        cohort_base.acquisition_channel,
        orders.order_date,
        count(distinct cohort_base.user_id) as customers_ordering,
        count(distinct orders.order_id) as order_count,
        sum(orders.order_total) as revenue

    from cohort_base

    inner join orders
        on cohort_base.user_id = orders.user_id

    group by
        cohort_base.cohort_date,
        cohort_base.acquisition_channel,
        orders.order_date

)

select * from cohort_orders
