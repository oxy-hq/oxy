-- Tables for the `oltp-bookings` custom app (customer-apps/examples/oltp-bookings).
--
-- Applied in filename order after 0001. `IF NOT EXISTS` throughout because
-- apply is idempotent by ledger, but a crash between the DDL and its ledger
-- row can re-run one file.
--
-- Never edit this file once it has been applied anywhere — apply compares
-- checksums and refuses, since an edit would leave already-migrated tenants
-- permanently divergent. Add `0003_*.sql` instead.

CREATE SCHEMA IF NOT EXISTS app_bookings;

CREATE TABLE IF NOT EXISTS app_bookings.orders (
    id        BIGSERIAL PRIMARY KEY,
    table_no  INTEGER NOT NULL,
    status    TEXT NOT NULL DEFAULT 'open',
    placed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS app_bookings.order_items (
    id       BIGSERIAL PRIMARY KEY,
    order_id BIGINT NOT NULL REFERENCES app_bookings.orders(id),
    sku      TEXT NOT NULL,
    qty      INTEGER NOT NULL CHECK (qty > 0)
);

-- The CHECK is what makes overselling impossible even if application code is
-- wrong: `UPDATE … SET on_hand = on_hand - $2 WHERE on_hand >= $2` locks the
-- row for the transaction, and this constraint catches anything that slips past.
CREATE TABLE IF NOT EXISTS app_bookings.inventory (
    sku     TEXT PRIMARY KEY,
    on_hand INTEGER NOT NULL CHECK (on_hand >= 0)
);

CREATE INDEX IF NOT EXISTS idx_orders_open
    ON app_bookings.orders (status)
    WHERE status = 'open';

INSERT INTO app_bookings.inventory (sku, on_hand)
VALUES ('poke-bowl', 40), ('miso-soup', 90), ('green-tea', 200)
ON CONFLICT (sku) DO NOTHING;
