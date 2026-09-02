-- Landing tables for the `toast` Airway pipeline.
--
-- `raw_*` schemas hold ETL'd data and are analyst-readable by default, unlike
-- `app_*` — which is why the semantic model can query this without an opt-in.
CREATE SCHEMA IF NOT EXISTS raw_toast;

CREATE TABLE IF NOT EXISTS raw_toast.sales (
    id            BIGINT PRIMARY KEY,
    business_date DATE NOT NULL,
    net_sales     NUMERIC(12,2) NOT NULL,
    location      TEXT NOT NULL
);

INSERT INTO raw_toast.sales (id, business_date, net_sales, location) VALUES
    (1, DATE '2026-08-01', 4210.55, 'downtown'),
    (2, DATE '2026-08-02', 3980.10, 'downtown'),
    (3, DATE '2026-08-01', 2015.00, 'airport')
ON CONFLICT (id) DO NOTHING;
