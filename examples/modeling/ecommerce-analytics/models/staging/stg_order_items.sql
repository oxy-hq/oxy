with source as (

    select * from {{ source('ecommerce', 'raw_order_items') }}

),

renamed as (

    select
        id as order_item_id,
        order_id,
        product_id,
        quantity,
        cast(unit_price as double) as unit_price,
        cast(discount_amount as double) as discount_amount,
        cast(unit_price as double) * quantity - cast(discount_amount as double) as line_total

    from source

)

select * from renamed
