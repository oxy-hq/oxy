-- Demo rows for the live-app half of the story.
--
-- A NEW file rather than an edit to 0002, which is the rule this demo teaches:
-- apply compares checksums, so editing a shipped migration leaves every tenant
-- that already ran it permanently divergent. 0002 shipped with no orders, so
-- "how many orders are open right now?" answered from an empty table — true,
-- but it showed nothing.
--
-- Fixed ids with ON CONFLICT DO NOTHING so a re-apply is a no-op. `placed_at`
-- is relative to now() because a frozen timestamp makes "right now" questions
-- read as stale the day after they were written.

INSERT INTO app_bookings.orders (id, table_no, status, placed_at) VALUES
    (1,  4, 'open',   now() - interval '12 minutes'),
    (2,  7, 'open',   now() - interval '34 minutes'),
    (3, 11, 'open',   now() - interval '3 minutes'),
    (4,  2, 'closed', now() - interval '2 hours'),
    (5,  9, 'closed', now() - interval '4 hours')
ON CONFLICT (id) DO NOTHING;

INSERT INTO app_bookings.order_items (id, order_id, sku, qty) VALUES
    (1, 1, 'poke-bowl', 2),
    (2, 1, 'green-tea', 2),
    (3, 2, 'miso-soup', 1),
    (4, 3, 'poke-bowl', 1),
    (5, 4, 'poke-bowl', 3),
    (6, 5, 'miso-soup', 2)
ON CONFLICT (id) DO NOTHING;

-- BIGSERIAL keeps its own counter, which explicit ids do not advance. Without
-- this the app's first INSERT draws id 1 and fails the primary key.
SELECT setval(pg_get_serial_sequence('app_bookings.orders', 'id'),
              (SELECT max(id) FROM app_bookings.orders));
SELECT setval(pg_get_serial_sequence('app_bookings.order_items', 'id'),
              (SELECT max(id) FROM app_bookings.order_items));
