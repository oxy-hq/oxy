{{
    config(
        materialized='table'
    )
}}

-- User engagement metrics combining page views, orders, and reviews

with users as (

    select * from {{ ref('stg_users') }}

),

page_views as (

    select * from {{ ref('stg_page_views') }}

),

reviews as (

    select * from {{ ref('stg_product_reviews') }}

),

customers as (

    select * from {{ ref('customers') }}

),

user_page_views as (

    select
        user_id,
        count(*) as total_page_views,
        count(distinct session_id) as total_sessions,
        count(distinct cast(viewed_at as date)) as active_days,
        sum(case when page_url = '/products' or page_url = '/products/detail' then 1 else 0 end) as product_page_views,
        sum(case when page_url = '/cart' then 1 else 0 end) as cart_views,
        sum(case when page_url = '/checkout' then 1 else 0 end) as checkout_views,
        sum(case when device_type = 'mobile' then 1 else 0 end) as mobile_views,
        sum(case when device_type = 'desktop' then 1 else 0 end) as desktop_views,
        min(viewed_at) as first_visit,
        max(viewed_at) as last_visit

    from page_views

    group by user_id

),

user_reviews as (

    select
        user_id,
        count(*) as total_reviews,
        avg(cast(rating as double)) as avg_rating_given

    from reviews

    group by user_id

),

final as (

    select
        users.user_id,
        users.first_name,
        users.last_name,
        users.acquisition_channel,
        customers.customer_segment,
        customers.lifetime_orders,
        customers.lifetime_revenue,
        coalesce(user_page_views.total_page_views, 0) as total_page_views,
        coalesce(user_page_views.total_sessions, 0) as total_sessions,
        coalesce(user_page_views.active_days, 0) as active_days,
        coalesce(user_page_views.product_page_views, 0) as product_page_views,
        coalesce(user_page_views.cart_views, 0) as cart_views,
        coalesce(user_page_views.checkout_views, 0) as checkout_views,
        coalesce(user_page_views.mobile_views, 0) as mobile_views,
        coalesce(user_page_views.desktop_views, 0) as desktop_views,
        user_page_views.first_visit,
        user_page_views.last_visit,
        coalesce(user_reviews.total_reviews, 0) as total_reviews,
        user_reviews.avg_rating_given,
        case
            when user_page_views.total_sessions is null then 'inactive'
            when user_page_views.total_sessions = 1 then 'one_time_visitor'
            when user_page_views.total_sessions between 2 and 5 then 'casual'
            else 'power_user'
        end as engagement_tier

    from users

    left join customers
        on users.user_id = customers.user_id

    left join user_page_views
        on users.user_id = user_page_views.user_id

    left join user_reviews
        on users.user_id = user_reviews.user_id

)

select * from final
