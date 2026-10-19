//! Command snippets: saved commands runnable on any session.
//! Author: Passwd Word

use opsdeck_ssh::{ExecOutput, SessionId, SessionManager};
use opsdeck_storage::Storage;
use opsdeck_storage::models::{Snippet, new_id, now_unix};

use crate::error::OpsError;

/// Snippet management + execution.
pub struct SnippetService {
    storage: Storage,
}

impl SnippetService {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }

    /// Create a snippet.
    pub async fn create(
        &self,
        title: &str,
        command: &str,
        description: Option<&str>,
        host_id: Option<&str>,
        tags: &[&str],
    ) -> Result<Snippet, OpsError> {
        let now = now_unix();
        let snippet = Snippet {
            id: new_id(),
            title: title.to_string(),
            command: command.to_string(),
            description: description.map(str::to_string),
            host_id: host_id.map(str::to_string),
            tags: serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string()),
            created_at: now,
            updated_at: now,
        };
        self.storage.snippets().create(&snippet).await?;
        Ok(snippet)
    }

    pub async fn update(&self, snippet: &Snippet) -> Result<(), OpsError> {
        Ok(self.storage.snippets().update(snippet).await?)
    }

    pub async fn delete(&self, id: &str) -> Result<(), OpsError> {
        Ok(self.storage.snippets().delete(id).await?)
    }

    pub async fn list(&self) -> Result<Vec<Snippet>, OpsError> {
        Ok(self.storage.snippets().list_all().await?)
    }

    pub async fn search(&self, query: &str) -> Result<Vec<Snippet>, OpsError> {
        Ok(self.storage.snippets().search(query).await?)
    }

    /// Run a snippet's command on a session.
    pub async fn run(
        &self,
        ssh: &SessionManager,
        session: SessionId,
        snippet_id: &str,
    ) -> Result<ExecOutput, OpsError> {
        let snippet = self
            .storage
            .snippets()
            .get(snippet_id)
            .await
            .map_err(|_| OpsError::SnippetNotFound(snippet_id.to_string()))?;
        Ok(ssh.exec(session, &snippet.command).await?)
    }

    /// Seed the built-in snippet library on first run.
    pub async fn seed_defaults(&self) -> Result<(), OpsError> {
        if !self.storage.snippets().list_all().await?.is_empty() {
            return Ok(());
        }
        let defaults: &[(&str, &str, &str, &[&str])] = &[
            (
                "Disk usage",
                "df -h",
                "Human-readable disk usage",
                &["system"],
            ),
            (
                "Memory usage",
                "free -h",
                "Human-readable memory usage",
                &["system"],
            ),
            (
                "Top processes",
                "ps aux --sort=-%cpu | head -n 20",
                "Top CPU consumers",
                &["system"],
            ),
            (
                "Listening ports",
                "ss -tlnp",
                "Listening TCP ports",
                &["network"],
            ),
            ("System uptime", "uptime", "Uptime and load", &["system"]),
            (
                "Docker containers",
                "docker ps --format 'table {{.Names}}\\t{{.Status}}\\t{{.Ports}}'",
                "Running containers",
                &["docker"],
            ),
            (
                "Nginx reload",
                "sudo nginx -t && sudo systemctl reload nginx",
                "Test and reload nginx",
                &["web"],
            ),
        ];
        for (title, command, desc, tags) in defaults {
            self.create(title, command, Some(desc), None, tags).await?;
        }
        Ok(())
    }
}

/// Check whether a command looks dangerous (docs §6.8).
///
/// Used by batch execution and snippet runs to force an explicit user
/// confirmation. Conservative: matches well-known destructive patterns.
pub fn dangerous_reason(command: &str) -> Option<String> {
    let c = command.to_lowercase();
    // Normalize: collapse whitespace for pattern matching.
    let norm: String = c.split_whitespace().collect::<Vec<_>>().join(" ");
    let patterns: &[(&str, &str)] = &[
        ("rm -rf /", "recursive delete from filesystem root"),
        ("rm -rf ~", "recursive delete of home directory"),
        ("rm -rf .", "recursive delete of current directory"),
        ("mkfs", "filesystem formatting"),
        ("dd if=", "raw disk write via dd"),
        ("> /dev/sd", "raw disk write"),
        ("shutdown", "system shutdown"),
        ("poweroff", "system power off"),
        ("halt", "system halt"),
        ("reboot", "system reboot"),
        (":(){:|:&};:", "fork bomb"),
    ];
    for (pat, reason) in patterns {
        if norm.contains(pat) {
            return Some((*reason).to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dangerous_detection() {
        assert!(dangerous_reason("df -h").is_none());
        assert!(dangerous_reason("sudo rm -rf / --no-preserve-root").is_some());
        assert!(dangerous_reason("reboot").is_some());
        assert!(dangerous_reason("dd if=/dev/zero of=/dev/sda").is_some());
        assert!(dangerous_reason("echo hello").is_none());
    }

    #[tokio::test]
    async fn seed_defaults_idempotent() {
        let storage = Storage::open_in_memory().await.unwrap();
        let svc = SnippetService::new(storage);
        svc.seed_defaults().await.unwrap();
        let n = svc.list().await.unwrap().len();
        assert!(n > 0);
        svc.seed_defaults().await.unwrap();
        assert_eq!(svc.list().await.unwrap().len(), n);
    }
}
