use arrow::{
    array::RecordBatch,
    util::display::{ArrayFormatter, FormatOptions},
};
use minijinja::value::{Enumerator, Object, ObjectExt, ObjectRepr, Value};
use std::fmt;
use std::{collections::HashMap, fmt::Debug, sync::Arc};

#[derive(Debug, Clone)]
pub struct J2Table(Vec<RecordBatch>);

impl J2Table {
    pub fn new(batches: Vec<RecordBatch>) -> Self {
        J2Table(batches)
    }
}

impl IntoIterator for J2Table {
    type Item = RecordBatch;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Object for J2Table {
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
            ObjectRepr::Map => {
                let mut dbg = f.debug_map();
                for (key, value) in self.try_iter_pairs().into_iter().flatten() {
                    dbg.entry(&key, &value);
                }
                dbg.finish()
            }
            // for either sequences or iterables, a length is needed, otherwise we
            // don't want to risk iteration during printing and fall back to the
            // debug print.
            ObjectRepr::Seq | ObjectRepr::Iterable if self.enumerator_len().is_some() => {
                for value in self.try_iter().into_iter().flatten() {
                    let _ = &value.fmt(f);
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
