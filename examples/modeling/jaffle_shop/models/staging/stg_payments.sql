with source as (

    select * from {{ source('jaffle_shop', 'raw_payments') }}

),

renamed as (

    select
        id as payment_id,
        order_id,
        payment_method,
        -- amount is stored in cents, convert to dollars
        amount / 100.0 as amount

    from source

)

select * from renamed
