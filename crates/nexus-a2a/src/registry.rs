//! Agent Card registry — stores discovered remote agents and the local agent card.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::error::A2aError;
use crate::types::AgentCard;

/// Manages the set of known A2A peers (by their Agent Card URL) and the
/// local server's own Agent Card.
#[derive(Clone)]
pub struct AgentCardRegistry {
    inner: Arc<RwLock<RegistryInner>>,
}

struct RegistryInner {
    local_card: Option<AgentCard>,
    remote_cards: HashMap<String, AgentCard>,
}

impl AgentCardRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(RegistryInner {
                local_card: None,
                remote_cards: HashMap::new(),
            })),
        }
    }

    /// Set the local agent card (published at `/.well-known/agent.json`).
    pub async fn set_local(&self, card: AgentCard) {
        let mut inner = self.inner.write().await;
        info!(name = %card.name, url = %card.url, "Local A2A agent card set");
        inner.local_card = Some(card);
    }

    /// Get the local agent card.
    pub async fn local_card(&self) -> Option<AgentCard> {
        self.inner.read().await.local_card.clone()
    }

    /// Register a remote agent card (from discovery or manual registration).
    pub async fn register_remote(&self, card: AgentCard) {
        let mut inner = self.inner.write().await;
        info!(name = %card.name, url = %card.url, "Registered remote A2A agent");
        inner.remote_cards.insert(card.url.clone(), card);
    }

    /// Remove a remote agent by URL.
    pub async fn remove_remote(&self, url: &str) -> Result<(), A2aError> {
        let mut inner = self.inner.write().await;
        if inner.remote_cards.remove(url).is_none() {
            return Err(A2aError::TaskNotFound(format!("No remote agent at {url}")));
        }
        Ok(())
    }

    /// List all remote agents.
    pub async fn list_remote(&self) -> Vec<AgentCard> {
        self.inner
            .read()
            .await
            .remote_cards
            .values()
            .cloned()
            .collect()
    }

    /// Get a remote agent card by URL.
    pub async fn get_remote(&self, url: &str) -> Option<AgentCard> {
        self.inner.read().await.remote_cards.get(url).cloned()
    }

    /// Fetch and register a remote agent card by fetching `{base_url}/.well-known/agent.json`.
    ///
    /// SECURITY: defeats DNS rebinding. The fetch resolves the hostname once,
    /// validates every resolved IP against the SSRF block list, then pins the
    /// reqwest client to those IPs via `resolve_to_addrs`. An attacker that
    /// registers `evil.com` (resolving public) and then flips DNS to
    /// `169.254.169.254` between registration and fetch will still hit the
    /// validated public IP — the pinned client ignores the rebinding.
    pub async fn discover(&self, base_url: &str) -> Result<AgentCard, A2aError> {
        let url = format!(
            "{}/{}",
            base_url.trim_end_matches('/'),
            ".well-known/agent.json"
        );

        // Resolve + validate.
        let (host, addrs) = resolve_and_validate(&url).await?;

        // Build a one-shot client whose DNS is pinned to the validated IPs.
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .resolve_to_addrs(&host, &addrs)
            .build()
            .map_err(|e| A2aError::Internal(format!("pinned client build failed: {e}")))?;

        let card: AgentCard = client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        self.register_remote(card.clone()).await;
        Ok(card)
    }
}

