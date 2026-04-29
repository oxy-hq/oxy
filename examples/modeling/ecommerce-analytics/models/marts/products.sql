{{
    config(
        materialized='table'
    )
}}

-- Product dimension with sales and review metrics

with products as (

    select * from {{ ref('stg_products') }}

),

product_metrics as (

    select * from {{ ref('int_product_metrics') }}

),

final as (

    select
        products.product_id,
        products.product_name,
        products.category,
        products.price as current_price,
        products.cost as current_cost,
        products.price - products.cost as unit_margin,
        products.created_at,
        products.is_active,
        coalesce(product_metrics.total_orders, 0) as total_orders,
        coalesce(product_metrics.total_units_sold, 0) as total_units_sold,
        coalesce(product_metrics.total_revenue, 0) as total_revenue,
        coalesce(product_metrics.total_cost, 0) as total_cost,
        coalesce(product_metrics.total_margin, 0) as total_margin,
        coalesce(product_metrics.avg_discount, 0) as avg_discount,
        product_metrics.first_sold_date,
        product_metrics.last_sold_date,
        coalesce(product_metrics.review_count, 0) as review_count,
        product_metrics.avg_rating,
        coalesce(product_metrics.positive_review_count, 0) as positive_review_count,
        coalesce(product_metrics.negative_review_count, 0) as negative_review_count,
        case
            when product_metrics.total_units_sold is null or product_metrics.total_units_sold = 0 then 'no_sales'
            when product_metrics.total_units_sold < 10 then 'low_volume'
            when product_metrics.total_units_sold < 50 then 'medium_volume'
            else 'high_volume'
        end as sales_tier

    from products

    left join product_metrics
        on products.product_id = product_metrics.product_id

)

select * from final
