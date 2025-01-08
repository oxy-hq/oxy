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

struct MapAdapter<'writer, 'state, Event> {
    key: String,
    writer: &'writer mut (dyn Write<Event> + 'writer),
    state: &'state mut MapAdapterState,
}

impl<'writer, 'state, Event> MapAdapter<'writer, 'state, Event> {
    fn wrap<'slot, 'context: 'writer + 'slot>(
        execution_context: &'context mut ExecutionContext<'_, Event>,
        slot: &'slot mut Option<Self>,
        map_state: &'state mut MapAdapterState,
        key: &str,
    ) -> ExecutionContext<'slot, Event> {
        let current = context! {
          ..Value::from_serialize(execution_context.context.clone()),
          ..Value::from_object(map_state.result.to_owned()),
        };
        log::info!(
            "MapAdapter.wrap{key} with context: {:?}, Current result: {:?}",
            current,
            map_state.result
        );
        execution_context.wrap(
            move |writer| {
                slot.insert(MapAdapter {
                    state: map_state,
                    writer,
                    key: key.to_string(),
                })
            },
            Some(key.to_string()),
            current,
        )
    }
}

impl<Event> Write<Event> for MapAdapter<'_, '_, Event> {
    fn write(&mut self, value: ContextValue) {
        log::info!(
            "MapAdapter.write to key `{}` with value: {:?}",
            self.key,
            value
        );
        self.state.result.set_value(&self.key, value.clone());
    }

    fn notify(&self, event: Event) {
        self.writer.notify(event);
    }
}

pub struct MapExecutor<'context, 'writer: 'context, Event> {
    pub execution_context: &'context mut ExecutionContext<'writer, Event>,
    map_state: MapAdapterState,
}

impl<'context, 'writer: 'context, Event> MapExecutor<'context, 'writer, Event> {
    pub fn new(execution_context: &'context mut ExecutionContext<'writer, Event>) -> Self {
        Self {
            execution_context,
            map_state: Default::default(),
        }
    }

    pub async fn entries<E, I>(&mut self, entries: I) -> Result<(), OnyxError>
    where
        E: Executable<Event>,
        I: IntoIterator<Item = (String, E)>,
    {
        for (key, entry) in entries {
            self.entry(&key, &entry).await?;
        }
        Ok(())
    }

    pub async fn entry(
        &mut self,
        key: &str,
        entry: &dyn Executable<Event>,
    ) -> Result<(), OnyxError> {
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

pub struct LoopAdapter<'writer, 'state, Event> {
    writer: &'writer mut (dyn Write<Event> + 'writer),
    state: &'state mut LoopAdapterState,
}

impl<'writer, 'state, Event> LoopAdapter<'writer, 'state, Event> {
    fn wrap<'slot, 'context: 'writer + 'slot>(
        execution_context: &'context mut ExecutionContext<'_, Event>,
        slot: &'slot mut Option<Self>,
        loop_state: &'state mut LoopAdapterState,
        input: &ContextValue,
    ) -> ExecutionContext<'slot, Event> {
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
            move |writer| {
                slot.insert(LoopAdapter {
                    writer,
                    state: loop_state,
                })
            },
            Some(name.to_string()),
            current,
        )
    }
}

impl<Event> Write<Event> for LoopAdapter<'_, '_, Event> {
    fn write(&mut self, value: ContextValue) {
        self.state.result.push(value);
    }

    fn notify(&self, event: Event) {
        self.writer.notify(event);
    }
}

pub struct LoopExecutor<'context, 'writer: 'context, Event> {
    execution_context: &'context mut ExecutionContext<'writer, Event>,
    loop_state: LoopAdapterState,
}

impl<'context, 'writer: 'context, Event> LoopExecutor<'context, 'writer, Event> {
    pub fn new(execution_context: &'context mut ExecutionContext<'writer, Event>) -> Self {
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
        entry: &dyn Executable<Event>,
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