/// Parse a URL, resolve its host via DNS, and reject if any resolved IP is in
/// the SSRF block list. Returns `(host, validated_socket_addrs)`.
///
/// Block list mirrors `nexus-http::security::url_guard`:
/// - non-http(s) schemes
/// - loopback / link-local / RFC1918 / unspecified
/// - 169.254.169.254 (cloud metadata) is link-local so it's already covered
/// - IPv6 loopback / link-local / unique-local
async fn resolve_and_validate(
    raw: &str,
) -> Result<(String, Vec<std::net::SocketAddr>), A2aError> {
    use std::net::IpAddr;

    let parsed = url::Url::parse(raw)
        .map_err(|e| A2aError::InvalidAgentResponse(format!("invalid URL: {e}")))?;

    match parsed.scheme() {
        "http" | "https" => {}
        other => {
            return Err(A2aError::InvalidAgentResponse(format!(
                "scheme '{other}' not allowed; must be http or https"
            )))
        }
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| A2aError::InvalidAgentResponse("URL has no host".into()))?
        .to_ascii_lowercase();
    if host == "localhost"
        || host.ends_with(".localhost")
        || host == "metadata.google.internal"
        || host == "metadata"
    {
        return Err(A2aError::InvalidAgentResponse(format!(
            "host '{host}' is blocked"
        )));
    }
    let port = parsed.port_or_known_default().ok_or_else(|| {
        A2aError::InvalidAgentResponse(format!("URL {raw} has no port and unknown default"))
    })?;

    // IPv6 literals come back wrapped in brackets from url::Url::host_str().
    let host_for_ip = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(&host);
    if let Ok(ip) = host_for_ip.parse::<IpAddr>() {
        check_ip(ip)?;
        return Ok((host, vec![std::net::SocketAddr::new(ip, port)]));
    }

    let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host.as_str(), port))
        .await
        .map_err(|e| {
            A2aError::InvalidAgentResponse(format!("DNS resolution failed for {host}: {e}"))
        })?
        .collect();
    if addrs.is_empty() {
        return Err(A2aError::InvalidAgentResponse(format!(
            "DNS returned no addresses for {host}"
        )));
    }
    for sa in &addrs {
        check_ip(sa.ip())?;
    }
    Ok((host, addrs))
}

fn check_ip(ip: std::net::IpAddr) -> Result<(), A2aError> {
    use std::net::{IpAddr, Ipv4Addr};
    match ip {
        IpAddr::V4(v4) => {
            if v4.is_loopback() {
                return Err(A2aError::InvalidAgentResponse(format!(
                    "loopback address {v4} is blocked"
                )));
            }
            if v4.is_private() {
                return Err(A2aError::InvalidAgentResponse(format!(
                    "private address {v4} is blocked"
                )));
            }
            if v4.is_link_local() {
                return Err(A2aError::InvalidAgentResponse(format!(
                    "link-local address {v4} is blocked"
                )));
            }
            if v4.is_unspecified() {
                return Err(A2aError::InvalidAgentResponse(format!(
                    "unspecified address {v4} is blocked"
                )));
            }
            if v4.is_broadcast() {
                return Err(A2aError::InvalidAgentResponse(format!(
                    "broadcast address {v4} is blocked"
                )));
            }
            let o = v4.octets();
            // 100.64.0.0/10 — carrier-grade NAT
            if o[0] == 100 && (o[1] & 0xC0) == 0x40 {
                return Err(A2aError::InvalidAgentResponse(format!(
                    "CGNAT address {v4} is blocked"
                )));
            }
            // 169.254.169.254 — cloud metadata (already link-local but explicit)
            if v4 == Ipv4Addr::new(169, 254, 169, 254) {
                return Err(A2aError::InvalidAgentResponse(format!(
                    "cloud metadata address {v4} is blocked"
                )));
            }
            Ok(())
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() {
                return Err(A2aError::InvalidAgentResponse(format!(
                    "IPv6 loopback {v6} is blocked"
                )));
            }
            if v6.is_unspecified() {
                return Err(A2aError::InvalidAgentResponse(format!(
                    "IPv6 unspecified {v6} is blocked"
                )));
            }
            let seg = v6.segments();
            if (seg[0] & 0xffc0) == 0xfe80 {
                return Err(A2aError::InvalidAgentResponse(format!(
                    "IPv6 link-local {v6} is blocked"
                )));
            }
            if (seg[0] & 0xfe00) == 0xfc00 {
                return Err(A2aError::InvalidAgentResponse(format!(
                    "IPv6 unique-local {v6} is blocked"
                )));
            }
            // IPv4-mapped IPv6 — re-check as v4.
            if seg[0] == 0
                && seg[1] == 0
                && seg[2] == 0
                && seg[3] == 0
                && seg[4] == 0
                && seg[5] == 0xffff
            {
                let v4 = Ipv4Addr::new(
                    (seg[6] >> 8) as u8,
                    (seg[6] & 0xff) as u8,
                    (seg[7] >> 8) as u8,
                    (seg[7] & 0xff) as u8,
                );
                return check_ip(IpAddr::V4(v4));
            }
            Ok(())
        }
    }
}

impl Default for AgentCardRegistry {
    fn default() -> Self {
        Self::new()
    }
}
