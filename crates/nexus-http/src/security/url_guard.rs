//! SSRF guard — rejects URLs that would let a user pivot the server into
//! internal networks or metadata endpoints.
//!
//! Used by any handler that accepts a user-controlled URL and then makes the
//! server issue an outbound request to it (webhooks, federation peers, etc.).
//!
//! The guard is intentionally strict:
//! - Scheme MUST be `http` or `https`.
//! - Host MUST be present.
//! - IP literals in the following ranges are rejected:
//!     * loopback (127.0.0.0/8, ::1)
//!     * link-local (169.254.0.0/16, fe80::/10)
//!     * RFC1918 private (10/8, 172.16/12, 192.168/16)
//!     * unspecified (0.0.0.0, ::)
//!     * unique-local IPv6 (fc00::/7)
//! - Hostnames `localhost`, `*.localhost`, and `metadata.google.internal`
//!   are rejected.
//!
//! Note: `is_public_url` checks the URL string but does not protect against
//! DNS rebinding (a hostname that resolves to a public IP at registration
//! time and to e.g. 169.254.169.254 at fetch time). For outbound calls that
//! issue more than one request to the same hostname — webhook delivery,
//! federation health checks, MCP HTTP transport, image fetching — use
//! `resolve_and_validate_async` plus `pinned_client` from this module to
//! resolve once, validate every IP, and pin the actual fetch to those IPs.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// Validate that the given URL is safe for the server to dispatch a request to.
///
/// Returns `Ok(())` if the URL passes all checks, or `Err(reason)` otherwise.
pub fn is_public_url(raw: &str) -> Result<(), String> {
    if raw.is_empty() {
        return Err("URL is empty".into());
    }
    let parsed = url::Url::parse(raw).map_err(|e| format!("invalid URL: {e}"))?;

    match parsed.scheme() {
        "http" | "https" => {}
        other => {
            return Err(format!(
                "scheme '{other}' not allowed; must be http or https"
            ))
        }
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| "URL has no host".to_string())?
        .to_ascii_lowercase();

    // Reject well-known loopback hostnames even before DNS.
    if host == "localhost"
        || host.ends_with(".localhost")
        || host == "metadata.google.internal"
        || host == "metadata"
    {
        return Err(format!("host '{host}' is blocked"));
    }

    // IPv6 literals come back wrapped in brackets (e.g. "[::1]") from url 2.x
    // host_str(); strip them before parsing as IpAddr.
    let host_for_ip = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(&host);

    if let Ok(ip) = host_for_ip.parse::<IpAddr>() {
        return check_ip(ip);
    }

    // Not an IP literal — DNS name; accept (DNS rebinding not handled here).
    Ok(())
}

fn check_ip(ip: IpAddr) -> Result<(), String> {
    match ip {
        IpAddr::V4(v4) => check_v4(v4),
        IpAddr::V6(v6) => check_v6(v6),
    }
}

fn check_v4(ip: Ipv4Addr) -> Result<(), String> {
    if ip.is_loopback() {
        return Err(format!("loopback address {ip} is blocked"));
    }
    if ip.is_private() {
        return Err(format!("private address {ip} is blocked"));
    }
    if ip.is_link_local() {
        return Err(format!("link-local address {ip} is blocked"));
    }
    if ip.is_unspecified() {
        return Err(format!("unspecified address {ip} is blocked"));
    }
    if ip.is_broadcast() {
        return Err(format!("broadcast address {ip} is blocked"));
    }
    // 100.64.0.0/10 — carrier-grade NAT
    let o = ip.octets();
    if o[0] == 100 && (o[1] & 0xC0) == 0x40 {
        return Err(format!("CGNAT address {ip} is blocked"));
    }
    // 169.254.169.254 — AWS/GCE metadata (covered by link-local, but explicit)
    if ip == Ipv4Addr::new(169, 254, 169, 254) {
        return Err(format!("cloud metadata address {ip} is blocked"));
    }
    Ok(())
}

