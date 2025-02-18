use super::value::ContextValue;

pub trait Write {
    fn write(&mut self, value: ContextValue);
}

pub struct OutputCollector {
    pub output: ContextValue,
}

impl Default for OutputCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputCollector {
    pub fn new() -> Self {
        Self {
            output: ContextValue::None,
        }
    }
}

impl Write for OutputCollector {
    fn write(&mut self, value: ContextValue) {
        self.output = value;
    }
}
