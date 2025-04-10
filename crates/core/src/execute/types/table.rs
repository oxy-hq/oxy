use std::{
    collections::HashMap,
    fmt::{self, Debug, Display, Formatter},
    hash::{Hash, Hasher},
    sync::Arc,
};

use arrow::{
    array::RecordBatch,
    datatypes::Schema,
    util::display::{ArrayFormatter, FormatOptions},
};
use minijinja::{
    Value,
    value::{Enumerator, Object, ObjectExt, ObjectRepr},
};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};

use crate::{
    adapters::connector::load_result,
    ai::utils::record_batches_to_2d_array,
    errors::OxyError,
    execute::agent::{AgentReference, SqlQueryReference},
    utils::truncate_datasets,
};

use super::utils::{record_batches_to_markdown, record_batches_to_table};

#[derive(Clone, Debug)]
struct ArrowTable {
    schema: Arc<Schema>,
    batches: Vec<RecordBatch>,
}

impl Display for ArrowTable {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match record_batches_to_table(&self.batches, &self.schema) {
            Ok(table) => write!(f, "{}", table),
            Err(e) => write!(f, "ArrowTable: {}", e),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TableReference {
    pub sql: String,
    pub database_ref: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Table {
    pub reference: Option<TableReference>,
    pub file_path: String,
    #[serde(skip)]
    inner: OnceCell<ArrowTable>,
}

impl Hash for Table {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.file_path.hash(state);
    }
}

impl Table {
    pub fn new(file_path: String) -> Self {
        Table {
            reference: None,
            file_path,
            inner: OnceCell::new(),
        }
    }

    pub fn with_reference(file_path: String, reference: TableReference) -> Self {
        Table {
            reference: Some(reference),
            file_path,
            inner: OnceCell::new(),
        }
    }

    fn get_inner(&self) -> Result<&ArrowTable, OxyError> {
        self.inner.get_or_try_init(|| {
            let file_path = self.file_path.clone();
            let (batches, schema) = load_result(&file_path).map_err(|_| {
                OxyError::RuntimeError("Executed query did not generate a valid output file. If you are using an agent to generate the query, consider giving it a shorter prompt.".to_string())
            })?;
            Ok(ArrowTable { schema, batches })
        })
    }

    pub fn to_markdown(&self) -> Result<String, OxyError> {
        let table = self.get_inner()?;
        Ok(record_batches_to_markdown(&table.batches, &table.schema)
            .map_err(|err| {
                OxyError::RuntimeError(format!("Failed to render table result:\n{}", err))
            })?
            .to_string())
    }

    pub fn to_term(&self) -> Result<String, OxyError> {
        let table = self.get_inner()?;
        Ok(record_batches_to_table(&table.batches, &table.schema)
            .map_err(|err| {
                OxyError::RuntimeError(format!("Failed to render table result:\n{}", err))
            })?
            .to_string())
    }

    pub fn into_reference(self) -> Option<AgentReference> {
        self.get_inner().ok()?;
        let table = self.inner.into_inner()?;
        let TableReference { sql, database_ref } = self.reference?;
        let (truncated_results, truncated) = truncate_datasets(table.batches);
        let formatted_results =
            record_batches_to_2d_array(&truncated_results, &table.schema).ok()?;
        Some(AgentReference::SqlQuery(SqlQueryReference {
            sql_query: sql,
            database: database_ref,
            result: formatted_results,
            is_result_truncated: truncated,
        }))
    }

    pub fn to_export(&self) -> Option<(String, &Arc<Schema>, &Vec<RecordBatch>)> {
        let table = self.get_inner().ok()?;
        let TableReference { sql, .. } = self.reference.clone()?;
        Some((sql, &table.schema, &table.batches))
    }
}

impl Debug for Table {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Table({:?})", &self.file_path)
    }
}

impl Display for Table {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.to_markdown() {
            Ok(inner) => {
                write!(f, "{}", inner)
            }
            Err(e) => write!(f, "Table({}): {}", &self.file_path, e),
        }
    }
}

impl Object for Table {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Iterable
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let table = self.get_inner().ok()?;
        let schema = table.schema.clone();
        let (idx, _field) = schema.column_with_name(key.as_str()?)?;
        let mut values = Vec::new();
        for batch in &table.batches {
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
        let table = match self.get_inner() {
            Ok(inner) => inner,
            Err(_) => return Enumerator::Empty,
        };
        let mut values = vec![];
        let schema = table.schema.clone();
        let options = FormatOptions::default().with_display_error(true);
        for batch in &table.batches {
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
