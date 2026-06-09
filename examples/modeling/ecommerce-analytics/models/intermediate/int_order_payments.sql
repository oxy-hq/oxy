{{
    config(
        materialized='ephemeral'
    )
}}

-- Aggregates payments at the order level

with op_payments as (

    select * from {{ ref('stg_payments') }}

),

op_order_payments as (

    select
        order_id,
        sum(case when payment_status = 'success' then amount else 0 end) as total_paid,
        sum(case when payment_status = 'refunded' then amount else 0 end) as total_refunded,
        count(distinct payment_id) as payment_count,
        count(distinct payment_method) as distinct_payment_methods,
        min(payment_date) as first_payment_date,
        max(payment_date) as last_payment_date,
        max(case when payment_status = 'failed' then 1 else 0 end) as had_failed_payment

    from op_payments

    group by order_id

)

select * from op_order_payments
