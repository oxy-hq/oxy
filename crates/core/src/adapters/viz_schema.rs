pub const VIZ_SCHEMA: &str = r##"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "VisualizeParams",
  "type": "object",
  "properties": {
    "xAxis": {
      "type": "object",
      "description": "X-axis configuration for controlling scale type, labels, and category data. Not applicable for pie charts",
      "properties": {
        "type": {
          "type": "string",
          "description": "Type of axis scale: 'category' for categorical data, 'value' for numerical data, 'time' for time series, 'log' for logarithmic scale"
        },
        "name": {
          "type": "string",
          "description": "Display name for the axis, shown as axis label"
        },
        "data": {
          "type": "array",
          "description": "Category data array for category axis type (e.g., ['Mon', 'Tue', 'Wed']). Not needed for value/time/log axis types",
          "items": {}
        }
      },
      "required": ["type"]
    },
    "yAxis": {
      "type": "object",
      "description": "Y-axis configuration for controlling scale type, labels, and formatting. Not applicable for pie charts",
      "properties": {
        "type": {
          "type": "string",
          "description": "Type of axis scale: 'category' for categorical data, 'value' for numerical data, 'time' for time series, 'log' for logarithmic scale"
        },
        "name": {
          "type": "string",
          "description": "Display name for the axis, shown as axis label"
        },
        "data": {
          "type": "array",
          "description": "Category data array for category axis type (e.g., ['Mon', 'Tue', 'Wed']). Not needed for value/time/log axis types",
          "items": {}
        }
      },
      "required": ["type"]
    },
    "series": {
      "type": "array",
      "description": "Array of data series defining chart content and chart type. Each series represents a dataset to be visualized",
      "items": {
        "type": "object",
        "properties": {
          "name": {
            "type": "string",
            "description": "Display name for the series, shown in legend and tooltips"
          },
          "type": {
            "type": "string",
            "enum": ["line", "bar", "pie"],
            "description": "Chart type: 'line' for line charts, 'bar' for bar charts, 'pie' for pie charts"
          },
          "data": {
            "type": "array",
            "description": "Data array for the series. Format depends on chart type: simple array for line/bar charts, or array of objects with name/value for pie charts",
            "items": {}
          }
        },
        "required": ["type"]
      }
    },
    "title": {
      "type": "string",
      "description": "Chart title"
    }
  },
  "required": ["series"]
}"##;
