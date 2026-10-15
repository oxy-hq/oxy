use std::{
    ops::DerefMut,
    sync::{Arc, RwLock},
};

use futures::TryFutureExt;
use minijinja::{Environment, Value, context, value::Enumerator};
use tokio::task::spawn_blocking;

use oxy_shared::errors::OxyError;
use oxy_shared::jinja::{name_the_limit, render_bounded};

/// `now(utc?, fmt?)` — see [`oxy_shared::jinja::add_now_function`]:
///
/// ```jinja2
/// {{ now() }}                           # local datetime, RFC 3339
/// {{ now(utc=true, fmt='%Y-%m-%d') }}   # UTC date, "2024-10-21"
/// ```
///
/// A `fmt` chrono rejects is a render error, never a panic.
fn add_global_functions(env: &mut Environment<'static>) {
    oxy_shared::jinja::add_now_function(env);

    env.add_filter(
        "tojson",
        |value: Value| -> Result<String, minijinja::Error> {
            serde_json::to_string(&value).map_err(|e| {
                minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    format!("Failed to convert to JSON: {}", e),
                )
            })
        },
    );

    // `sqlquote` writes a value as a `'…'` SQL literal:
    //   {{ controls.store | sqlquote }}  →  'Paris'
    // Do NOT add surrounding quotes in the template — sqlquote provides them.
    //
    // Nothing rendered through this environment is sent to a database from
    // here — control defaults, retrieval text, eval prompts — so the engine
    // that would read the literal is not known, and engines disagree on how a
    // quote or a backslash is written inside one. A value holding either is
    // refused rather than escaped by one engine's rule and broken out of on
    // another's. SQL that runs is rendered by the automation engine, which
    // escapes by the rule of the task's `database`.
    env.add_filter(
        "sqlquote",
        |value: Value| -> Result<String, minijinja::Error> {
            oxy_shared::sql_literal::quote_engine_unknown(&value.to_string()).map_err(|refused| {
                minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, refused.to_string())
            })
        },
    );
}

#[derive(Debug, Clone)]
pub struct Renderer {
    env: Arc<RwLock<Environment<'static>>>,
    global_context: Arc<Value>,
    current_context: Value,
}

