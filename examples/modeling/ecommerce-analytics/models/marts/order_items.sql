{{
    config(
        materialized='table'
    )
}}

-- Order item fact table with full enrichment

with enriched_items as (

    select * from {{ ref('int_order_items_enriched') }}

),

orders as (

    select * from {{ ref('stg_orders') }}

),

final as (

    select
        enriched_items.order_item_id,
        enriched_items.order_id,
        orders.user_id,
        orders.order_date,
        orders.order_status,
        enriched_items.product_id,
        enriched_items.product_name,
        enriched_items.category,
        enriched_items.quantity,
        enriched_items.unit_price,
        enriched_items.unit_cost,
        enriched_items.discount_amount,
        enriched_items.line_total,
        enriched_items.line_cost,
        enriched_items.line_margin,
        case
            when enriched_items.discount_amount > 0 then true
            else false
        end as is_discounted

    from enriched_items

    inner join orders
        on enriched_items.order_id = orders.order_id

)

select * from final
