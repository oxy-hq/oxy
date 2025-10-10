use crate::errors::OxyError;
use crate::github::{auth::GitHubEnv, types::*};
use reqwest::{
    Client,
    header::{AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT},
};
use serde_json::Value;

pub struct GitHubClient {
    client: Client,
    base_url: String,
}

impl GitHubClient {
    pub fn from_token(token: String) -> Result<Self, OxyError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|e| OxyError::RuntimeError(format!("Invalid token format: {e}")))?,
        );
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("Oxy-GitHub-Integration/1.0"),
        );
        headers.insert(
            "Accept",
            HeaderValue::from_static("application/vnd.github.v3+json"),
        );

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| OxyError::RuntimeError(format!("Failed to create HTTP client: {e}")))?;

        Ok(Self {
            client,
            base_url: GitHubEnv::github_api_url(),
        })
    }

    pub async fn validate_connection(&self) -> Result<(), OxyError> {
        let url = format!("{}/installation", self.base_url);

        let response = self.client.get(&url).send().await.map_err(|e| {
            OxyError::RuntimeError(format!("Failed to validate GitHub connection: {e}"))
        })?;

        if !response.status().is_success() {
            return Err(OxyError::RuntimeError(format!(
                "GitHub API error: {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        Ok(())
    }

    pub async fn list_repositories(&self) -> Result<Vec<GitHubRepository>, OxyError> {
        let mut all_repos = Vec::new();
        let mut page = 1;
        let per_page = 100;

        loop {
            let repos = self.list_repositories_paginated(page, per_page).await?;
            let is_last_page = repos.len() < per_page as usize;

            all_repos.extend(repos);

            if is_last_page {
                break;
            }

            page += 1;
        }

        Ok(all_repos)
    }

    pub async fn list_branches(
        &self,
        full_repo_name: String,
    ) -> Result<Vec<GitHubBranch>, OxyError> {
        let mut all_branches = Vec::new();
        let mut page = 1;
        let per_page = 100;

        loop {
            let branches = self
                .list_branches_paginated(full_repo_name.clone(), page, per_page)
                .await?;
            let is_last_page = branches.len() < per_page as usize;

            all_branches.extend(branches);

            if is_last_page {
                break;
            }

            page += 1;
        }

        Ok(all_branches)
    }

    pub async fn list_repositories_paginated(
        &self,
        page: u32,
        per_page: u32,
    ) -> Result<Vec<GitHubRepository>, OxyError> {
        let url = format!(
            "{}/installation/repositories?sort=updated&page={}&per_page={}",
            self.base_url, page, per_page
        );

        let response =
            self.client.get(&url).send().await.map_err(|e| {
                OxyError::RuntimeError(format!("Failed to fetch repositories: {e}"))
            })?;

        if !response.status().is_success() {
            return Err(OxyError::RuntimeError(format!(
                "GitHub API error: {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        // The installation repositories endpoint returns a different structure with repositories nested
        #[derive(serde::Deserialize)]
        struct InstallationRepositoriesResponse {
            repositories: Vec<GitHubRepository>,
        }

        let response_data: InstallationRepositoriesResponse =
            response.json().await.map_err(|e| {
                OxyError::RuntimeError(format!("Failed to parse repositories response: {e}"))
            })?;

        Ok(response_data.repositories)
    }

    pub async fn list_branches_paginated(
        &self,
        full_repo_name: String,
        page: u32,
        per_page: u32,
    ) -> Result<Vec<GitHubBranch>, OxyError> {
        let url = format!(
            "{}/repos/{}/branches?page={}&per_page={}",
            self.base_url, full_repo_name, page, per_page
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to fetch branches: {e}")))?;

        if !response.status().is_success() {
            return Err(OxyError::RuntimeError(format!(
                "GitHub API error: {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let branches: Vec<GitHubBranch> = response.json().await.map_err(|e| {
            OxyError::RuntimeError(format!("Failed to parse branches response: {e}"))
        })?;

        Ok(branches)
    }

    pub async fn get_repository(&self, repo_id: i64) -> Result<GitHubRepository, OxyError> {
        let url = format!("{}/repositories/{}", self.base_url, repo_id);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to fetch repository: {e}")))?;

        if !response.status().is_success() {
            return Err(OxyError::RuntimeError(format!(
                "GitHub API error: {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let repo: GitHubRepository = response.json().await.map_err(|e| {
            OxyError::RuntimeError(format!("Failed to parse repository response: {e}"))
        })?;

        Ok(repo)
    }

    pub async fn get_latest_commit_hash(&self, repo_id: i64) -> Result<String, OxyError> {
        let repo = self.get_repository(repo_id).await?;
        self.get_branch_commit_hash(repo_id, &repo.default_branch)
            .await
    }

    pub async fn get_branch_commit_hash(
        &self,
        repo_id: i64,
        branch: &str,
    ) -> Result<String, OxyError> {
        let url = format!(
            "{}/repositories/{}/branches/{}",
            self.base_url, repo_id, branch
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to fetch branch: {e}")))?;

        if response.status().is_success() {
            let branch_data: Value = response.json().await.map_err(|e| {
                OxyError::RuntimeError(format!("Failed to parse branch response: {e}"))
            })?;

            let commit_sha = branch_data
                .get("commit")
                .and_then(|c| c.get("sha"))
                .and_then(|s| s.as_str())
                .ok_or_else(|| {
                    OxyError::RuntimeError("No commit SHA found in response".to_string())
                })?;

            Ok(commit_sha.to_string())
        } else {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            Err(OxyError::RuntimeError(format!(
                "Failed to fetch branch commit: HTTP {status} - {error_text}"
            )))
        }
    }

    pub async fn get_commit_details(
        &self,
        repo_id: i64,
        commit_sha: &str,
    ) -> Result<CommitInfo, OxyError> {
        let url = format!(
            "{}/repositories/{}/commits/{}",
            self.base_url, repo_id, commit_sha
        );

        let response =
            self.client.get(&url).send().await.map_err(|e| {
                OxyError::RuntimeError(format!("Failed to fetch commit details: {e}"))
            })?;

        if response.status().is_success() {
            let commit_data: Value = response.json().await.map_err(|e| {
                OxyError::RuntimeError(format!("Failed to parse commit response: {e}"))
            })?;

            let sha = commit_data
                .get("sha")
                .and_then(|s| s.as_str())
                .unwrap_or(commit_sha)
                .to_string();

            let message = commit_data
                .get("commit")
                .and_then(|c| c.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("No commit message")
                .to_string();

            let author_name = commit_data
                .get("commit")
                .and_then(|c| c.get("author"))
                .and_then(|a| a.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("Unknown")
                .to_string();

            let author_email = commit_data
                .get("commit")
                .and_then(|c| c.get("author"))
                .and_then(|a| a.get("email"))
                .and_then(|e| e.as_str())
                .unwrap_or("")
                .to_string();

            let date = commit_data
                .get("commit")
                .and_then(|c| c.get("author"))
                .and_then(|a| a.get("date"))
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .to_string();

            Ok(CommitInfo {
                sha,
                message,
                author_name,
                author_email,
                date,
            })
        } else {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            Err(OxyError::RuntimeError(format!(
                "Failed to fetch commit details: HTTP {status} - {error_text}"
            )))
        }
    }

    pub async fn create_repository(
        &self,
        repo_name: &str,
        description: Option<&str>,
        private: Option<bool>,
        owner_type: Option<&str>,
        owner: Option<&str>,
    ) -> Result<GitHubRepository, OxyError> {
        let url = if owner_type == Some("User") {
            format!("{}/user/repos", self.base_url)
        } else {
            format!("{}/orgs/{}/repos", self.base_url, owner.unwrap())
        };

        let mut payload = serde_json::json!({
            "name": repo_name,
        });

        if let Some(desc) = description {
            payload["description"] = serde_json::Value::String(desc.to_string());
        }

        if let Some(is_private) = private {
            payload["private"] = serde_json::Value::Bool(is_private);
        }

        let response = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to create repository: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            return Err(OxyError::RuntimeError(format!(
                "GitHub API error creating repository: HTTP {status} - {error_text}. Note: For user accounts with GitHub Apps, the app must have 'Contents' and 'Administration' repository permissions, and the user account must allow the app to create repositories."
            )));
        }

        let repo: GitHubRepository = response.json().await.map_err(|e| {
            OxyError::RuntimeError(format!("Failed to parse repository creation response: {e}"))
        })?;

        Ok(repo)
    }
}
