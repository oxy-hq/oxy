{{
    config(
        materialized='ephemeral'
    )
}}

-- Aggregates product-level sales and review metrics

with pm_order_items as (

    select * from {{ ref('int_order_items_enriched') }}

),

pm_orders as (

    select * from {{ ref('stg_orders') }}

),

pm_reviews as (

    select * from {{ ref('stg_product_reviews') }}

),

pm_product_sales as (

    select
        pm_order_items.product_id,
        count(distinct pm_order_items.order_id) as total_orders,
        sum(pm_order_items.quantity) as total_units_sold,
        sum(pm_order_items.line_total) as total_revenue,
        sum(pm_order_items.line_cost) as total_cost,
        sum(pm_order_items.line_margin) as total_margin,
        avg(pm_order_items.discount_amount) as avg_discount,
        min(pm_orders.order_date) as first_sold_date,
        max(pm_orders.order_date) as last_sold_date

    from pm_order_items

    inner join pm_orders
        on pm_order_items.order_id = pm_orders.order_id

    where pm_orders.order_status not in ('cancelled', 'refunded')

    group by pm_order_items.product_id

),

pm_product_reviews as (

    select
        product_id,
        count(*) as review_count,
        avg(cast(rating as double)) as avg_rating,
        sum(case when rating >= 4 then 1 else 0 end) as positive_review_count,
        sum(case when rating <= 2 then 1 else 0 end) as negative_review_count

    from pm_reviews

    group by product_id

),

pm_combined as (

    select
        pm_product_sales.*,
        pm_product_reviews.review_count,
        pm_product_reviews.avg_rating,
        pm_product_reviews.positive_review_count,
        pm_product_reviews.negative_review_count

    from pm_product_sales

    left join pm_product_reviews
        on pm_product_sales.product_id = pm_product_reviews.product_id

)

select * from pm_combined
