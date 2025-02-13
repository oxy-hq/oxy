use arrow::{
    array::RecordBatch,
    util::{
        display::{ArrayFormatter, FormatOptions},
        pretty::pretty_format_batches,
    },
};
use base64::{self, engine::general_purpose, Engine};
use minijinja::value::{Enumerator, Object, ObjectExt, ObjectRepr, Value};
use serde::Deserialize;
use std::fmt::{self, Display};
use std::{collections::HashMap, fmt::Debug, sync::Arc};

use arrow::ipc::reader::StreamReader;
use arrow::ipc::writer::StreamWriter;
use serde::Serialize;
use std::io::Cursor;

#[derive(Clone)]
pub struct ArrowTable(pub Vec<RecordBatch>);

impl Serialize for ArrowTable {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut buffer = Vec::new();
        let schema = self.0.first().map(|batch| batch.schema()).ok_or_else(|| {
            serde::ser::Error::custom("Cannot serialize: ArrowTable has no RecordBatches")
        })?;

        // Serialize the RecordBatches to Arrow IPC format
        {
            let mut writer = StreamWriter::try_new(&mut buffer, &schema).map_err(|e| {
                serde::ser::Error::custom(format!("Arrow StreamWriter error: {}", e))
            })?;
            for batch in &self.0 {
                writer.write(batch).map_err(|e| {
                    serde::ser::Error::custom(format!("Failed to write batch: {}", e))
                })?;
            }
            writer.finish().map_err(|e| {
                serde::ser::Error::custom(format!("Failed to finish writing: {}", e))
            })?;
        }

        // Serialize the IPC buffer as a Base64 string
        let encoded = general_purpose::STANDARD.encode(buffer);
        serializer.serialize_str(&encoded)
    }
}

impl<'de> Deserialize<'de> for ArrowTable {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let encoded: String = Deserialize::deserialize(deserializer)?;

        // Decode the Base64 string to bytes
        let bytes = general_purpose::STANDARD
            .decode(&encoded)
            .map_err(|e| serde::de::Error::custom(format!("Base64 decode error: {}", e)))?;

        // Deserialize the bytes into RecordBatches using Arrow StreamReader
        let cursor = Cursor::new(bytes);
        let reader = StreamReader::try_new(cursor, None)
            .map_err(|e| serde::de::Error::custom(format!("Arrow StreamReader error: {}", e)))?;

        let mut batches = Vec::new();
        for batch in reader {
            batches.push(batch.map_err(|e| {
                serde::de::Error::custom(format!("Failed to read RecordBatch: {}", e))
            })?);
        }

        Ok(ArrowTable(batches))
    }
}

impl Debug for ArrowTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let result = pretty_format_batches(&self.0).map_err(|e| {
            log::error!("Error formatting ArrowTable: {:?}", e);
            fmt::Error
        })?;
        result.fmt(f)
    }
}

impl ArrowTable {
    pub fn new(batches: Vec<RecordBatch>) -> Self {
        ArrowTable(batches)
    }
}

impl IntoIterator for ArrowTable {
    type Item = RecordBatch;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Object for ArrowTable {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Iterable
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        if self.0.is_empty() {
            return None;
        }
        let schema = self.0[0].schema();
        let (idx, _field) = schema.column_with_name(key.as_str()?)?;
        let mut values = Vec::new();
        for batch in &self.0 {
            let array = batch.column(idx);
            let formatter = arrow::util::display::ArrayFormatter::try_new(
                array,
                &arrow::util::display::FormatOptions::default(),
            )
            .ok()?;
            for idx in 0..batch.num_rows() {
                values.push(Value::from(formatter.value(idx).to_string()));
            }
        }
        log::info!("ArrowTable.{} Values: {:?}", key, values);
        Some(Value::from(values))
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        if self.0.is_empty() {
            return Enumerator::Empty;
        }
        let mut values = vec![];
        let schema = self.0[0].schema();
        let options = FormatOptions::default().with_display_error(true);
        for batch in &self.0 {
            let formatters = batch
                .columns()
                .iter()
                .map(|c| ArrayFormatter::try_new(c.as_ref(), &options).unwrap())
                .collect::<Vec<_>>();

            for row in 0..batch.num_rows() {
                let mut cells = HashMap::new();
                for (idx, formatter) in formatters.iter().enumerate() {
                    cells.insert(
                        schema.field(idx).name().to_string(),
                        Value::from(formatter.value(row).to_string()),
                    );
                }
                values.push(Value::from(cells));
            }
        }
        Enumerator::Values(values)
    }

    fn render(self: &Arc<Self>, f: &mut fmt::Formatter<'_>) -> fmt::Result
    where
        Self: Sized + 'static,
    {
        match self.repr() {
            ObjectRepr::Seq | ObjectRepr::Iterable if self.enumerator_len().is_some() => {
                for value in self.try_iter().into_iter().flatten() {
                    let _ = &std::fmt::Debug::fmt(&value, f);
                    f.write_str("\n")?;
                }
                f.write_str("")
            }
            _ => {
                write!(f, "{self:?}")
            }
        }
    }
}
