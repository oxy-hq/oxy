//! In-memory provider shaped like Neon. **Provisions nothing real.**
//!
//! Deterministic by construction: ids and passwords come from a counter, not a
//! clock or an RNG, so tests assert on exact values and stay reproducible.
//!
//! Supports fault injection ([`MockProvider::push_fault`]) so the provisioner's
//! failure semantics — partial provision, retry, reconcile — are testable
//! without a live provider that can be made to fail on demand.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use async_trait::async_trait;

use super::types::{Branch, CreateProjectRequest, DatabaseInfo, Project, Role};
use super::{OltpProvider, ProviderError};

/// Key for a role within a project branch.
type RoleKey = (String, String, String);

#[derive(Default)]
struct State {
    /// project id → project
    projects: HashMap<String, Project>,
    /// project name → project id, enforcing the provider's uniqueness rule
    names_taken: HashMap<String, String>,
    /// (project, branch, role) → current password
    roles: HashMap<RoleKey, String>,
    seq: u64,
    faults: VecDeque<ProviderError>,
}

impl State {
    fn next(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }

    /// Pop an injected fault, if the test queued one for this call.
    fn take_fault(&mut self) -> Option<ProviderError> {
        self.faults.pop_front()
    }
}

#[derive(Default)]
pub struct MockProvider {
    state: Mutex<State>,
}

impl MockProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a failure for the next provider call, whatever it is. Calls pop
    /// faults in FIFO order, so a test can script a sequence.
    pub fn push_fault(&self, err: ProviderError) {
        self.state.lock().expect("mock lock").faults.push_back(err);
    }

    pub fn project_count(&self) -> usize {
        self.state.lock().expect("mock lock").projects.len()
    }

    /// Role names on a branch, sorted — for assertions.
    pub fn role_names(&self, project_id: &str, branch_id: &str) -> Vec<String> {
        let state = self.state.lock().expect("mock lock");
        let mut names: Vec<String> = state
            .roles
            .keys()
            .filter(|(p, b, _)| p == project_id && b == branch_id)
            .map(|(_, _, r)| r.clone())
            .collect();
        names.sort();
        names
    }

    /// Current password for a role, if it exists. Test-only: a real provider
    /// never re-discloses this.
    pub fn peek_password(&self, project_id: &str, branch_id: &str, role: &str) -> Option<String> {
        let key = (
            project_id.to_string(),
            branch_id.to_string(),
            role.to_string(),
        );
        self.state
            .lock()
            .expect("mock lock")
            .roles
            .get(&key)
            .cloned()
    }
}

#[async_trait]
impl OltpProvider for MockProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    async fn create_project(&self, req: CreateProjectRequest) -> Result<Project, ProviderError> {
        let mut state = self.state.lock().expect("mock lock");
        if let Some(f) = state.take_fault() {
            return Err(f);
        }
        if state.names_taken.contains_key(&req.name) {
            return Err(ProviderError::ProjectNameTaken(req.name));
        }

        let n = state.next();
        let owner_name = super::OWNER_ROLE.to_string();
        let project = Project {
            id: format!("proj-{n}"),
            name: req.name.clone(),
            region_id: req.region_id,
            pg_version: req.pg_version,
            branch: Branch {
                id: format!("br-{n}"),
                name: "main".to_string(),
            },
            database: DatabaseInfo {
                name: "neondb".to_string(),
                owner_name: owner_name.clone(),
            },
            owner_role: Role {
                name: owner_name.clone(),
                password: Some(format!("mock-pw-{n}")),
            },
            host: format!("ep-{n}.mock.local"),
        };

        state.roles.insert(
            (project.id.clone(), project.branch.id.clone(), owner_name),
            format!("mock-pw-{n}"),
        );
        state.names_taken.insert(req.name, project.id.clone());
        // Stored redacted: re-reads must not re-disclose the owner password,
        // matching the real provider.
        let mut stored = project.clone();
        stored.owner_role = stored.owner_role.redacted();
        state.projects.insert(project.id.clone(), stored);

