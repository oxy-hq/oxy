use std::{
    ops::DerefMut,
    sync::{Arc, RwLock},
};

use futures::TryFutureExt;
use minijinja::{Environment, Value, context, value::Enumerator};
use tokio::task::spawn_blocking;

use crate::errors::OxyError;

pub trait TemplateRegister: Sync + Send {
    fn register_template(&self, renderer: &Renderer) -> Result<(), OxyError>;
}

impl TemplateRegister for &str {
    fn register_template(&self, renderer: &Renderer) -> Result<(), OxyError> {
        renderer.register_template(self)
    }
}

pub struct NoopRegister;

impl TemplateRegister for NoopRegister {
    fn register_template(&self, _renderer: &Renderer) -> Result<(), OxyError> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Renderer {
    env: Arc<RwLock<Environment<'static>>>,
    global_context: Arc<Value>,
    current_context: Value,
}

impl Renderer {
    pub fn new(global_context: Value) -> Self {
        let env = Environment::new();
        Renderer {
            env: Arc::new(RwLock::new(env)),
            global_context: Arc::new(global_context),
            current_context: Default::default(),
        }
    }

    pub fn from_template<T: TemplateRegister>(
        global_context: Value,
        template: &T,
    ) -> Result<Self, OxyError> {
        let renderer = Renderer::new(global_context);
        renderer.register(template)?;
        Ok(renderer)
    }

    pub fn wrap(&self, context: &Value) -> Renderer {
        Renderer {
            env: self.env.clone(),
            global_context: self.global_context.clone(),
            current_context: context! {
              ..Value::from_serialize(&self.current_context),
              ..Value::from_serialize(context),
            },
        }
    }

    pub fn switch_context(&self, global_context: Value, context: Value) -> Renderer {
        Renderer {
            env: Arc::new(RwLock::new(Environment::new())),
            global_context: Arc::new(global_context),
            current_context: context,
        }
    }

    pub fn register<T: TemplateRegister>(&self, item: &T) -> Result<(), OxyError> {
        item.register_template(self)
    }

    pub fn child_register(&self) -> ChildRegister<'_> {
        ChildRegister::new(self)
    }

    pub fn register_template(&self, value: &str) -> Result<(), OxyError> {
        self.env
            .write()?
            .deref_mut()
            .add_template_owned(value.to_string(), value.to_string())
            .map_err(|err| OxyError::RuntimeError(format!("Failed to add template {err}")))?;
        Ok(())
    }

    pub fn render(&self, template: &str) -> Result<String, OxyError> {
        let ctx = self.get_context();
        self.render_sync_internal(template, ctx)
    }

    pub fn render_once(&self, template: &str, context: Value) -> Result<String, OxyError> {
        self.register_template(template)?;
        self.render_sync_internal(template, context)
    }

    pub async fn render_async(&self, template: &str) -> Result<String, OxyError> {
        self.render_async_internal(template, self.get_context())
            .await
    }

    pub async fn render_once_async(
        &mut self,
        template: &str,
        context: Value,
    ) -> Result<String, OxyError> {
        self.register_template(template)?;
        self.render_async_internal(template, context).await
    }

    pub fn eval_expression(&self, template: &str) -> Result<Value, OxyError> {
        let env = self.env.read()?;
        let variable_regex = regex::Regex::new(r"^\{\{(.*)\}\}$")
            .map_err(|err| OxyError::RuntimeError(format!("Invalid regex: {err}")))?;
        let variable = variable_regex.replace(template.trim(), "$1").to_string();
        let expression = env.compile_expression(&variable).map_err(|err| {
            OxyError::RuntimeError(format!("Failed to compile expression {template} :{err}"))
        })?;
        let context = self.get_context();
        let value = expression.eval(&context).map_err(|err| {
            OxyError::RuntimeError(format!("Error evaluating expression: {}", err))
        })?;
        tracing::info!(
            "Evaluated expression: {} -> {:?} with context: {:?}",
            template,
            value,
            &context
        );
        Ok(value)
    }

    pub fn eval_enumerate<V>(&self, template: &str) -> Result<Vec<V>, OxyError>
    where
        V: From<Value>,
    {
        let rendered = self.eval_expression(template)?;
        let rendered_value = match rendered.as_object() {
            Some(obj) => obj,
            None => {
                return Err(OxyError::RuntimeError(format!(
                    "Values {} did not resolve to an object",
                    template,
                )));
            }
        };

        match rendered_value.enumerate() {
            Enumerator::Seq(length) => {
                let mut values = Vec::new();
                for idx in 0..length {
                    let value = rendered_value
                        .get_value(&Value::from(idx))
                        .unwrap_or_default();
                    values.push(value.into());
                }
                Ok(values)
            }
            _ => Err(OxyError::RuntimeError(format!(
                "Values {} did not resolve to an array. \nContext: {}",
                template,
                self.get_context()
            ))),
        }
    }

    async fn render_async_internal(
        &self,
        template: &str,
        context: Value,
    ) -> Result<String, OxyError> {
        let env = self.env.clone();
        let template = template.to_string();
        spawn_blocking(move || {
            let env = env.read()?;
            let tmpl = match env.get_template(&template) {
                Ok(tmpl) => tmpl,
                Err(err) => {
                    return Err(OxyError::ConfigurationError(format!(
                        "Template \"{template}\" not found: {err}"
                    )));
                }
            };
            tmpl.render(context).map_err(|err| {
                OxyError::RuntimeError(format!("Error rendering template: {:?}", err))
            })
        })
        .map_err(|err| OxyError::RuntimeError(format!("Error rendering template: {:?}", err)))
        .await?
    }

    fn render_sync_internal(&self, template: &str, context: Value) -> Result<String, OxyError> {
        let env = self.env.write()?;
        let tmpl = env.get_template(template).map_err(|err| {
            OxyError::ConfigurationError(format!("Template \"{template}\" not found: {err}"))
        })?;
        tmpl.render(context)
            .map_err(|err| OxyError::RuntimeError(format!("Error rendering template: {:?}", err)))
    }

    pub fn get_context(&self) -> Value {
        context! {
          ..Value::from_serialize(self.global_context.to_owned()),
          ..Value::from_serialize(&self.current_context),
        }
    }
}

pub struct ChildRegister<'register> {
    renderer: &'register Renderer,
}

impl<'register> ChildRegister<'register> {
    pub fn new(renderer: &'register Renderer) -> Self {
        ChildRegister { renderer }
    }

    pub fn entries<T, I>(&mut self, values: I) -> Result<(), OxyError>
    where
        T: TemplateRegister,
        I: IntoIterator<Item = T>,
    {
        for value in values {
            self.entry(&value)?;
        }
        Ok(())
    }

    pub fn entry<T: TemplateRegister>(&self, value: &T) -> Result<&Self, OxyError> {
        value.register_template(self.renderer)?;
        Ok(self)
    }
}