fn check_v6(ip: Ipv6Addr) -> Result<(), String> {
    if ip.is_loopback() {
        return Err(format!("IPv6 loopback {ip} is blocked"));
    }
    if ip.is_unspecified() {
        return Err(format!("IPv6 unspecified {ip} is blocked"));
    }
    let seg = ip.segments();
    // fe80::/10 — link-local
    if (seg[0] & 0xffc0) == 0xfe80 {
        return Err(format!("IPv6 link-local {ip} is blocked"));
    }
    // fc00::/7 — unique local
    if (seg[0] & 0xfe00) == 0xfc00 {
        return Err(format!("IPv6 unique-local {ip} is blocked"));
    }
    // ::ffff:0:0/96 — IPv4-mapped; recheck as V4
    if seg[0] == 0 && seg[1] == 0 && seg[2] == 0 && seg[3] == 0 && seg[4] == 0 && seg[5] == 0xffff {
        let v4 = Ipv4Addr::new(
            (seg[6] >> 8) as u8,
            (seg[6] & 0xff) as u8,
            (seg[7] >> 8) as u8,
            (seg[7] & 0xff) as u8,
        );
        return check_v4(v4);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// DNS-rebinding-safe outbound fetch
// ---------------------------------------------------------------------------

/// Resolve `url` once, validate every resolved IP against the SSRF block list,
/// and return `(host, port, addrs)`. The returned addresses can be passed to
/// `pinned_client` to force the actual fetch to use a pre-validated IP — this
/// is what defeats DNS rebinding.
///
/// Returns `Err` if the URL fails the static guard, if DNS returns no
/// addresses, or if **any** resolved IP is in the block list.
pub async fn resolve_and_validate_async(
    url: &str,
) -> Result<(String, u16, Vec<SocketAddr>), String> {
    is_public_url(url)?;

    let parsed = url::Url::parse(url).map_err(|e| format!("invalid URL: {e}"))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| "URL has no host".to_string())?
        .to_ascii_lowercase();
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| format!("URL {url} has no port and unknown default"))?;

    // Already-validated IP literal: skip DNS.
    let host_for_ip = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(&host);
    if let Ok(ip) = host_for_ip.parse::<IpAddr>() {
        check_ip(ip)?;
        return Ok((host, port, vec![SocketAddr::new(ip, port)]));
    }

    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), port))
        .await
        .map_err(|e| format!("DNS resolution failed for {host}: {e}"))?
        .collect();

    if addrs.is_empty() {
        return Err(format!("DNS returned no addresses for {host}"));
    }

    for sa in &addrs {
        check_ip(sa.ip())?;
    }

    Ok((host, port, addrs))
}

/// Build a single-shot `reqwest::Client` whose DNS resolver is pinned to the
/// supplied addresses for `host:port`. The client should be used for exactly
/// one outbound request — pin per request to keep the resolution fresh and
/// avoid stale DNS state.
///
/// The base client is consulted only for shared timeout / TLS / proxy config
/// patterns; we cannot literally clone an existing `reqwest::Client`'s
/// configuration, so the pinned client uses defaults plus the timeout the
/// caller passes in.
pub fn pinned_client(
    host: &str,
    addrs: &[SocketAddr],
    timeout: std::time::Duration,
) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder().timeout(timeout);
    builder = builder.resolve_to_addrs(host, addrs);
    builder
        .build()
        .map_err(|e| format!("reqwest client build failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_loopback() {
        assert!(is_public_url("http://127.0.0.1/x").is_err());
        assert!(is_public_url("http://localhost/x").is_err());
        assert!(is_public_url("http://[::1]/x").is_err());
    }

    #[test]
    fn rejects_rfc1918() {
        assert!(is_public_url("http://10.0.0.1/x").is_err());
        assert!(is_public_url("http://172.16.0.1/x").is_err());
        assert!(is_public_url("http://192.168.1.1/x").is_err());
    }

    #[test]
    fn rejects_link_local() {
        assert!(is_public_url("http://169.254.169.254/latest").is_err());
        assert!(is_public_url("http://[fe80::1]/x").is_err());
    }

    #[test]
    fn rejects_non_http_schemes() {
        assert!(is_public_url("file:///etc/passwd").is_err());
        assert!(is_public_url("gopher://example.com/x").is_err());
    }

    #[test]
    fn allows_public() {
        assert!(is_public_url("https://example.com/hook").is_ok());
        assert!(is_public_url("http://8.8.8.8/").is_ok());
    }

    #[tokio::test]
    async fn resolve_and_validate_rejects_loopback_literal() {
        let err = resolve_and_validate_async("http://127.0.0.1/x").await;
        assert!(err.is_err(), "loopback should be rejected");
    }

    #[tokio::test]
    async fn resolve_and_validate_rejects_private_literal() {
        assert!(resolve_and_validate_async("http://10.0.0.1/x")
            .await
            .is_err());
        assert!(resolve_and_validate_async("http://192.168.1.1/x")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn resolve_and_validate_rejects_metadata_literal() {
        let err = resolve_and_validate_async("http://169.254.169.254/latest").await;
        assert!(err.is_err(), "metadata IP must be rejected");
    }

    #[tokio::test]
    async fn resolve_and_validate_returns_addrs_for_public_literal() {
        let r = resolve_and_validate_async("http://8.8.8.8:80/").await;
        assert!(r.is_ok(), "public IP literal should validate, got {:?}", r);
        let (host, port, addrs) = r.unwrap();
        assert_eq!(host, "8.8.8.8");
        assert_eq!(port, 80);
        assert_eq!(addrs.len(), 1);
        assert_eq!(addrs[0].ip().to_string(), "8.8.8.8");
    }

    #[test]
    fn pinned_client_builds_with_one_addr() {
        let sa: SocketAddr = "8.8.8.8:443".parse().unwrap();
        let client = pinned_client("example.com", &[sa], std::time::Duration::from_secs(5));
        assert!(client.is_ok());
    }
}
