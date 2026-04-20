use std::collections::HashMap;
use std::path::Path;

use oxy_shared::errors::OxyError;
use tracing::info;

use crate::cli::run;
use crate::types::FileStatus;

/// Return the combined `git status --short` + `git diff --numstat` summary
/// for the working tree at `repo_path`.
pub async fn numstat_summary(repo_path: &Path) -> Result<Vec<FileStatus>, OxyError> {
    if !repo_path.exists() {
        return Err(OxyError::RuntimeError(format!(
            "Repository directory does not exist: {}",
            repo_path.display()
        )));
    }

    let status_str = run::run(repo_path, &["status", "--short", "--untracked-files=all"]).await?;
    let diff_str = run::run(repo_path, &["diff", "--numstat"]).await?;

    let mut diff_stats: HashMap<String, (u32, u32)> = HashMap::new();
    for line in diff_str.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let insertions = parts[0].parse::<u32>().unwrap_or(0);
            let deletions = parts[1].parse::<u32>().unwrap_or(0);
            let file_path = parts[2..].join(" ");
            diff_stats.insert(file_path, (insertions, deletions));
        }
    }

    let mut result = Vec::new();
    for line in status_str.lines() {
        if line.trim().is_empty() || line.len() < 3 {
            continue;
        }
        let status_chars = &line[0..2];
        let file_path = line[3..].trim().to_string();

        let status = match status_chars {
            "M " | " M" | "MM" => "M",
            "A " | " A" | "AM" => "A",
            "D " | " D" | "AD" => "D",
            "R " | " R" => "M",
            "C " | " C" => "A",
            "??" => "A",
            "UU" | "AA" | "DD" | "AU" | "UA" | "DU" | "UD" => "U",
            _ => "M",
        }
        .to_string();

        let (insert, delete) = diff_stats.get(&file_path).unwrap_or(&(0, 0));
        result.push(FileStatus {
            path: file_path,
            status,
            insert: *insert,
            delete: *delete,
        });
    }

    Ok(result)
}

/// Returns file-level insert/delete counts for commits that are ahead of
/// the configured upstream (`@{upstream}...HEAD`).
///
/// Returns an empty vec when no upstream is configured (the branch has
/// never been pushed) rather than propagating an error.
pub async fn numstat_ahead(root: &Path) -> Result<Vec<FileStatus>, OxyError> {
    let range = "@{upstream}...HEAD";

    let numstat = match run::run(root, &["diff", "--numstat", range]).await {
        Ok(out) => out,
        Err(_) => return Ok(vec![]),
    };
    let name_status = match run::run(root, &["diff", "--name-status", range]).await {
        Ok(out) => out,
        Err(_) => return Ok(vec![]),
    };

    let mut stat_map: HashMap<String, (u32, u32)> = HashMap::new();
    for line in numstat.trim().lines() {
        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        if parts.len() >= 3 {
            let ins = parts[0].trim().parse::<u32>().unwrap_or(0);
            let del = parts[1].trim().parse::<u32>().unwrap_or(0);
            stat_map.insert(parts[2].trim().to_string(), (ins, del));
        }
    }

    let mut result = Vec::new();
    for line in name_status.trim().lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut cols = line.splitn(3, '\t');
        let status_char = cols.next().unwrap_or("").trim();
        let path = match status_char.chars().next() {
            Some('R') | Some('C') => {
                cols.next();
                cols.next().unwrap_or("").trim().to_string()
            }
            _ => cols.next().unwrap_or("").trim().to_string(),
        };
        if path.is_empty() {
            continue;
        }
        let status = match status_char.chars().next() {
            Some('A') => "A",
            Some('D') => "D",
            _ => "M",
        }
        .to_string();
        let (ins, del) = stat_map.get(&path).copied().unwrap_or((0, 0));
        result.push(FileStatus {
            path,
            status,
            insert: ins,
            delete: del,
        });
    }
    Ok(result)
}

/// Return the contents of `file_path` at `commit` (defaults to `HEAD`).
pub async fn file_at_rev(
    repo_path: &Path,
    file_path: &str,
    commit: Option<&str>,
) -> Result<String, OxyError> {
    if !repo_path.exists() {
        return Err(OxyError::RuntimeError(format!(
            "Repository directory does not exist: {}",
            repo_path.display()
        )));
    }

    let commit_ref = commit.unwrap_or("HEAD");
    let show_ref = format!("{}:{}", commit_ref, file_path);

    info!(
        "Getting file content for '{}' from commit '{}' in repository at {}",
        file_path,
        commit_ref,
        repo_path.display()
    );

    run::run(repo_path, &["show", &show_ref]).await
}
