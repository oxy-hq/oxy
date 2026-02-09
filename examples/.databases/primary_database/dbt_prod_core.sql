CREATE TABLE `df-warehouse-prod.dbt_prod_core.dim_document_stats`
(
  id_document STRING,
  id_creator STRING,
  reads INT64,
  reads_non_creator INT64,
  readers INT64,
  readers_non_creator INT64,
  executions INT64,
  executions_non_creator INT64
);
CREATE TABLE `df-warehouse-prod.dbt_prod_core.fct_typeform_answers`
(
  id_form STRING,
  id_field STRING,
  id_response STRING,
  email STRING,
  full_name STRING,
  company_name STRING,
  primary_role STRING,
  company_size STRING,
  sql_or_python STRING,
  challenges_to_solve STRUCT<id_response STRING, challenges_to_solve STRING>,
  field_text STRING,
  field_type STRING,
  answer STRING,
  ts_submitted_at TIMESTAMP
);
CREATE TABLE `df-warehouse-prod.dbt_prod_core.fct_table_searches`
(
  id_search STRING,
  id_user STRING,
  filters_score INT64,
  table_admin_quality_score INT64,
  filters_tag STRING,
  term STRING,
  pagination_from INT64,
  pagination_size INT64,
  ts TIMESTAMP,
  ds DATE
);
CREATE TABLE `df-warehouse-prod.dbt_prod_core.dim_user_document_stats`
(
  id_user STRING,
  reads INT64,
  reads_non_creator INT64,
  readers INT64,
  readers_non_creator INT64,
  executions INT64,
  executions_non_creator INT64
);
CREATE TABLE `df-warehouse-prod.dbt_prod_core.dim_recent_user_activity`
(
  id_user STRING,
  email STRING,
  name STRING,
  organization_subdomain STRING,
  ds_latest_visit DATE
);
CREATE VIEW `df-warehouse-prod.dbt_prod_core.dim_typeform_private_beta_invites`
AS with cte as (
  select
    requests.email,
    array_agg(requests.id_form) as id_forms_completed,
    array_concat_agg(requests.answers) as all_answers,
    max(requests.is_lifetime_purchase) as has_lifetime_purchase,
    sum(if(invites.ts is not null, 1, 0)) n_invites_sent,
    max(invites.ts) as ts_latest_invite_sent,
    min(requests.ts_submitted_at) as ts_first_signup,
    max(requests.ts_submitted_at) as ts_latest_signup,
  from `df-warehouse-prod`.`dbt_prod_core`.`fct_typeform_private_beta_requests` requests
  left join `df-warehouse-prod`.`retool`.`fct_invites_sent` invites
    on invites.email = requests.email
  group by 1
),

reordered_array as (
  select
    email,
    id_forms_completed,
    -- order appears to be stochastic, so re-order by field name instead
    array(select x from unnest(all_answers) as x order by x.field_text) as all_answers,
    has_lifetime_purchase,
    n_invites_sent,
    ts_latest_invite_sent,
    ts_first_signup,
    ts_latest_signup,
  from cte
)
select
  *
from reordered_array;
CREATE TABLE `df-warehouse-prod.dbt_prod_core.fct_typeform_responses`
(
  id_form STRING,
  id_response STRING,
  email STRING,
  full_name STRING,
  company_name STRING,
  primary_role STRING,
  company_size STRING,
  sql_or_python STRING,
  ts_submitted_at TIMESTAMP,
  answers ARRAY<STRUCT<field_text STRING, answer STRING>>
);
CREATE TABLE `df-warehouse-prod.dbt_prod_core.fct_typeform_private_beta_requests`
(
  id_form STRING,
  id_response STRING,
  email STRING,
  full_name STRING,
  company_name STRING,
  primary_role STRING,
  company_size STRING,
  sql_or_python STRING,
  ts_submitted_at TIMESTAMP,
  answers ARRAY<STRUCT<field_text STRING, answer STRING>>,
  is_lifetime_purchase BOOL
);
CREATE TABLE `df-warehouse-prod.dbt_prod_core.fct_typeform_leads`
(
  id_form STRING,
  id_response STRING,
  email STRING,
  full_name STRING,
  company_name STRING,
  primary_role STRING,
  company_size STRING,
  sql_or_python STRING,
  ts_submitted_at TIMESTAMP,
  answers ARRAY<STRUCT<field_text STRING, answer STRING>>,
  is_lifetime_purchase BOOL,
  is_demo_request BOOL
);
CREATE TABLE `df-warehouse-prod.dbt_prod_core.dim_typeform_leads`
(
  email STRING,
  full_name STRING,
  company_name STRING,
  primary_role STRING,
  company_size STRING,
  sql_or_python STRING,
  id_forms_completed ARRAY<STRING>,
  all_answers ARRAY<STRUCT<field_text STRING, answer STRING>>,
  has_lifetime_purchase BOOL,
  ts_first_signup TIMESTAMP,
  ts_latest_signup TIMESTAMP
);
CREATE TABLE `df-warehouse-prod.dbt_prod_core.dim_users_for_reverse_etl`
(
  id_user STRING,
  email STRING,
  name STRING,
  first_name STRING,
  id_organization STRING,
  organization_subdomain STRING,
  ts_first_active TIMESTAMP,
  ts_last_active TIMESTAMP
);
CREATE TABLE `df-warehouse-prod.dbt_prod_core.dim_searchers`
(
  ds DATE,
  id_user STRING,
  organization_subdomain STRING,
  term STRING
);
CREATE TABLE `df-warehouse-prod.dbt_prod_core.fct_user_events_active`
(
  id_user STRING,
  id_event STRING,
  event_type STRING,
  organization_subdomain STRING,
  ts TIMESTAMP,
  ds DATE
);
CREATE TABLE `df-warehouse-prod.dbt_prod_core.dim_users`
(
  id_user STRING,
  email STRING,
  name STRING,
  id_organization STRING,
  organization_subdomain STRING,
  ts_first_event TIMESTAMP
);
CREATE TABLE `df-warehouse-prod.dbt_prod_core.dim_typeform_prequel_beta_signup_2`
(
  email STRING,
  full_name STRING,
  company_name STRING,
  favorite_features STRING,
  primary_role STRING,
  company_size STRING,
  warehouses STRING,
  id_response STRING,
  ts TIMESTAMP,
  ds DATE,
  invite_sent BOOL
);
CREATE TABLE `df-warehouse-prod.dbt_prod_core.fct_user_events`
(
  id_user STRING,
  id_event STRING,
  event_type STRING,
  organization_subdomain STRING,
  ts TIMESTAMP,
  ds DATE
);
CREATE TABLE `df-warehouse-prod.dbt_prod_core.dim_organizations`
(
  organization_subdomain STRING,
  ts_earliest_action TIMESTAMP
);
CREATE TABLE `df-warehouse-prod.dbt_prod_core.dim_organizations_with_warehouse`
(
  organization_subdomain STRING,
  ts_earliest_warehouse_added TIMESTAMP
);
