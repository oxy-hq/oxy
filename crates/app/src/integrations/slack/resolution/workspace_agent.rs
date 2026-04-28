use crate::integrations::slack::services::channel_defaults::ChannelDefaultsService;
use crate::integrations::slack::services::user_preferences::UserPreferencesService;
use entity::org_members;
use entity::prelude::Workspaces;
use entity::slack_installations::Model as InstallationRow;
use entity::slack_user_links::Model as UserLinkRow;
use entity::workspaces;
use oxy::adapters::workspace::builder::WorkspaceBuilder;
use oxy::adapters::workspace::resolve_workspace_path;
use oxy::database::client::establish_connection;
use oxy_shared::errors::OxyError;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum Resolution {
    Resolved {
        workspace_id: Uuid,
        agent_path: String,
    },
    /// Multiple workspaces exist and no preference (user default or channel
    /// default) narrowed the choice down. The orchestrator opens the picker
    /// and waits for the user to choose.
    PickerNeeded {
        workspaces: Vec<WorkspaceSummary>,
    },
    NoWorkspaces(NoWorkspacesReason),
    /// The resolved workspace exists but has no agents configured.
    WorkspaceHasNoAgents {
        workspace_id: Uuid,
        workspace_name: String,
    },
}

/// Why `resolve` returned `NoWorkspaces`. Surfaced in the Slack ephemeral so
/// the user (and whoever is debugging the deployment) can see what's actually
/// wrong instead of a generic "no access" line.
#[derive(Debug, Clone)]
pub enum NoWorkspacesReason {
    /// The linked Oxy account isn't in this install's org at all.
    UserNotInOrg { oxy_user_id: Uuid, org_id: Uuid },
    /// The org has zero workspace rows.
    OrgHasNoWorkspaces { org_id: Uuid },
}

