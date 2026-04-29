{{
    config(
        materialized='ephemeral'
    )
}}

-- Enriches order items with product details

with oie_order_items as (

    select * from {{ ref('stg_order_items') }}

),

oie_products as (

    select * from {{ ref('stg_products') }}

),

oie_enriched as (

    select
        oie_order_items.order_item_id,
        oie_order_items.order_id,
        oie_order_items.product_id,
        oie_products.product_name,
        oie_products.category,
        oie_order_items.quantity,
        oie_order_items.unit_price,
        oie_order_items.discount_amount,
        oie_order_items.line_total,
        oie_products.cost as unit_cost,
        oie_products.cost * oie_order_items.quantity as line_cost,
        oie_order_items.line_total - (oie_products.cost * oie_order_items.quantity) as line_margin

    from oie_order_items

    left join oie_products
        on oie_order_items.product_id = oie_products.product_id

)

select * from oie_enriched