        Ok(project)
    }

    async fn get_project(&self, project_id: &str) -> Result<Option<Project>, ProviderError> {
        let mut state = self.state.lock().expect("mock lock");
        if let Some(f) = state.take_fault() {
            return Err(f);
        }
        Ok(state.projects.get(project_id).cloned())
    }

    async fn delete_project(&self, project_id: &str) -> Result<(), ProviderError> {
        let mut state = self.state.lock().expect("mock lock");
        if let Some(f) = state.take_fault() {
            return Err(f);
        }
        if let Some(p) = state.projects.remove(project_id) {
            state.names_taken.remove(&p.name);
            state.roles.retain(|(proj, _, _), _| proj != project_id);
        }
        // Idempotent: absent is success.
        Ok(())
    }

    async fn create_role(
        &self,
        project_id: &str,
        branch_id: &str,
        role_name: &str,
    ) -> Result<Role, ProviderError> {
        let mut state = self.state.lock().expect("mock lock");
        if let Some(f) = state.take_fault() {
            return Err(f);
        }
        if !state.projects.contains_key(project_id) {
            return Err(ProviderError::ProjectNotFound(project_id.to_string()));
        }
        let n = state.next();
        let password = format!("mock-pw-{n}");
        state.roles.insert(
            (
                project_id.to_string(),
                branch_id.to_string(),
                role_name.to_string(),
            ),
            password.clone(),
        );
        Ok(Role {
            name: role_name.to_string(),
            password: Some(password),
        })
    }

    async fn get_role(
        &self,
        project_id: &str,
        branch_id: &str,
        role_name: &str,
    ) -> Result<Option<Role>, ProviderError> {
        let mut state = self.state.lock().expect("mock lock");
        if let Some(f) = state.take_fault() {
            return Err(f);
        }
        let key = (
            project_id.to_string(),
            branch_id.to_string(),
            role_name.to_string(),
        );
        // Password deliberately withheld: the real provider only ever
        // discloses it at create/reset.
        Ok(state.roles.get(&key).map(|_| Role {
            name: role_name.to_string(),
            password: None,
        }))
    }

    async fn reset_role_password(
        &self,
        project_id: &str,
        branch_id: &str,
        role_name: &str,
    ) -> Result<Role, ProviderError> {
        let mut state = self.state.lock().expect("mock lock");
        if let Some(f) = state.take_fault() {
            return Err(f);
        }
        let key = (
            project_id.to_string(),
            branch_id.to_string(),
            role_name.to_string(),
        );
        if !state.roles.contains_key(&key) {
            return Err(ProviderError::RoleNotFound(
                role_name.to_string(),
                branch_id.to_string(),
            ));
        }
        let n = state.next();
        let password = format!("mock-pw-{n}");
        state.roles.insert(key, password.clone());
        Ok(Role {
            name: role_name.to_string(),
            password: Some(password),
        })
    }

    async fn delete_role(
        &self,
        project_id: &str,
        branch_id: &str,
        role_name: &str,
    ) -> Result<(), ProviderError> {
        let mut state = self.state.lock().expect("mock lock");
        if let Some(f) = state.take_fault() {
            return Err(f);
        }
        state.roles.remove(&(
            project_id.to_string(),
            branch_id.to_string(),
            role_name.to_string(),
        ));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(name: &str) -> CreateProjectRequest {
        CreateProjectRequest {
            name: name.to_string(),
            region_id: "aws-us-east-2".into(),
            // Deliberately NOT `DEFAULT_PG_VERSION`: this asserts the value
            // round-trips, which a number equal to the default could satisfy by
            // coincidence. The literals in the provider test doubles are meant
            // to differ from it — only `config.rs` carries the shipped default.
            pg_version: 17,
        }
    }

    #[tokio::test]
    async fn create_discloses_the_owner_password_exactly_once() {
        let p = MockProvider::new();
        let created = p.create_project(req("acme")).await.unwrap();
        assert!(created.owner_role.password.is_some());

        let refetched = p.get_project(&created.id).await.unwrap().unwrap();
        assert!(
            refetched.owner_role.password.is_none(),
            "re-reading a project must not re-disclose the owner password"
        );
    }

    #[tokio::test]
    async fn duplicate_name_is_rejected_not_adopted() {
        let p = MockProvider::new();
        p.create_project(req("acme")).await.unwrap();
        let err = p.create_project(req("acme")).await.unwrap_err();
        assert!(matches!(err, ProviderError::ProjectNameTaken(n) if n == "acme"));
        assert_eq!(
            p.project_count(),
            1,
            "the second create must not have landed"
        );
    }

    #[tokio::test]
    async fn delete_project_is_idempotent_and_frees_the_name() {
        let p = MockProvider::new();
        let proj = p.create_project(req("acme")).await.unwrap();
        p.delete_project(&proj.id).await.unwrap();
        p.delete_project(&proj.id).await.unwrap();
        assert_eq!(p.project_count(), 0);
        // Name is reusable once the project is gone.
        p.create_project(req("acme")).await.unwrap();
    }

    #[tokio::test]
    async fn deleting_a_project_takes_its_roles_with_it() {
        let p = MockProvider::new();
        let proj = p.create_project(req("acme")).await.unwrap();
        p.create_role(&proj.id, &proj.branch.id, "app_x_rw")
            .await
            .unwrap();
        p.delete_project(&proj.id).await.unwrap();
        assert!(p.role_names(&proj.id, &proj.branch.id).is_empty());
    }

    #[tokio::test]
    async fn get_role_never_returns_a_password() {
        let p = MockProvider::new();
        let proj = p.create_project(req("acme")).await.unwrap();
        p.create_role(&proj.id, &proj.branch.id, "app_x_rw")
            .await
            .unwrap();
        let role = p
            .get_role(&proj.id, &proj.branch.id, "app_x_rw")
            .await
            .unwrap()
            .unwrap();
        assert!(role.password.is_none());
    }

    #[tokio::test]
    async fn reset_changes_the_password() {
        let p = MockProvider::new();
        let proj = p.create_project(req("acme")).await.unwrap();
        let before = p
            .create_role(&proj.id, &proj.branch.id, "app_x_rw")
            .await
            .unwrap()
            .password
            .unwrap();
        let after = p
            .reset_role_password(&proj.id, &proj.branch.id, "app_x_rw")
            .await
            .unwrap()
            .password
            .unwrap();
        assert_ne!(before, after);
        assert_eq!(
            p.peek_password(&proj.id, &proj.branch.id, "app_x_rw"),
            Some(after)
        );
    }

    #[tokio::test]
    async fn reset_on_a_missing_role_errors_rather_than_creating_one() {
        let p = MockProvider::new();
        let proj = p.create_project(req("acme")).await.unwrap();
        let err = p
            .reset_role_password(&proj.id, &proj.branch.id, "nope")
            .await
            .unwrap_err();
        assert!(matches!(err, ProviderError::RoleNotFound(..)));
    }

    #[tokio::test]
    async fn create_role_on_a_missing_project_errors() {
        let p = MockProvider::new();
        let err = p.create_role("proj-nope", "br-1", "r").await.unwrap_err();
        assert!(matches!(err, ProviderError::ProjectNotFound(_)));
    }

    #[tokio::test]
    async fn delete_role_is_idempotent() {
        let p = MockProvider::new();
        let proj = p.create_project(req("acme")).await.unwrap();
        p.delete_role(&proj.id, &proj.branch.id, "ghost")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn injected_faults_pop_in_order() {
        let p = MockProvider::new();
        p.push_fault(ProviderError::RateLimited);
        p.push_fault(ProviderError::Transport("boom".into()));

        assert!(matches!(
            p.create_project(req("acme")).await.unwrap_err(),
            ProviderError::RateLimited
        ));
        assert!(matches!(
            p.create_project(req("acme")).await.unwrap_err(),
            ProviderError::Transport(_)
        ));
        // Queue drained — the third call succeeds.
        p.create_project(req("acme")).await.unwrap();
    }

    #[test]
    fn retryable_classification_matches_intent() {
        assert!(ProviderError::RateLimited.is_retryable());
        assert!(ProviderError::Transport("x".into()).is_retryable());
        assert!(
            ProviderError::Api {
                status: 503,
                message: "x".into()
            }
            .is_retryable()
        );
        assert!(!ProviderError::ProjectNameTaken("x".into()).is_retryable());
        assert!(
            !ProviderError::Api {
                status: 400,
                message: "x".into()
            }
            .is_retryable()
        );
    }
}
