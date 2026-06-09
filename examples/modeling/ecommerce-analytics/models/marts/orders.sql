{{
    config(
        materialized='table'
    )
}}

-- Order fact table with item and payment summaries

with orders as (

    select * from {{ ref('stg_orders') }}

),

user_orders as (

    select * from {{ ref('int_user_order_history') }}

),

order_payments as (

    select * from {{ ref('int_order_payments') }}

),

order_items_agg as (

    select
        order_id,
        count(*) as item_count,
        sum(quantity) as total_quantity,
        sum(line_total) as items_subtotal,
        sum(discount_amount) as total_discount,
        sum(line_margin) as order_margin,
        count(distinct category) as distinct_categories

    from {{ ref('int_order_items_enriched') }}

    group by order_id

),

final as (

    select
        orders.order_id,
        orders.user_id,
        orders.order_date,
        orders.order_status,
        orders.shipping_address_country,
        orders.shipping_cost,
        user_orders.order_sequence_number,
        user_orders.order_sequence_number = 1 as is_first_order,
        order_items_agg.item_count,
        order_items_agg.total_quantity,
        order_items_agg.items_subtotal,
        order_items_agg.total_discount,
        order_items_agg.order_margin,
        order_items_agg.distinct_categories,
        coalesce(order_payments.total_paid, 0) as total_paid,
        coalesce(order_payments.total_refunded, 0) as total_refunded,
        coalesce(order_payments.payment_count, 0) as payment_count,
        coalesce(order_payments.had_failed_payment, 0) as had_failed_payment,
        coalesce(order_items_agg.items_subtotal, 0) + orders.shipping_cost as order_total

    from orders

    left join user_orders
        on orders.order_id = user_orders.order_id

    left join order_payments
        on orders.order_id = order_payments.order_id

    left join order_items_agg
        on orders.order_id = order_items_agg.order_id

)

select * from final
