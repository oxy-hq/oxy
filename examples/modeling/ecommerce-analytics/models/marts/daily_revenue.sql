{{
    config(
        materialized='incremental',
        unique_key='revenue_date'
    )
}}

-- Daily revenue summary, built incrementally

with orders as (

    select * from {{ ref('orders') }}

),

daily as (

    select
        order_date as revenue_date,
        count(distinct order_id) as total_orders,
        count(distinct user_id) as unique_customers,
        sum(case when is_first_order then 1 else 0 end) as new_customer_orders,
        sum(items_subtotal) as gross_revenue,
        sum(total_discount) as total_discounts,
        sum(shipping_cost) as total_shipping,
        sum(order_total) as net_revenue,
        sum(order_margin) as total_margin,
        sum(total_quantity) as total_items_sold,
        sum(case when order_status = 'completed' then 1 else 0 end) as completed_orders,
        sum(case when order_status = 'cancelled' then 1 else 0 end) as cancelled_orders,
        sum(case when order_status = 'refunded' then 1 else 0 end) as refunded_orders

    from orders

    {% if is_incremental() %}
    where order_date > (select max(revenue_date) from {{ this }})
    {% endif %}

    group by order_date

)

select * from daily
