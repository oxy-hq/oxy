{{
    config(
        materialized='table'
    )
}}

-- Customer dimension with lifetime metrics

with users as (

    select * from {{ ref('stg_users') }}

),

user_orders as (

    select * from {{ ref('int_user_order_history') }}

),

customer_summary as (

    select
        user_id,
        min(order_date) as first_order_date,
        max(order_date) as most_recent_order_date,
        count(distinct order_id) as lifetime_orders,
        sum(order_item_total) as lifetime_revenue,
        sum(order_cost) as lifetime_cost,
        sum(order_margin) as lifetime_margin,
        sum(shipping_cost) as lifetime_shipping_paid,
        sum(total_items) as lifetime_items_purchased,
        avg(order_item_total) as avg_order_value,
        sum(case when order_status = 'refunded' then 1 else 0 end) as refunded_orders,
        sum(case when order_status = 'cancelled' then 1 else 0 end) as cancelled_orders

    from user_orders

    group by user_id

),

final as (

    select
        users.user_id,
        users.first_name,
        users.last_name,
        users.email,
        users.country,
        users.signup_date,
        users.acquisition_channel,
        users.is_active,
        coalesce(customer_summary.lifetime_orders, 0) as lifetime_orders,
        customer_summary.first_order_date,
        customer_summary.most_recent_order_date,
        coalesce(customer_summary.lifetime_revenue, 0) as lifetime_revenue,
        coalesce(customer_summary.lifetime_cost, 0) as lifetime_cost,
        coalesce(customer_summary.lifetime_margin, 0) as lifetime_margin,
        coalesce(customer_summary.avg_order_value, 0) as avg_order_value,
        coalesce(customer_summary.lifetime_items_purchased, 0) as lifetime_items_purchased,
        coalesce(customer_summary.refunded_orders, 0) as refunded_orders,
        coalesce(customer_summary.cancelled_orders, 0) as cancelled_orders,
        case
            when customer_summary.lifetime_orders is null then 'never_ordered'
            when customer_summary.lifetime_orders = 1 then 'single_purchaser'
            when customer_summary.lifetime_orders between 2 and 5 then 'repeat_buyer'
            else 'loyal_customer'
        end as customer_segment

    from users

    left join customer_summary
        on users.user_id = customer_summary.user_id

)

select * from final
