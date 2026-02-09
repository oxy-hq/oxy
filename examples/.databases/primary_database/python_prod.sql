CREATE TABLE `df-warehouse-prod.python_prod.v1_tables_popular_tables`
(
  context_library_name STRING,
  context_library_version STRING,
  event STRING,
  event_text STRING,
  id STRING,
  loaded_at TIMESTAMP,
  original_timestamp TIMESTAMP,
  received_at TIMESTAMP,
  sent_at TIMESTAMP,
  timestamp TIMESTAMP,
  user_id STRING,
  uuid_ts TIMESTAMP
)
PARTITION BY DATE(_PARTITIONTIME);
CREATE TABLE `df-warehouse-prod.python_prod.v1_organizations`
(
  context_library_name STRING,
  context_library_version STRING,
  event STRING,
  event_text STRING,
  id STRING,
  loaded_at TIMESTAMP,
  original_timestamp TIMESTAMP,
  received_at TIMESTAMP,
  sent_at TIMESTAMP,
  timestamp TIMESTAMP,
  user_id STRING,
  uuid_ts TIMESTAMP
)
PARTITION BY DATE(_PARTITIONTIME);
CREATE TABLE `df-warehouse-prod.python_prod.v1_search_table`
(
  context_library_name STRING,
  context_library_version STRING,
  event STRING,
  event_text STRING,
  id STRING,
  loaded_at TIMESTAMP,
  original_timestamp TIMESTAMP,
  received_at TIMESTAMP,
  sent_at TIMESTAMP,
  timestamp TIMESTAMP,
  user_id STRING,
  uuid_ts TIMESTAMP
)
PARTITION BY DATE(_PARTITIONTIME);
CREATE TABLE `df-warehouse-prod.python_prod.v1_users`
(
  context_library_name STRING,
  context_library_version STRING,
  event STRING,
  event_text STRING,
  id STRING,
  loaded_at TIMESTAMP,
  original_timestamp TIMESTAMP,
  received_at TIMESTAMP,
  sent_at TIMESTAMP,
  timestamp TIMESTAMP,
  user_id STRING,
  uuid_ts TIMESTAMP
)
PARTITION BY DATE(_PARTITIONTIME);
