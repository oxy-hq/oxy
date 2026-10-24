//! The rows an Airhouse read returns, and how a reader takes them apart.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::usage_airhouse::SinkError;

/// The rows a read returned, as pgwire's text protocol delivers them: every
/// cell is text or NULL, so the readers ask for the type they expect and a
/// query selects timestamps as `epoch_us(…)`, never as a formatted string.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SqlRows {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
}

impl SqlRows {
    pub fn iter(&self) -> impl Iterator<Item = RowRef<'_>> {
        self.rows.iter().map(|cells| RowRef {
            columns: &self.columns,
            cells,
        })
    }
}

/// One row of [`SqlRows`], read by column name.
#[derive(Clone, Copy)]
pub struct RowRef<'a> {
    columns: &'a [String],
    cells: &'a [Option<String>],
}

impl RowRef<'_> {
    fn cell(&self, column: &str) -> Result<Option<&str>, SinkError> {
        let at = self
            .columns
            .iter()
            .position(|c| c == column)
            .ok_or_else(|| SinkError::Failed(format!("the result has no column `{column}`")))?;
        Ok(self.cells.get(at).and_then(|c| c.as_deref()))
    }

    fn bad(column: &str, want: &str, got: &str) -> SinkError {
        SinkError::Failed(format!(
            "column `{column}` is not {want}: {}",
            got.chars().take(200).collect::<String>()
        ))
    }

    pub fn opt_text(&self, column: &str) -> Result<Option<String>, SinkError> {
        Ok(self.cell(column)?.map(str::to_string))
    }

    pub fn text(&self, column: &str) -> Result<String, SinkError> {
        self.opt_text(column)?
            .ok_or_else(|| SinkError::Failed(format!("column `{column}` is NULL")))
    }

    pub fn int(&self, column: &str) -> Result<i64, SinkError> {
        let text = self.text(column)?;
        text.trim()
            .parse()
            .map_err(|_| Self::bad(column, "an integer", &text))
    }

    pub fn opt_int(&self, column: &str) -> Result<Option<i64>, SinkError> {
        match self.cell(column)? {
            None => Ok(None),
            Some(text) => text
                .trim()
                .parse()
                .map(Some)
                .map_err(|_| Self::bad(column, "an integer", text)),
        }
    }

    pub fn uuid(&self, column: &str) -> Result<Uuid, SinkError> {
        let text = self.text(column)?;
        Uuid::parse_str(text.trim()).map_err(|_| Self::bad(column, "a uuid", &text))
    }

    /// A timestamp selected as `epoch_us(…)`: microseconds since the epoch.
    pub fn ts(&self, column: &str) -> Result<DateTime<Utc>, SinkError> {
        let micros = self.int(column)?;
        DateTime::from_timestamp_micros(micros)
            .ok_or_else(|| Self::bad(column, "a timestamp", &micros.to_string()))
    }

    pub fn opt_ts(&self, column: &str) -> Result<Option<DateTime<Utc>>, SinkError> {
        match self.opt_int(column)? {
            None => Ok(None),
            Some(micros) => DateTime::from_timestamp_micros(micros)
                .map(Some)
                .ok_or_else(|| Self::bad(column, "a timestamp", &micros.to_string())),
        }
    }
}
