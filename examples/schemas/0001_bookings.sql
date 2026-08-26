-- Tables for the `bookings` custom app. Applied to the org's OLTP database in
-- filename order; the zero-padded prefix IS the ordering.
CREATE SCHEMA IF NOT EXISTS app_bookings;

CREATE TABLE IF NOT EXISTS app_bookings.customers (
    id         BIGSERIAL PRIMARY KEY,
    email      TEXT NOT NULL UNIQUE,
    full_name  TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_customers_email ON app_bookings.customers (email);
