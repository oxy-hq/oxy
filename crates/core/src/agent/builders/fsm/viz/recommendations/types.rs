use arrow::datatypes::DataType;
use serde::{Deserialize, Serialize};

use crate::{
    config::model::{BarChartDisplay, LineChartDisplay, PieChartDisplay},
    execute::types::VizParams,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChartDisplay {
    Line(LineChartDisplay),
    Bar(BarChartDisplay),
    Pie(PieChartDisplay),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChartWithDescription {
    pub description: String,
    #[serde(flatten)]
    pub display: ChartDisplay,
}

impl std::fmt::Display for ChartDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChartDisplay::Line(opts) => {
                write!(f, "Line: x={}, y={}, data={}", opts.x, opts.y, opts.data)?;
                if let Some(ref s) = opts.series {
                    write!(f, ", series={}", s)?;
                }
                Ok(())
            }
            ChartDisplay::Bar(opts) => {
                write!(f, "Bar: x={}, y={}, data={}", opts.x, opts.y, opts.data)?;
                if let Some(ref s) = opts.series {
                    write!(f, ", series={}", s)?;
                }
                Ok(())
            }
            ChartDisplay::Pie(opts) => {
                write!(
                    f,
                    "Pie: name={}, value={}, data={}",
                    opts.name, opts.value, opts.data
                )
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChartType {
    Line,
    Bar,
    Pie,
}

#[derive(Debug, Clone)]
pub struct ChartRecommendation {
    pub chart_type: ChartType,
    pub score: f64,
    pub rationale: String,
    pub display: ChartDisplay,
}

impl std::fmt::Display for ChartRecommendation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{:?}] Score: {:.2} - {}",
            self.chart_type, self.score, self.rationale
        )
    }
}

#[derive(Debug, Clone)]
pub enum ColumnKind {
    Numeric {
        is_integer: bool,
        has_negatives: bool,
        min: Option<f64>,
        max: Option<f64>,
    },
    Temporal,
    Categorical {
        cardinality: usize,
    },
    Text,
    Boolean,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub arrow_type: DataType,
    pub kind: ColumnKind,
    pub null_count: usize,
    pub row_count: usize,
    pub unique_count: usize,
}

impl ColumnInfo {
    pub fn is_numeric(&self) -> bool {
        matches!(self.kind, ColumnKind::Numeric { .. })
    }

    pub fn is_temporal(&self) -> bool {
        matches!(self.kind, ColumnKind::Temporal)
    }

    pub fn is_categorical(&self) -> bool {
        matches!(self.kind, ColumnKind::Categorical { .. })
    }

    pub fn is_boolean(&self) -> bool {
        matches!(self.kind, ColumnKind::Boolean)
    }

    pub fn cardinality(&self) -> Option<usize> {
        match &self.kind {
            ColumnKind::Categorical { cardinality } => Some(*cardinality),
            ColumnKind::Boolean => Some(2),
            _ => None,
        }
    }

    pub fn all_positive(&self) -> bool {
        match &self.kind {
            ColumnKind::Numeric {
                has_negatives, min, ..
            } => !has_negatives && min.map_or(true, |m| m >= 0.0),
            _ => false,
        }
    }
}

/// Context provided to chart generators with all analyzed data
#[derive(Debug, Clone)]
pub struct AnalysisContext {
    pub columns: Vec<ColumnInfo>,
    pub row_count: usize,
    pub data_reference: String,
}

impl AnalysisContext {
    pub fn numeric_columns(&self) -> Vec<&ColumnInfo> {
        self.columns.iter().filter(|c| c.is_numeric()).collect()
    }

    pub fn temporal_columns(&self) -> Vec<&ColumnInfo> {
        self.columns.iter().filter(|c| c.is_temporal()).collect()
    }

    pub fn categorical_columns(&self) -> Vec<&ColumnInfo> {
        self.columns.iter().filter(|c| c.is_categorical()).collect()
    }

    pub fn boolean_columns(&self) -> Vec<&ColumnInfo> {
        self.columns.iter().filter(|c| c.is_boolean()).collect()
    }
}

/// Context for scoring decisions
#[derive(Debug, Clone)]
pub struct ScoringContext {
    pub row_count: usize,
    pub cardinality: Option<usize>,
    pub has_temporal: bool,
    pub has_grouping: bool,
    pub num_series: usize,
}
