{{
    config(
        materialized='table'
    )
}}

-- Category-level performance summary with rankings

with order_items as (

    select * from {{ ref('order_items') }}

),

category_metrics as (

    select
        category,
        count(distinct order_id) as total_orders,
        count(distinct user_id) as unique_customers,
        sum(quantity) as total_units_sold,
        sum(line_total) as total_revenue,
        sum(line_cost) as total_cost,
        sum(line_margin) as total_margin,
        avg(line_total) as avg_item_value,
        sum(discount_amount) as total_discounts,
        sum(case when is_discounted then 1 else 0 end) as discounted_item_count,
        count(*) as total_line_items

    from order_items

    where order_status not in ('cancelled', 'refunded')

    group by category

),

with_rankings as (

    select
        *,
        rank() over (order by total_revenue desc) as revenue_rank,
        rank() over (order by total_units_sold desc) as volume_rank,
        rank() over (order by total_margin desc) as margin_rank,
        total_margin / nullif(total_revenue, 0) as margin_pct,
        cast(discounted_item_count as double) / nullif(total_line_items, 0) as discount_rate

    from category_metrics

)

select * from with_rankings
