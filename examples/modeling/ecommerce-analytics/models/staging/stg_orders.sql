with source as (

    select * from {{ source('ecommerce', 'raw_orders') }}

),

renamed as (

    select
        id as order_id,
        user_id,
        cast(order_date as date) as order_date,
        status as order_status,
        shipping_address_country,
        cast(shipping_cost as double) as shipping_cost

    from source

)

select * from renamed
