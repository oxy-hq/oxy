with source as (

    select * from {{ source('ecommerce', 'raw_users') }}

),

renamed as (

    select
        id as user_id,
        first_name,
        last_name,
        email,
        country,
        cast(signup_date as date) as signup_date,
        acquisition_channel,
        is_active

    from source

)

select * from renamed