impl Renderer {
    pub fn new(global_context: Value) -> Self {
        let env = setup_jinja_environment();

        Renderer {
            env: Arc::new(RwLock::new(env)),
            global_context: Arc::new(global_context),
            current_context: Default::default(),
        }
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
        let env = setup_jinja_environment();

        Renderer {
            env: Arc::new(RwLock::new(env)),
            global_context: Arc::new(global_context),
            current_context: context,
        }
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

    pub fn render_str(&self, source: &str) -> Result<String, OxyError> {
        let ctx = self.get_context();
        let env = self.env.read()?;
        env.template_from_str(source)
            .and_then(|tmpl| render_bounded(&tmpl, ctx))
            .map_err(|err| {
                OxyError::RuntimeError(format!("Error rendering template string: {err:?}"))
            })
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
        let value = expression
            .eval(&context)
            .map_err(name_the_limit)
            .map_err(|err| {
                OxyError::RuntimeError(format!(
                    "Error evaluating expression: {} with context: {:?}",
                    err, context
                ))
            })?;
        tracing::debug!(
            "Evaluated expression: {} -> {:?} with context: {:?}",
            template,
            value,
            &context
        );
        Ok(value)
    }

    pub fn eval_enumerate(&self, template: &str) -> Result<Vec<Value>, OxyError> {
        let rendered = self.eval_expression(template)?;
        let rendered_value = match rendered.as_object() {
            Some(obj) => obj,
            None => {
                return Err(OxyError::RuntimeError(format!(
                    "Values {template} did not resolve to an object",
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
                    values.push(value);
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
            render_bounded(&tmpl, context)
                .map_err(|err| OxyError::RuntimeError(format!("Error rendering template: {err:?}")))
        })
        .map_err(|err| OxyError::RuntimeError(format!("Error rendering template: {err:?}")))
        .await?
    }

    fn render_sync_internal(&self, template: &str, context: Value) -> Result<String, OxyError> {
        let env = self.env.write()?;
        let tmpl = env.get_template(template).map_err(|err| {
            OxyError::ConfigurationError(format!("Template \"{template}\" not found: {err}"))
        })?;
        render_bounded(&tmpl, context)
            .map_err(|err| OxyError::RuntimeError(format!("Error rendering template: {err:?}")))
    }

    pub fn get_context(&self) -> Value {
        context! {
          ..Value::from_serialize(self.global_context.as_ref()),
          ..Value::from_serialize(&self.current_context),
        }
    }
}

pub fn setup_jinja_environment() -> Environment<'static> {
    let mut env = Environment::new();
    oxy_shared::jinja::apply_limits(&mut env);
    add_global_functions(&mut env);
    env
}

#[cfg(test)]
mod tests {
    use super::*;
    use minijinja::context;

    #[test]
    fn test_now_function_default() {
        let renderer = Renderer::new(context! {});
        let template = "{{ now() }}";
        renderer.register_template(template).unwrap();
        let result = renderer.render(template).unwrap();
        // Should return a non-empty datetime string
        assert!(!result.is_empty());
    }

    #[test]
    fn test_now_function_with_utc() {
        let renderer = Renderer::new(context! {});
        let template = "{{ now(utc=true) }}";
        renderer.register_template(template).unwrap();
        let result = renderer.render(template).unwrap();
        // Should return a non-empty UTC datetime string
        assert!(!result.is_empty());
    }

    #[test]
    fn test_now_function_with_format() {
        let renderer = Renderer::new(context! {});
        let template = "{{ now(fmt='%Y-%m-%d') }}";
        renderer.register_template(template).unwrap();
        let result = renderer.render(template).unwrap();
        // Should return a date in YYYY-MM-DD format
        assert_eq!(result.len(), 10, "Expected YYYY-MM-DD format"); // YYYY-MM-DD is 10 characters
        assert!(result.contains('-'));
    }

    #[test]
    fn test_now_function_with_utc_and_format() {
        let renderer = Renderer::new(context! {});
        let template = "{{ now(utc=true, fmt='%Y-%m-%d %H:%M:%S') }}";
        renderer.register_template(template).unwrap();
        let result = renderer.render(template).unwrap();
        // Should return a datetime in YYYY-MM-DD HH:MM:SS format
        assert_eq!(result.len(), 19, "Expected YYYY-MM-DD HH:MM:SS format"); // "YYYY-MM-DD HH:MM:SS" is 19 characters
    }

    /// A format chrono rejects used to panic inside `to_string()`, unwinding
    /// the request's task. It is a render error on every entry point now.
    #[test]
    fn now_with_a_bad_format_is_an_error_not_a_panic() {
        let renderer = Renderer::new(context! {});
        let template = "{{ now(fmt='%Q') }}";
        let err = renderer.render_str(template).unwrap_err().to_string();
        assert!(err.contains("is not a strftime format"), "{err}");
        renderer.register_template(template).unwrap();
        assert!(renderer.render(template).is_err());
    }

    /// The environment carries the fuel limit, and the error names it.
    #[test]
    fn an_endless_loop_stops_and_names_the_limit() {
        let renderer = Renderer::new(context! {});
        let err = renderer
            .render_str(
                "{% for a in range(100000) %}{% for b in range(100000) %}{% endfor %}{% endfor %}",
            )
            .unwrap_err()
            .to_string();
        assert!(err.contains("TEMPLATE_FUEL"), "{err}");
    }

    fn sqlquote(value: &str) -> Result<String, OxyError> {
        Renderer::new(context! { v => value }).render_str("x = {{ v | sqlquote }} AND tail")
    }

    #[test]
    fn sqlquote_writes_a_plain_value_unchanged() {
        assert_eq!(sqlquote("Paris").unwrap(), "x = 'Paris' AND tail");
        assert_eq!(sqlquote("2024-01-01").unwrap(), "x = '2024-01-01' AND tail");
        assert_eq!(sqlquote("").unwrap(), "x = '' AND tail");
    }

    /// No engine is known to this renderer, and a quote-doubled literal ends
    /// early on an engine that reads a backslash (`C:\`, `x\' OR 1=1 -- `)
    /// or one that does not read `''` (`it's` on BigQuery). So none is written.
    #[test]
    fn sqlquote_refuses_a_value_engines_read_differently() {
        for value in ["a\\b", "it's", "x\\' OR 1=1 -- ", "C:\\"] {
            let refused = sqlquote(value).unwrap_err().to_string();
            assert!(refused.contains("is not known"), "{value:?}: {refused}");
        }
    }
}
