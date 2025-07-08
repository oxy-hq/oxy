use crate::errors::OxyError;
use crate::github::{auth::GitHubEnv, types::*};
use reqwest::{
    Client,
    header::{AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT},
};
use serde_json::Value;

/// GitHub API client for repository operations
pub struct GitHubClient {
    client: Client,
    base_url: String,
}

impl GitHubClient {
    /// Create a new GitHub client with the provided token
    pub fn new(token: String) -> Result<Self, OxyError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token))
                .map_err(|e| OxyError::RuntimeError(format!("Invalid token format: {}", e)))?,
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
            .map_err(|e| OxyError::RuntimeError(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            client,
            base_url: GitHubEnv::github_api_url(),
        })
    }

    /// Test the GitHub token by fetching user information
    pub async fn validate_token(&self) -> Result<(), OxyError> {
        let url = format!("{}/user", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to validate token: {}", e)))?;

        if !response.status().is_success() {
            return Err(OxyError::RuntimeError(format!(
                "GitHub API error: {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        Ok(())
    }

    /// List repositories accessible to the authenticated user
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

    /// Get authenticated user's repositories with pagination
    pub async fn list_repositories_paginated(
        &self,
        page: u32,
        per_page: u32,
    ) -> Result<Vec<GitHubRepository>, OxyError> {
        let url = format!(
            "{}/user/repos?sort=updated&page={}&per_page={}",
            self.base_url, page, per_page
        );

        let response =
            self.client.get(&url).send().await.map_err(|e| {
                OxyError::RuntimeError(format!("Failed to fetch repositories: {}", e))
            })?;

        if !response.status().is_success() {
            return Err(OxyError::RuntimeError(format!(
                "GitHub API error: {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let repos: Vec<GitHubRepository> = response.json().await.map_err(|e| {
            OxyError::RuntimeError(format!("Failed to parse repositories response: {}", e))
        })?;

        Ok(repos)
    }

    /// Get repository details by ID
    pub async fn get_repository(&self, repo_id: i64) -> Result<GitHubRepository, OxyError> {
        // Directly get the repository by ID from the GitHub API
        let url = format!("{}/repositories/{}", self.base_url, repo_id);

        let response =
            self.client.get(&url).send().await.map_err(|e| {
                OxyError::RuntimeError(format!("Failed to fetch repository: {}", e))
            })?;

        if !response.status().is_success() {
            return Err(OxyError::RuntimeError(format!(
                "GitHub API error: {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let repo: GitHubRepository = response.json().await.map_err(|e| {
            OxyError::RuntimeError(format!("Failed to parse repository response: {}", e))
        })?;

        Ok(repo)
    }

    /// Get the latest commit hash from the default branch of a repository
    pub async fn get_latest_commit_hash(&self, repo_id: i64) -> Result<String, OxyError> {
        let repo = self.get_repository(repo_id).await?;
        self.get_branch_commit_hash(repo_id, &repo.default_branch)
            .await
    }

    /// Get the latest commit hash from a specific branch
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
            .map_err(|e| OxyError::RuntimeError(format!("Failed to fetch branch: {}", e)))?;

        if response.status().is_success() {
            let branch_data: Value = response.json().await.map_err(|e| {
                OxyError::RuntimeError(format!("Failed to parse branch response: {}", e))
            })?;

            // Extract commit SHA from the response
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
                "Failed to fetch branch commit: HTTP {} - {}",
                status, error_text
            )))
        }
    }

    /// Get detailed commit information by SHA
    pub async fn get_commit_details(
        &self,
        repo_id: i64,
        commit_sha: &str,
    ) -> Result<CommitInfo, OxyError> {
        let url = format!(
            "{}/repositories/{}/commits/{}",
            self.base_url, repo_id, commit_sha
        );

        let response = self.client.get(&url).send().await.map_err(|e| {
            OxyError::RuntimeError(format!("Failed to fetch commit details: {}", e))
        })?;

        if response.status().is_success() {
            let commit_data: Value = response.json().await.map_err(|e| {
                OxyError::RuntimeError(format!("Failed to parse commit response: {}", e))
            })?;

            // Extract commit information from the response
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
                "Failed to fetch commit details: HTTP {} - {}",
                status, error_text
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = GitHubClient::new("test_token".to_string());
        assert!(client.is_ok());
    }
}
