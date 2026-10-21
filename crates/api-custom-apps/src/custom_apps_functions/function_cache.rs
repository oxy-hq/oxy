//! The `app_functions` row of a build, remembered per `(app, build, function)`.
//!
//! Every invocation looked the function up by `(build, name)` to read its
//! manifest, one round trip before the isolate. The row cannot change under
//! that key: `record_functions` in `custom_apps_publish` inserts it once, when
//! the build is created, and nothing updates it afterwards — a re-publish
//! makes a new build with a new id, and a promote or rollback repoints the
//! environment at a different build id rather than rewriting this one. So the
//! entry never goes stale, and there is no invalidation hook to call.
//!
//! The cache decides nothing about who may run what. Which build an
//! environment serves is read where it always was (the app resolution cache
//! for production, the environment row for staging and a sandbox), and the
//! environment gate runs before this lookup. This only answers "what does this
//! build say about this function".
//!
//! A miss is never remembered: a function the build does not declare is
//! looked up again on the next call, as before. The entries are bounded and
//! expire like every other cache in `custom_apps_cache`.

use std::sync::OnceLock;

use entity::app_functions;
use entity::prelude::AppFunctions;
use sea_orm::{ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter};
use uuid::Uuid;

use super::super::custom_apps_cache::{TtlLru, standard_cache};

type Key = (Uuid, Uuid, String);

fn cache() -> &'static TtlLru<Key, app_functions::Model> {
    static CACHE: OnceLock<TtlLru<Key, app_functions::Model>> = OnceLock::new();
    CACHE.get_or_init(standard_cache)
}

fn key(app_id: Uuid, build_id: Uuid, name: &str) -> Key {
    (app_id, build_id, name.to_string())
}

/// The row `build_id` holds for function `name`, from memory when this
/// replica has read it within the last minute.
pub(super) async fn function_row<C: ConnectionTrait>(
    db: &C,
    app_id: Uuid,
    build_id: Uuid,
    name: &str,
) -> Result<Option<app_functions::Model>, DbErr> {
    if let Some(row) = cache().get(&key(app_id, build_id, name)) {
        return Ok(Some(row));
    }
    let row = AppFunctions::find()
        .filter(app_functions::Column::BuildId.eq(build_id))
        .filter(app_functions::Column::Name.eq(name))
        .one(db)
        .await?;
    if let Some(row) = &row {
        cache().insert(key(app_id, build_id, name), row.clone());
    }
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(build_id: Uuid, name: &str) -> app_functions::Model {
        serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4(),
            "app_id": Uuid::new_v4(),
            "build_id": build_id,
            "name": name,
            "manifest_json": { "route": true },
            "artifact_key": format!("functions/{name}.js"),
            "created_at": chrono::Utc::now().fixed_offset(),
        }))
        .expect("a function row")
    }

    #[test]
    fn a_row_is_found_by_app_build_and_function_together() {
        let (app, build) = (Uuid::new_v4(), Uuid::new_v4());
        cache().insert(key(app, build, "price"), row(build, "price"));
        assert!(cache().get(&key(app, build, "price")).is_some());
        assert!(cache().get(&key(app, build, "other")).is_none());
        assert!(cache().get(&key(app, Uuid::new_v4(), "price")).is_none());
        assert!(cache().get(&key(Uuid::new_v4(), build, "price")).is_none());
    }
}
