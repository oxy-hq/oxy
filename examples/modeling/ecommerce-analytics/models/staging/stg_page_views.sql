with source as (

    select * from {{ source('web_analytics', 'raw_page_views') }}

),

renamed as (

    select
        id as page_view_id,
        user_id,
        page_url,
        referrer,
        session_id,
        cast(viewed_at as timestamp) as viewed_at,
        device_type

    from source

)

select * from renamed
