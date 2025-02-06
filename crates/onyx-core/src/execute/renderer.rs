use futures::TryFutureExt;
use minijinja::{Environment, Value};
use tokio::task::spawn_blocking;

use crate::errors::OnyxError;

pub trait TemplateRegister {
    fn register_template(&self, renderer: &mut Renderer) -> Result<(), OnyxError>;
}

impl TemplateRegister for &str {
    fn register_template(&self, renderer: &mut Renderer) -> Result<(), OnyxError> {
        renderer.register_template(self)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Renderer {
    env: Environment<'static>,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer {
    pub fn new() -> Self {
        let env = Environment::new();
        Renderer { env }
    }

    pub fn register(&mut self, item: &dyn TemplateRegister) -> Result<(), OnyxError> {
        item.register_template(self)
    }

    pub fn struct_register(&mut self) -> StructRegister {
        StructRegister::new(self)
    }

    pub fn list_register(&mut self) -> ListRegister {
        ListRegister::new(self)
    }

    pub fn register_template(&mut self, value: &str) -> Result<(), OnyxError> {
        let name = value.to_string();
        let template = value.to_string();
        self.env
            .add_template_owned(name, template)
            .map_err(|err| OnyxError::RuntimeError(format!("Failed to add template {err}")))?;
        Ok(())
    }

    pub async fn render_async(&self, template: &str, context: Value) -> Result<String, OnyxError> {
        let env = self.env.clone();
        let template = template.to_string();
        spawn_blocking(move || {
            let tmpl = match env.get_template(&template) {
                Ok(tmpl) => tmpl,
                Err(err) => {
                    return Err(OnyxError::ConfigurationError(format!(
                        "Template \"{template}\" not found: {err}"
                    )));
                }
            };
            tmpl.render(context).map_err(|err| {
                OnyxError::RuntimeError(format!("Error rendering template: {:?}", err))
            })
        })
        .map_err(|err| OnyxError::RuntimeError(format!("Error rendering template: {:?}", err)))
        .await?
    }

    pub async fn render_temp_async(
        &mut self,
        template: &str,
        context: Value,
    ) -> Result<String, OnyxError> {
        self.register(&template)?;
        self.render_async(template, context).await
    }

    pub fn eval_expression(&self, template: &str, context: &Value) -> Result<Value, OnyxError> {
        let tmpl = self.env.get_template(template).map_err(|err| {
            OnyxError::ConfigurationError(format!("Template \"{template}\" not found: {err}"))
        })?;
        let variables = tmpl.undeclared_variables(true);
        if variables.len() != 1 {
            return Err(OnyxError::RuntimeError(format!(
                "Expected one variable in expression, found {}",
                variables.len()
            )));
        }
        let variable = variables.iter().next().unwrap();
        let expression = self.env.compile_expression(variable).map_err(|err| {
            OnyxError::RuntimeError(format!("Failed to compile expression {variable} :{err}"))
        })?;
        let value = expression.eval(context).map_err(|err| {
            OnyxError::RuntimeError(format!("Error evaluating expression: {}", err))
        })?;
        log::info!(
            "Evaluated expression: {} -> {:?} with context: {:?}",
            template,
            value,
            context
        );
        Ok(value)
    }
}

pub struct StructRegister<'renderer> {
    renderer: &'renderer mut Renderer,
}

impl<'renderer> StructRegister<'renderer> {
    pub fn new(renderer: &'renderer mut Renderer) -> Self {
        StructRegister { renderer }
    }

    pub fn fields<T, I>(&mut self, values: I) -> Result<(), OnyxError>
    where
        T: TemplateRegister,
        I: IntoIterator<Item = T>,
    {
        for value in values {
            self.field(&value)?;
        }
        Ok(())
    }

    pub fn field(&mut self, value: &dyn TemplateRegister) -> Result<&mut Self, OnyxError> {
        value.register_template(self.renderer)?;
        Ok(self)
    }
}

pub struct ListRegister<'renderer> {
    renderer: &'renderer mut Renderer,
}

impl<'renderer> ListRegister<'renderer> {
    pub fn new(renderer: &'renderer mut Renderer) -> Self {
        ListRegister { renderer }
    }

    pub fn items<D, I>(&mut self, entries: I) -> Result<(), OnyxError>
    where
        D: TemplateRegister,
        I: IntoIterator<Item = D>,
    {
        for entry in entries {
            self.item(&entry)?;
        }
        Ok(())
    }

    pub fn item(&mut self, value: &dyn TemplateRegister) -> Result<&mut Self, OnyxError> {
        value.register_template(self.renderer)?;
        Ok(self)
    }
}
