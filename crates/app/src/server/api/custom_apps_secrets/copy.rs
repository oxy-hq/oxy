//! `copy_from` on the set route: store another environment's value of a key in
//! this one, **server-side**. Before, staff copied a production credential into
//! staging by revealing it in the console and pasting it back — a plaintext
//! production secret through a browser and a clipboard, for a value the server
//! already holds. Now the value is read and written here, and never returned.
//!
//! **Only downhill** ([`downhill`]): production into staging or a sandbox,
//! staging into a sandbox. Nothing is copied *into* production — a staging
//! value is test material, and promoting one is a decision a person makes by
//! typing it, not a button. The source is resolved through the same
//! [`environment::resolve`] as every other request, so reading it is gated
//! exactly as revealing it would be; and a sandbox agent token, which never
//! reveals a value, never copies one either.

use axum::http::StatusCode;
use entity::apps;
use oxy::service::secret_manager::SecretManagerService;
use oxy_app_core::custom_app_environment::AppEnvironment;
use sea_orm::DatabaseConnection;

use super::environment::{self, Caller};
use super::{Failure, SetSecretRequest, ops, scope};

/// Whether a value may be copied from `source` into `target`.
pub(super) fn downhill(source: &AppEnvironment, target: &AppEnvironment) -> bool {
    matches!(
        (source, target),
        (AppEnvironment::Production, AppEnvironment::Staging)
            | (AppEnvironment::Production, AppEnvironment::Dev { .. })
            | (AppEnvironment::Staging, AppEnvironment::Dev { .. })
    )
}

/// A request names a value or a source, never both: with both, which one was
/// meant is a guess, and guessing wrong writes the wrong secret.
pub(super) fn one_value(body: &SetSecretRequest) -> Result<(), Failure> {
    if body.copy_from.is_some() && !body.value.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "send a value or copy_from, not both".to_string(),
        ));
    }
    Ok(())
}

/// When `body` asks for a copy, fill its value from the source environment and
/// answer which that was; `None` (and `body` untouched) otherwise.
///
/// `target` is already resolved — authorized, and a sandbox known to exist.
pub(super) async fn fill_from_source(
    db: &DatabaseConnection,
    app: &apps::Model,
    caller: Caller<'_>,
    target: &AppEnvironment,
    body: &mut SetSecretRequest,
) -> Result<Option<AppEnvironment>, Failure> {
    one_value(body)?;
    let Some(raw) = body.copy_from.clone() else {
        return Ok(None);
    };
    if super::agent::token_of(caller.user).is_some() {
        return Err((StatusCode::NOT_FOUND, "secret not found".to_string()));
    }
    let source = environment::resolve(db, app, caller, Some(&raw)).await?;
    if !downhill(&source, target) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "copy_from {source} into {target} is refused: a value is copied from production \
                 into staging or a sandbox, or from staging into a sandbox"
            ),
        ));
    }
    let key = ops::bare_key(&body.key)?;
    let value = SecretManagerService::new(app.project_id)
        .get_secret(&scope::secret_name(app.id, &source, key))
        .await
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("{source} holds no value for {key} to copy"),
            )
        })?;
    body.value = value;
    Ok(Some(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sandbox() -> AppEnvironment {
        AppEnvironment::Dev {
            handle: "a1".into(),
        }
    }

    /// The whole table: three directions copy, and nothing ever lands in
    /// production or moves sideways.
    #[test]
    fn a_value_is_copied_only_downhill() {
        use AppEnvironment::{Production, Staging};
        let all = [Production, Staging, sandbox()];
        let allowed: Vec<(String, String)> = all
            .iter()
            .flat_map(|s| all.iter().map(move |t| (s, t)))
            .filter(|(s, t)| downhill(s, t))
            .map(|(s, t)| (s.name(), t.name()))
            .collect();
        assert_eq!(
            allowed,
            vec![
                ("production".to_string(), "staging".to_string()),
                ("production".to_string(), "dev-a1".to_string()),
                ("staging".to_string(), "dev-a1".to_string()),
            ]
        );
    }

    #[test]
    fn a_request_names_a_value_or_a_source() {
        let body = |value: &str, copy_from: Option<&str>| {
            serde_json::from_value::<SetSecretRequest>(serde_json::json!({
                "key": "K", "value": value, "copy_from": copy_from
            }))
            .expect("request")
        };
        assert!(one_value(&body("v", None)).is_ok());
        assert!(one_value(&body("", Some("production"))).is_ok());
        let (code, _) = one_value(&body("v", Some("production"))).unwrap_err();
        assert_eq!(code, StatusCode::BAD_REQUEST);
        // `value` may be left out entirely when copying.
        let copy: SetSecretRequest =
            serde_json::from_value(serde_json::json!({ "key": "K", "copy_from": "staging" }))
                .expect("value is optional");
        assert!(one_value(&copy).is_ok());
    }
}
