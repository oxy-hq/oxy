with source as (

    select * from {{ source('web_analytics', 'raw_product_reviews') }}

),

renamed as (

    select
        id as review_id,
        user_id,
        product_id,
        rating,
        review_text,
        cast(created_at as date) as review_date

    from source

)

select * from renamed
