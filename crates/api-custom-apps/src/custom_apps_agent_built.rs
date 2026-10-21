//! The axum-facing half of the sandbox-agent-built rules: [`AgentBuilt`], the
//! `409 draft_published_by_agent` body a promote or rollback refuses with, and
//! [`PromoteRefusal`], what the promote routes answer with.
//!
//! The mark itself (which builds a sandbox agent token published, its cache,
//! and production's fallback rule) lives in `oxy_custom_apps::agent_built`,
//! which has no HTTP types; it is re-exported here under the old path.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use entity::{api_tokens, app_builds, app_environment_events, users};
use sea_orm::{ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QueryOrder};
use uuid::Uuid;

pub use oxy_custom_apps::agent_built::*;

/// A build no blind promote ships, and who published it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentBuilt {
    /// The build's label, as the publish named it.
    pub build_id: String,
    /// The environment the token published it to: a sandbox of its own
    /// (`dev-<handle>`), or `staging`. `None` when no pointer event records
    /// one.
    pub environment: Option<String>,
    pub token_id: Uuid,
    /// `None` when the token's row is gone with its minter.
    pub token_name: Option<String>,
    /// The address of the person who minted the token.
    pub minter: Option<String>,
}

impl AgentBuilt {
    /// Who published it, where to, and what to do: sentences a person can
    /// act on, whether the build was a draft or a sandbox's.
    pub fn message(&self) -> String {
        let token = match &self.token_name {
            Some(name) => format!("sandbox agent token {name:?}"),
            None => format!(
                "a sandbox agent token that no longer exists ({})",
                self.token_id
            ),
        };
        let minter = match &self.minter {
            Some(email) => format!(", minted by {email}"),
            None => String::new(),
        };
        let to = match &self.environment {
            Some(environment) => format!(" to {environment}"),
            None => String::new(),
        };
        format!(
            "build {:?} was published{to} by {token}{minter}, and nobody has approved it, so it \
             is not made live as it is. Publish the build under your own name (`oxyc publish`): \
             that build is yours, and is promoted as usual. Or approve a promote request for it",
            self.build_id
        )
    }

    /// `409`, as JSON a client can branch on.
    pub fn into_response(self) -> Response {
        let body = serde_json::json!({
            "code": PROMOTE_REFUSED,
            "error": PROMOTE_REFUSED,
            "message": self.message(),
            "build_id": self.build_id,
            "environment": self.environment,
            "token_id": self.token_id,
            "token_name": self.token_name,
            "minter": self.minter,
        });
        (StatusCode::CONFLICT, Json(body)).into_response()
    }
}

/// What a promote or a rollback route refuses with: the bare status it has
/// always answered, or — for a build a sandbox agent token published — the
/// coded `409`, or a refusal the one production move added
/// (`custom_apps_promote`: `staging_never_served`, `migration_failed`) as
/// `{"code","error","message"}`. One type, so a route gains a coded answer
/// without its other answers changing by a byte.
#[derive(Debug)]
pub enum PromoteRefusal {
    Status(StatusCode),
    AgentBuilt(Box<AgentBuilt>),
    Coded {
        status: StatusCode,
        code: &'static str,
        message: String,
    },
}

impl From<StatusCode> for PromoteRefusal {
    fn from(status: StatusCode) -> Self {
        Self::Status(status)
    }
}

impl From<AgentBuilt> for PromoteRefusal {
    fn from(built: AgentBuilt) -> Self {
        Self::AgentBuilt(Box::new(built))
    }
}

impl IntoResponse for PromoteRefusal {
    fn into_response(self) -> Response {
        match self {
            Self::Status(status) => status.into_response(),
            Self::AgentBuilt(built) => built.into_response(),
            Self::Coded {
                status,
                code,
                message,
            } => {
                let body = serde_json::json!({ "code": code, "error": code, "message": message });
                (status, Json(body)).into_response()
            }
        }
    }
}

