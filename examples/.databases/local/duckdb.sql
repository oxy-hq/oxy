-- monthly_revenue_by_region.csv
CREATE TABLE 'monthly_revenue_by_region.csv' ("month" VARCHAR, "region" VARCHAR, "revenue" DOUBLE);

-- fruit.csv
CREATE TABLE 'fruit.csv' ("id" BIGINT, "name" VARCHAR, "price" BIGINT);

-- orders.csv
CREATE TABLE 'orders.csv' ("id" BIGINT, "customer_id" BIGINT, "shipping_address_id" BIGINT, "order_date" DATE, "status" VARCHAR, "total_amount" DOUBLE, "shipping_cost" DOUBLE, "tax_amount" DOUBLE);

-- order_shipments.csv
CREATE TABLE 'order_shipments.csv' ("shipment_id" BIGINT, "order_id" BIGINT, "line_item_id" BIGINT, "tracking_number" VARCHAR, "carrier" VARCHAR, "shipped_date" DATE, "estimated_delivery" DATE, "actual_delivery" DATE, "status" VARCHAR);

-- monthly_active_user_by_platform.csv
CREATE TABLE 'monthly_active_user_by_platform.csv' ("month" VARCHAR, "platform" VARCHAR, "users" BIGINT);

-- users.csv
CREATE TABLE 'users.csv' ("user_id" VARCHAR, "user_name" VARCHAR);

-- shipping_addresses.csv
CREATE TABLE 'shipping_addresses.csv' ("id" BIGINT, "customer_id" BIGINT, "address_line_1" VARCHAR, "address_line_2" VARCHAR, "city" VARCHAR, "state" VARCHAR, "postal_code" VARCHAR, "country" VARCHAR, "is_default" BOOLEAN);

-- customer.csv
CREATE TABLE 'customer.csv' ("id" BIGINT, "name" VARCHAR, "gender" VARCHAR);

-- fruit_supplier_relationships.csv
CREATE TABLE 'fruit_supplier_relationships.csv' ("id" BIGINT, "fruit_id" BIGINT, "supplier_id" BIGINT, "wholesale_price" DOUBLE, "min_order_qty" BIGINT, "lead_time_days" BIGINT, "is_primary_supplier" BOOLEAN);

-- content_level_monthly_stats_fruits_veggies.csv
CREATE TABLE 'content_level_monthly_stats_fruits_veggies.csv' ("month" VARCHAR, "property_grouping" VARCHAR, "property" VARCHAR, "content_type" VARCHAR, "content_id" BIGINT, "views" VARCHAR, "minutes" VARCHAR);

-- fruit_suppliers.csv
CREATE TABLE 'fruit_suppliers.csv' ("id" BIGINT, "name" VARCHAR, "contact_email" VARCHAR, "contact_phone" VARCHAR, "country" VARCHAR, "rating" DOUBLE);

-- fruit_sales.csv
CREATE TABLE 'fruit_sales.csv' ("id" BIGINT, "fruit_id" BIGINT, "customer_id" BIGINT, "quantity" BIGINT, "amount" BIGINT, "date" DATE);

-- order_items.csv
CREATE TABLE 'order_items.csv' ("order_id" BIGINT, "line_item_id" BIGINT, "product_id" BIGINT, "quantity" BIGINT, "unit_price" BIGINT, "discount_percent" BIGINT);

-- order_returns.csv
CREATE TABLE 'order_returns.csv' ("return_id" BIGINT, "order_id" BIGINT, "line_item_id" BIGINT, "return_date" DATE, "reason" VARCHAR, "quantity" BIGINT, "refund_amount" DOUBLE, "status" VARCHAR);