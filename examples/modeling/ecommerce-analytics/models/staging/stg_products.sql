with source as (

    select * from {{ source('ecommerce', 'raw_products') }}

),

renamed as (

    select
        id as product_id,
        name as product_name,
        category,
        cast(price as double) as price,
        cast(cost as double) as cost,
        cast(created_at as date) as created_at,
        is_active

    from source

)

select * from renamed
