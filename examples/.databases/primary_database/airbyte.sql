CREATE TABLE `df-warehouse-prod.airbyte.typeform_forms__links`
(
  _airbyte_typeform_forms_hashid STRING,
  display STRING,
  _airbyte_emitted_at STRING,
  _airbyte__links_hashid STRING,
  _airbyte_typeform_forms_hashid_description STRING OPTIONS(description="Hash of the Typeform Forms table"),
  display_description STRING OPTIONS(description="Display name of the link"),
  _airbyte_emitted_at_description STRING OPTIONS(description="Timestamp of when the data was emitted"),
  _airbyte__links_hashid_description STRING OPTIONS(description="Hash of the _links table")
)
OPTIONS(
  description="Typeform form link"
);
