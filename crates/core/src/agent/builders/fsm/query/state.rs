use crate::{
    agent::builders::fsm::query::auto_sql::PrepareData,
    execute::types::{Output, OutputContainer, Table},
};

#[derive(Debug)]
pub struct Dataset {
    tables: Vec<Table>,
}

impl Dataset {
    pub fn new() -> Self {
        Self { tables: vec![] }
    }
}

impl PrepareData for Dataset {
    fn add_table(&mut self, table: Table) {
        self.tables.push(table);
    }

    fn get_tables(&self) -> &[Table] {
        &self.tables
    }
}

impl From<Dataset> for OutputContainer {
    fn from(val: Dataset) -> Self {
        OutputContainer::List(
            val.tables
                .into_iter()
                .map(|t| {
                    let output: Output = t.into();
                    output.into()
                })
                .collect(),
        )
    }
}