impl NoWorkspacesReason {
    /// Short human-readable explanation, embedded verbatim in the Slack
    /// ephemeral. Includes the IDs so an operator can grep logs / DB.
    pub fn user_message(&self) -> String {
        match self {
            Self::UserNotInOrg {
                oxy_user_id,
                org_id,
            } => format!(
                "Your Oxygen account isn't a member of this org.\n\
                 • Oxygen user: `{oxy_user_id}`\n\
                 • Org: `{org_id}`\n\
                 Ask an org admin to invite you, or reinstall Slack under the correct org."
            ),
            Self::OrgHasNoWorkspaces { org_id } => format!(
                "This Oxygen org has no workspaces yet. Create one in the web app first.\n\
                 • Org: `{org_id}`"
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorkspaceSummary {
    pub id: Uuid,
    pub name: String,
    pub agents: Vec<String>,
    pub default_agent: Option<String>,
}

/// Resolve the workspace and agent for an incoming Slack message.
///
/// Resolution cascade (first match wins):
/// 1. Org membership check — returns `NoWorkspaces(UserNotInOrg)` if not a member.
/// 2. Channel default — if a per-channel default exists and its workspace is still
///    accessible, resolve silently.
/// 3. User preference — if the user has a stored default workspace, resolve silently.
/// 4. Single workspace — if the org has exactly one workspace, resolve silently.
/// 5. Multiple workspaces and no preference → `PickerNeeded`.
pub async fn resolve(
    installation: &InstallationRow,
    user_link: &UserLinkRow,
    channel_id: &str,
) -> Result<Resolution, OxyError> {
    let conn = establish_connection().await?;

    // Access model matches the web app's list_workspaces endpoint (workspaces.rs
    // around line 1068): any member of the org sees every workspace in the
    // org. workspace_members rows are per-workspace role *overrides*, not the
    // source of base access — filtering by them here incorrectly hid all
    // workspaces from org Owners/Admins who never get explicit rows.
    let is_org_member = org_members::Entity::find()
        .filter(org_members::Column::OrgId.eq(installation.org_id))
        .filter(org_members::Column::UserId.eq(user_link.oxy_user_id))
        .one(&conn)
        .await
        .map_err(|e| OxyError::DBError(e.to_string()))?
        .is_some();
    if !is_org_member {
        tracing::info!(
            oxy_user_id = %user_link.oxy_user_id,
            org_id = %installation.org_id,
            "slack resolve: user is not a member of the install's org"
        );
        return Ok(Resolution::NoWorkspaces(NoWorkspacesReason::UserNotInOrg {
            oxy_user_id: user_link.oxy_user_id,
            org_id: installation.org_id,
        }));
    }

    let user_workspaces = Workspaces::find()
        .filter(workspaces::Column::OrgId.eq(Some(installation.org_id)))
        .all(&conn)
        .await
        .map_err(|e| OxyError::DBError(e.to_string()))?;

    if user_workspaces.is_empty() {
        tracing::info!(
            org_id = %installation.org_id,
            "slack resolve: org has no workspaces"
        );
        return Ok(Resolution::NoWorkspaces(
            NoWorkspacesReason::OrgHasNoWorkspaces {
                org_id: installation.org_id,
            },
        ));
    }

    tracing::debug!(
        oxy_user_id = %user_link.oxy_user_id,
        org_id = %installation.org_id,
        workspace_count = user_workspaces.len(),
        "slack resolve: found workspaces for user"
    );

    // 1. Check per-channel default first — it takes priority over user prefs so
    //    a channel admin can pin a workspace for everyone in that channel.
    if !channel_id.is_empty()
        && let Some(channel_default) =
            ChannelDefaultsService::get(installation.id, channel_id).await?
        && let Some(ws) = user_workspaces
            .iter()
            .find(|w| w.id == channel_default.workspace_id)
    {
        tracing::debug!(
            channel_id,
            workspace_id = %ws.id,
            "slack resolve: using channel default"
        );
        return resolve_for_workspace(ws).await;
    }

    // 2. Honor user preferences if present and the workspace is still valid.
    if let Some(prefs) = UserPreferencesService::get(user_link.id).await?
        && let Some(ws_id) = prefs.default_workspace_id
        && let Some(ws) = user_workspaces.iter().find(|w| w.id == ws_id)
    {
        return resolve_for_workspace(ws).await;
    }

    // 3. Single workspace → use it silently.
    if user_workspaces.len() == 1 {
        return resolve_for_workspace(&user_workspaces[0]).await;
    }

    // 4. Multiple workspaces and no preference resolved — ask the user to pick.
    tracing::debug!(
        oxy_user_id = %user_link.oxy_user_id,
        workspace_count = user_workspaces.len(),
        "slack resolve: multiple workspaces, no preference — returning PickerNeeded"
    );
    let summaries = build_summaries_from_rows(&user_workspaces).await;
    Ok(Resolution::PickerNeeded {
        workspaces: summaries,
    })
}

/// Cheap COUNT of workspaces visible to an org. Used to decide whether to
/// emit the "Wrong workspace?" footer button — no point showing it when
/// there's nothing to switch to.
pub async fn count_org_workspaces(org_id: Uuid) -> Result<usize, OxyError> {
    use sea_orm::PaginatorTrait;
    let conn = establish_connection().await?;
    Workspaces::find()
        .filter(workspaces::Column::OrgId.eq(Some(org_id)))
        .count(&conn)
        .await
        .map(|n| n as usize)
        .map_err(|e| OxyError::DBError(e.to_string()))
}

/// Build workspace summaries for the picker UI (used by the Switch-workspace flow).
pub async fn build_workspace_summaries(
    installation: &InstallationRow,
) -> Result<Vec<WorkspaceSummary>, OxyError> {
    let conn = establish_connection().await?;
    let workspaces = Workspaces::find()
        .filter(workspaces::Column::OrgId.eq(Some(installation.org_id)))
        .all(&conn)
        .await
        .map_err(|e| OxyError::DBError(e.to_string()))?;

    Ok(build_summaries_from_rows(&workspaces).await)
}

/// Build summaries from a slice of workspace rows (without re-querying the DB).
async fn build_summaries_from_rows(workspaces: &[workspaces::Model]) -> Vec<WorkspaceSummary> {
    let mut summaries = Vec::with_capacity(workspaces.len());
    for ws in workspaces {
        let WorkspaceAgents { agents, default } =
            read_workspace_agents(ws.id).await.unwrap_or_default();
        summaries.push(WorkspaceSummary {
            id: ws.id,
            name: ws.name.clone(),
            agents,
            default_agent: default,
        });
    }
    summaries
}

/// Build a `Resolution` for a known workspace row: pick its preferred
/// agent (configured default, falling back to alphabetical-first), or
/// flag `WorkspaceHasNoAgents` if the workspace is empty. Shared by the
/// channel-default / user-preference / single-workspace branches above.
async fn resolve_for_workspace(ws: &workspaces::Model) -> Result<Resolution, OxyError> {
    match pick_default_agent_path(ws.id).await? {
        Some(agent_path) => Ok(Resolution::Resolved {
            workspace_id: ws.id,
            agent_path,
        }),
        None => Ok(Resolution::WorkspaceHasNoAgents {
            workspace_id: ws.id,
            workspace_name: ws.name.clone(),
        }),
    }
}

/// Result of inspecting a workspace's agents: all agent paths plus the
/// configured `defaults.agent` (when set in the workspace's `config.yml`).
/// Built once per workspace so callers don't pay for two
/// `WorkspaceBuilder::build()` round-trips.
#[derive(Debug, Default, Clone)]
pub struct WorkspaceAgents {
    pub agents: Vec<String>,
    pub default: Option<String>,
}

/// Read the agent inventory for a workspace: every agent path (relative to
/// workspace root) plus the configured default (if any).
pub async fn read_workspace_agents(workspace_id: Uuid) -> Result<WorkspaceAgents, OxyError> {
    let path = resolve_workspace_path(workspace_id).await?;
    let wm = WorkspaceBuilder::new(workspace_id)
        .with_workspace_path_and_fallback_config(&path)
        .await?
        .build()
        .await?;

    let default = wm.config_manager.default_agent_ref().cloned();
    let workspace_path = wm.config_manager.workspace_path().to_path_buf();
    let absolute_agents = wm.config_manager.list_agents().await.unwrap_or_default();
    let agents = absolute_agents
        .iter()
        .map(|abs| {
            abs.strip_prefix(&workspace_path)
                .unwrap_or(abs.as_path())
                .to_string_lossy()
                .to_string()
        })
        .collect();

    Ok(WorkspaceAgents { agents, default })
}

/// Returns all agent paths for a workspace as relative paths
/// (e.g. `"agents/foo.agent.yml"`), stripped of the workspace root prefix.
pub async fn list_agents(workspace_id: Uuid) -> Result<Vec<String>, OxyError> {
    Ok(read_workspace_agents(workspace_id).await?.agents)
}

/// Pick the preferred agent path for dispatching a Slack query against
/// `workspace_id`. Honors `defaults.agent` from `config.yml` when set;
/// falls back to the alphabetically-first agent otherwise.
/// Returns `None` only when the workspace has no agents at all.
pub async fn pick_default_agent_path(workspace_id: Uuid) -> Result<Option<String>, OxyError> {
    let WorkspaceAgents {
        mut agents,
        default,
    } = read_workspace_agents(workspace_id).await?;
    if default.is_some() {
        return Ok(default);
    }
    agents.sort();
    Ok(agents.into_iter().next())
}
