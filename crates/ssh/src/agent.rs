//! SSH agent authentication.
//!
//! Talks to the local ssh-agent (`SSH_AUTH_SOCK` on Unix, Pageant /
//! OpenSSH named pipe on Windows) and tries every identity it offers.
//!
//! Author: Passwd Word

use russh::auth::Signer;
use russh::keys::HashAlg;
use russh::keys::agent::AgentIdentity;
use russh::keys::agent::client::{AgentClient, AgentStream};

use crate::{ClientHandle, SshError};

/// [`Signer`] adapter that asks the ssh-agent to sign on behalf of an identity.
struct AgentSigner<'a, S: AgentStream> {
    agent: &'a mut AgentClient<S>,
    identity: &'a AgentIdentity,
}

impl<S: AgentStream + Send> Signer for AgentSigner<'_, S> {
    type Error = SshError;

    async fn auth_sign(
        &mut self,
        _key: &AgentIdentity,
        hash_alg: Option<HashAlg>,
        to_sign: Vec<u8>,
    ) -> Result<Vec<u8>, Self::Error> {
        self.agent
            .sign_request(self.identity, hash_alg, to_sign)
            .await
            .map_err(|e| SshError::Agent(format!("agent signing failed: {e}")))
    }
}

/// Authenticate `username` against the local ssh-agent.
///
/// Tries every public-key identity the agent offers until one is accepted.
/// Certificate identities are skipped (not supported for agent auth here).
pub async fn authenticate_agent(handle: &mut ClientHandle, username: &str) -> Result<(), SshError> {
    #[cfg(unix)]
    let mut agent = AgentClient::connect_env()
        .await
        .map_err(|e| SshError::Agent(format!("cannot reach ssh-agent (SSH_AUTH_SOCK): {e}")))?;

    #[cfg(windows)]
    let mut agent = AgentClient::connect_pageant()
        .await
        .map_err(|e| SshError::Agent(format!("cannot reach Pageant: {e}")))?;

    #[cfg(not(any(unix, windows)))]
    return Err(SshError::Agent(
        "ssh-agent is not supported on this platform".into(),
    ));

    let identities = agent
        .request_identities()
        .await
        .map_err(|e| SshError::Agent(format!("agent refused to list identities: {e}")))?;

    if identities.is_empty() {
        return Err(SshError::Agent("ssh-agent has no identities loaded".into()));
    }

    let mut tried = 0usize;
    for identity in &identities {
        let key = match identity {
            AgentIdentity::PublicKey { key, .. } => key.clone(),
            AgentIdentity::Certificate { .. } => continue,
        };
        tried += 1;
        let mut signer = AgentSigner {
            agent: &mut agent,
            identity,
        };
        match handle
            .authenticate_publickey_with(username, key, None, &mut signer)
            .await
        {
            Ok(result) if result.success() => {
                tracing::info!("ssh-agent authentication succeeded");
                return Ok(());
            }
            Ok(_) => continue, // server said no; try the next identity
            Err(e) => return Err(e),
        }
    }

    Err(SshError::Auth(format!(
        "ssh-agent offered no acceptable identity (tried {tried})"
    )))
}
