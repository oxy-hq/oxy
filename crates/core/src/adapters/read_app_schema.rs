pub const READ_APP_SCHEMA: &str = r##"{
    "$schema": "http://json-schema.org/draft-07/schema#",
    "title": "ReadDataAppParams",
    "type": "object",
    "required": [],
    "properties": {
      "file_path": {
        "description": "The relative path to the existing data app file (e.g. my_dashboard.app.yml). Optional if a data app is already associated with the current thread.",
        "type": "string"
      }
    }
  }"##;