impl AgentBuilt {
    /// Hydrate the refusal for `build`, which `token_id` published: the
    /// token's name, its minter and the environment the build went to. The
    /// caller learned of the mark from
    /// `custom_apps_build_state::BuildState::promotable`.
    pub(crate) async fn describe<C: ConnectionTrait>(
        db: &C,
        build: &app_builds::Model,
        token_id: Uuid,
    ) -> Result<Self, DbErr> {
        let token = api_tokens::Entity::find_by_id(token_id).one(db).await?;
        let minter = match &token {
            Some(token) => users::Entity::find_by_id(token.principal_user_id)
                .one(db)
                .await?
                .and_then(|user| user.email),
            None => None,
        };
        Ok(Self {
            build_id: build.build_id.clone(),
            environment: published_to(db, build.id).await?,
            token_id,
            token_name: token.map(|token| token.name),
            minter,
        })
    }
}

/// The environment build `build_pk` was published to: the one its first
/// pointer event names. A publish moves exactly one pointer to its new build
/// — a sandbox's, or staging's — before anything else can point at it.
async fn published_to<C: ConnectionTrait>(db: &C, build_pk: Uuid) -> Result<Option<String>, DbErr> {
    let first = app_environment_events::Entity::find()
        .filter(app_environment_events::Column::BuildId.eq(build_pk))
        .order_by_asc(app_environment_events::Column::At)
        .one(db)
        .await?;
    Ok(first.map(|event| event.environment))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn built(token_name: Option<&str>, minter: Option<&str>) -> AgentBuilt {
        AgentBuilt {
            build_id: "agent-draft-1".into(),
            environment: Some("staging".into()),
            token_id: Uuid::from_u128(0x70),
            token_name: token_name.map(str::to_string),
            minter: minter.map(str::to_string),
        }
    }

    /// The refusal names the build, where it was published, the token and
    /// its minter, and says both ways forward.
    #[test]
    fn the_message_says_who_published_it_and_what_to_do() {
        let message = built(Some("nightly agent"), Some("minter@oxy.tech")).message();
        for said in [
            "\"agent-draft-1\"",
            "published to staging by",
            "sandbox agent token \"nightly agent\"",
            "minted by minter@oxy.tech",
            "Publish the build under your own name",
            "approve a promote request",
        ] {
            assert!(message.contains(said), "{said}: {message}");
        }
        // A token whose row is gone is still named, by id.
        let message = built(None, None).message();
        assert!(message.contains("no longer exists"), "{message}");
        assert!(!message.contains("minted by"), "{message}");
    }

    /// A sandbox build is refused under the same code, and the message says
    /// it was a sandbox's: the environment it names is the one thing that
    /// tells the two apart.
    #[tokio::test]
    async fn a_sandbox_build_is_named_by_the_sandbox_it_was_published_to() {
        let sandbox = AgentBuilt {
            build_id: "sbx-7".into(),
            environment: Some("dev-a1".into()),
            ..built(Some("nightly agent"), Some("minter@oxy.tech"))
        };
        let message = sandbox.message();
        assert!(
            message.contains("\"sbx-7\" was published to dev-a1 by"),
            "{message}"
        );
        assert!(!message.contains("staging"), "{message}");
        let response = sandbox.into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("read the body");
        let body: serde_json::Value = serde_json::from_slice(&bytes).expect("a JSON body");
        assert_eq!(body["code"], "draft_published_by_agent");
        assert_eq!(body["environment"], "dev-a1");

        // A build no pointer event records is still refused, and says no place.
        let nowhere = AgentBuilt {
            environment: None,
            ..built(Some("nightly agent"), None)
        };
        let message = nowhere.message();
        assert!(
            message.contains("was published by sandbox agent token"),
            "{message}"
        );
    }

    #[tokio::test]
    async fn the_refusal_is_a_409_with_the_code_a_client_branches_on() {
        let refused = built(Some("nightly agent"), Some("minter@oxy.tech"));
        let message = refused.message();
        let response = refused.into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("read the body");
        let body: serde_json::Value = serde_json::from_slice(&bytes).expect("a JSON body");
        assert_eq!(body["code"], "draft_published_by_agent");
        assert_eq!(body["error"], "draft_published_by_agent");
        assert_eq!(body["message"], message.as_str());
        assert_eq!(body["build_id"], "agent-draft-1");
        assert_eq!(body["environment"], "staging");
        assert_eq!(body["token_name"], "nightly agent");
        assert_eq!(body["minter"], "minter@oxy.tech");
    }
}
