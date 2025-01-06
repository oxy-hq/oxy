use minijinja::{context, value::Kwargs, Value};

use crate::errors::OnyxError;

use super::{
    value::{Array, ContextValue, Map},
    Executable, ExecutionContext, Write,
};

#[derive(Debug, Default)]
struct MapAdapterState {
    result: Map,
}

struct MapAdapter<'key, 'state: 'key> {
    key: &'key str,
    state: &'state mut MapAdapterState,
}

impl<'key, 'state: 'key> MapAdapter<'key, 'state> {
    fn wrap<'slot, 'context: 'state + 'slot>(
        execution_context: &'context mut ExecutionContext<'_>,
        slot: &'slot mut Option<Self>,
        map_state: &'state mut MapAdapterState,
        key: &'key str,
    ) -> ExecutionContext<'slot> {
        let current = context! {
          ..Value::from_serialize(execution_context.context.clone()),
          ..Value::from_object(map_state.result.to_owned()),
        };
        log::info!(
            "MapAdapter.wrap with context: {:?}, Current result: {:?}",
            current,
            map_state.result
        );
        execution_context.wrap(
            move |_writer| {
                slot.insert(MapAdapter {
                    state: map_state,
                    key,
                })
            },
            Some(key.to_string()),
            current,
        )
    }
}

impl Write for MapAdapter<'_, '_> {
    fn write(&mut self, value: ContextValue) {
        log::info!(
            "MapAdapter.write to key `{}` with value: {:?}",
            self.key,
            value
        );
        self.state.result.set_value(&self.key, value);
    }
}

pub struct MapExecutor<'context, 'writer: 'context> {
    pub execution_context: &'context mut ExecutionContext<'writer>,
    map_state: MapAdapterState,
}

impl<'context, 'writer: 'context> MapExecutor<'context, 'writer> {
    pub fn new(execution_context: &'context mut ExecutionContext<'writer>) -> Self {
        Self {
            execution_context: execution_context,
            map_state: Default::default(),
        }
    }

    pub async fn entries<E, I>(&mut self, entries: I) -> Result<(), OnyxError>
    where
        E: Executable,
        I: IntoIterator<Item = (String, E)>,
    {
        for (key, entry) in entries {
            self.entry(&key, &entry).await?;
        }
        Ok(())
    }

    pub async fn entry(&mut self, key: &str, entry: &dyn Executable) -> Result<(), OnyxError> {
        let mut slot = None;
        let mut state =
            MapAdapter::wrap(self.execution_context, &mut slot, &mut self.map_state, key);
        entry.execute(&mut state).await?;
        Ok(())
    }

    pub fn finish(&mut self) {
        self.execution_context
            .write(ContextValue::Map(self.map_state.result.to_owned()));
    }
}

#[derive(Debug, Default)]
pub struct LoopAdapterState {
    result: Vec<ContextValue>,
}

pub struct LoopAdapter<'state> {
    state: &'state mut LoopAdapterState,
}

impl<'state> LoopAdapter<'state> {
    fn wrap<'slot, 'context: 'state + 'slot>(
        execution_context: &'context mut ExecutionContext<'_>,
        slot: &'slot mut Option<Self>,
        loop_state: &'state mut LoopAdapterState,
        input: &ContextValue,
    ) -> ExecutionContext<'slot> {
        let name = &execution_context.key.clone().unwrap_or_default();
        let current = context! {
          ..Value::from_serialize(&execution_context.context),
          ..Value::from(Kwargs::from_iter([
            (name.to_string(), Value::from(Kwargs::from_iter([
              ("value", input.clone().into())
            ]))),
          ])),
        };
        log::info!("LoopAdapter.wrap with context: {:?}", current);
        execution_context.wrap(
            move |_writer| slot.insert(LoopAdapter { state: loop_state }),
            Some(name.to_string()),
            current,
        )
    }
}

impl Write for LoopAdapter<'_> {
    fn write(&mut self, value: ContextValue) {
        self.state.result.push(value);
    }
}

pub struct LoopExecutor<'context, 'writer: 'context> {
    execution_context: &'context mut ExecutionContext<'writer>,
    loop_state: LoopAdapterState,
}

impl<'context, 'writer: 'context> LoopExecutor<'context, 'writer> {
    pub fn new(execution_context: &'context mut ExecutionContext<'writer>) -> Self {
        Self {
            execution_context,
            loop_state: LoopAdapterState {
                result: Default::default(),
            },
        }
    }

    pub async fn params(
        &mut self,
        params: &Vec<ContextValue>,
        entry: &dyn Executable,
    ) -> Result<(), OnyxError> {
        for param in params {
            let mut slot = None;
            let mut loop_context = LoopAdapter::wrap(
                self.execution_context,
                &mut slot,
                &mut self.loop_state,
                param,
            );
            entry.execute(&mut loop_context).await?;
        }
        Ok(())
    }

    pub fn finish(&mut self) {
        self.execution_context.write(ContextValue::Array(Array(
            self.loop_state.result.to_owned(),
        )));
    }
}
