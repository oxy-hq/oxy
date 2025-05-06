pub const CREATE_APP_SCHEMA: &str = r##"{
    "$schema": "http://json-schema.org/draft-07/schema#",
    "title": "CreateDataAppParams",
    "type": "object",
    "required": [
      "app_config",
      "file_name"
    ],
    "properties": {
      "app_config": {
        "description": "The data app config",
        "allOf": [
          {
            "$ref": "#/definitions/AppConfig"
          }
        ]
      },
      "file_name": {
        "description": "The file name of the data app file without the extension",
        "type": "string"
      }
    },
    "definitions": {
      "AppConfig": {
        "type": "object",
        "required": [
          "display",
          "tasks"
        ],
        "properties": {
          "display": {
            "description": "display blocks to render the app",
            "type": "array",
            "items": {
              "$ref": "#/definitions/Display"
            }
          },
          "tasks": {
            "description": "tasks to prepare the data for the app",
            "type": "array",
            "items": {
              "$ref": "#/definitions/Task"
            }
          }
        }
      },
      "Condition": {
        "type": "object",
        "required": [
          "if",
          "tasks"
        ],
        "properties": {
          "if": {
            "type": "string"
          },
          "tasks": {
            "type": "array",
            "items": {
              "$ref": "#/definitions/Task"
            }
          }
        }
      },
      "Display": {
        "oneOf": [
          {
            "type": "object",
            "required": [
              "content",
              "type"
            ],
            "properties": {
              "content": {
                "type": "string"
              },
              "type": {
                "type": "string",
                "enum": [
                  "markdown"
                ]
              }
            }
          },
          {
            "type": "object",
            "required": [
              "data",
              "type",
              "x",
              "y"
            ],
            "properties": {
              "data": {
                "description": "reference data output from a task using task name",
                "type": "string"
              },
              "series": {
                "type": "string",
                "nullable": true
              },
              "title": {
                "type": "string",
                "nullable": true
              },
              "type": {
                "type": "string",
                "enum": [
                  "line_chart"
                ]
              },
              "x": {
                "type": "string"
              },
              "x_axis_label": {
                "type": "string",
                "nullable": true
              },
              "y": {
                "type": "string"
              },
              "y_axis_label": {
                "type": "string",
                "nullable": true
              }
            }
          },
          {
            "type": "object",
            "required": [
              "data",
              "name",
              "type",
              "value"
            ],
            "properties": {
              "data": {
                "type": "string"
              },
              "name": {
                "type": "string",
                "description": "name of the column to be used as the label for the pie chart"
              },
              "title": {
                "type": "string",
                "nullable": true
              },
              "type": {
                "type": "string",
                "enum": [
                  "pie_chart"
                ]
              },
              "value": {
                "type": "string",
                "description": "name of the column to be used as the value for the pie chart"
              }
            }
          },
          {
            "type": "object",
            "required": [
              "data",
              "type",
              "x",
              "y"
            ],
            "properties": {
              "data": {
                "type": "string"
              },
              "series": {
                "type": "string",
                "nullable": true
              },
              "title": {
                "type": "string",
                "nullable": true
              },
              "type": {
                "type": "string",
                "enum": [
                  "bar_chart"
                ]
              },
              "x": {
                "type": "string"
              },
              "y": {
                "type": "string"
              }
            }
          },
          {
            "type": "object",
            "required": [
              "data",
              "type"
            ],
            "properties": {
              "data": {
                "type": "string"
              },
              "title": {
                "type": "string",
                "nullable": true
              },
              "type": {
                "type": "string",
                "enum": [
                  "table"
                ]
              }
            }
          }
        ]
      },
      "ExportFormat": {
        "type": "string",
        "enum": [
          "sql",
          "csv",
          "json",
          "txt",
          "docx"
        ]
      },
      "LoopValues": {
        "anyOf": [
          {
            "type": "string"
          },
          {
            "type": "array",
            "items": {
              "type": "string"
            }
          }
        ]
      },
      "Task": {
        "type": "object",
        "oneOf": [
          {
            "type": "object",
            "required": [
              "database",
              "name",
              "type",
              "sql_query"
            ],
            "properties": {
              "name": {
                "type": "string"
              },
              "cache": {
                "allOf": [
                  {
                    "$ref": "#/definitions/TaskCache"
                  }
                ],
                "nullable": true
              },
              "database": {
                "type": "string"
              },
              "sql_file": {
                "type": "string"
              },
              "sql_query": {
                "type": "string"
              },
              "dry_run_limit": {
                "type": "integer",
                "format": "uint64",
                "minimum": 0,
                "nullable": true
              },
              "export": {
                "allOf": [
                  {
                    "$ref": "#/definitions/TaskExport"
                  }
                ],
                "nullable": true
              },
              "type": {
                "type": "string",
                "enum": [
                  "execute_sql"
                ]
              },
              "variables": {
                "default": null,
                "type": "object",
                "additionalProperties": {
                  "type": "string"
                },
                "nullable": true
              }
            }
          }
      ]
      },
      "TaskCache": {
        "type": "object",
        "required": [
          "path"
        ],
        "properties": {
          "enabled": {
            "default": false,
            "type": "boolean"
          },
          "path": {
            "type": "string"
          }
        }
      },
      "TaskExport": {
        "type": "object",
        "required": [
          "format",
          "path"
        ],
        "properties": {
          "format": {
            "$ref": "#/definitions/ExportFormat"
          },
          "path": {
            "type": "string"
          }
        }
      }
    }
  }"##;
