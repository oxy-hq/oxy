with source as (

    select * from {{ source('ecommerce', 'raw_payments') }}

),

renamed as (

    select
        id as payment_id,
        order_id,
        payment_method,
        cast(amount as double) as amount,
        cast(payment_date as date) as payment_date,
        status as payment_status

    from source

)

select * from renamed
