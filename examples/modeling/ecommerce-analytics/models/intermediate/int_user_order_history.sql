{{
    config(
        materialized='ephemeral'
    )
}}

-- Computes per-user order history and lifetime metrics

with uoh_orders as (

    select * from {{ ref('stg_orders') }}

),

uoh_order_items as (

    select * from {{ ref('int_order_items_enriched') }}

),

uoh_order_totals as (

    select
        order_id,
        sum(line_total) as order_item_total,
        sum(line_cost) as order_cost,
        sum(line_margin) as order_margin,
        count(distinct product_id) as distinct_products,
        sum(quantity) as total_items

    from uoh_order_items

    group by order_id

),

uoh_user_orders as (

    select
        uoh_orders.user_id,
        uoh_orders.order_id,
        uoh_orders.order_date,
        uoh_orders.order_status,
        uoh_order_totals.order_item_total,
        uoh_order_totals.order_cost,
        uoh_order_totals.order_margin,
        uoh_order_totals.distinct_products,
        uoh_order_totals.total_items,
        uoh_orders.shipping_cost,
        row_number() over (
            partition by uoh_orders.user_id
            order by uoh_orders.order_date
        ) as order_sequence_number,
        count(*) over (
            partition by uoh_orders.user_id
        ) as lifetime_order_count

    from uoh_orders

    left join uoh_order_totals
        on uoh_orders.order_id = uoh_order_totals.order_id

)

select * from uoh_user_orders
