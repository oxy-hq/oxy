with payments as (

    select * from {{ ref('stg_payments') }}

),

orders as (

    select * from {{ ref('stg_orders') }}

),

payment_method_summary as (

    select
        p.payment_method,
        count(distinct p.order_id)      as order_count,
        count(p.payment_id)             as payment_count,
        round(sum(p.amount), 2)         as total_revenue,
        round(avg(p.amount), 2)         as avg_payment_amount,
        round(
            100.0 * sum(p.amount) / sum(sum(p.amount)) over (),
            2
        )                               as revenue_pct

    from payments p

    left join orders o
        on p.order_id = o.order_id

    group by p.payment_method

)

select * from payment_method_summary
