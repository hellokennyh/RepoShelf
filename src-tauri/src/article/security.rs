use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;
use url::Url;

use crate::error::AppError;

pub const DEFAULT_ARTICLE_TIMEOUT_SECS: u64 = 20;
pub const MAX_ARTICLE_REDIRECTS: usize = 3;
pub const MAX_ARTICLE_BODY_BYTES: usize = 2 * 1024 * 1024; // 2 MiB

/// Check if an IPv4 address is in a safe, public range.
///
/// Explicitly rejects:
/// - 0.0.0.0/8 (Current network / default route)
/// - 10.0.0.0/8 (Private network)
/// - 100.64.0.0/10 (Shared address space / Carrier-grade NAT)
/// - 127.0.0.0/8 (Loopback)
/// - 169.254.0.0/16 (Link-local, including 169.254.169.254 metadata service)
/// - 172.16.0.0/12 (Private network)
/// - 192.0.0.0/24 (IETF Protocol Assignments)
/// - 192.0.2.0/24 (TEST-NET-1 documentation)
/// - 192.168.0.0/16 (Private network)
/// - 198.18.0.0/15 (Benchmarking / Fake-IP)
/// - 198.51.100.0/24 (TEST-NET-2 documentation)
/// - 203.0.113.0/24 (TEST-NET-3 documentation)
/// - 224.0.0.0/4 (Multicast)
/// - 240.0.0.0/4 (Reserved)
/// - 255.255.255.255 (Broadcast)
pub fn is_safe_ipv4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    if o[0] == 0 {
        return false;
    }
    if o[0] == 10 {
        return false;
    }
    if o[0] == 100 && (o[1] & 0b1100_0000) == 64 {
        return false;
    }
    if o[0] == 127 {
        return false;
    }
    if o[0] == 169 && o[1] == 254 {
        return false;
    }
    if o[0] == 172 && (16..=31).contains(&o[1]) {
        return false;
    }
    if o[0] == 192 && o[1] == 0 && o[2] == 0 {
        return false;
    }
    if o[0] == 192 && o[1] == 0 && o[2] == 2 {
        return false;
    }
    if o[0] == 192 && o[1] == 168 {
        return false;
    }
    if o[0] == 198 && (o[1] == 18 || o[1] == 19) {
        return false;
    }
    if o[0] == 198 && o[1] == 51 && o[2] == 100 {
        return false;
    }
    if o[0] == 203 && o[1] == 0 && o[2] == 113 {
        return false;
    }
    if (224..=239).contains(&o[0]) {
        return false;
    }
    if o[0] >= 240 {
        return false;
    }

    true
}

/// Check if an IPv6 address is in a safe, public range.
///
/// Explicitly rejects:
/// - ::/128 (Unspecified)
/// - ::1/128 (Loopback)
/// - fc00::/7 (Unique local address / private)
/// - fe80::/10 (Link-local unicast)
/// - ff00::/8 (Multicast)
/// - ::ffff:0:0/96 (IPv4-mapped)
/// - ::<ipv4> (IPv4-compatible)
/// - 64:ff9b::/96 (NAT64 prefix)
/// - 2001:db8::/32 (Documentation)
/// - 100::/64 (Discard prefix)
/// - 2001::/23 (IETF Protocol Assignments)
pub fn is_safe_ipv6(ip: Ipv6Addr) -> bool {
    let segs = ip.segments();
    if ip.is_unspecified() || ip.is_loopback() {
        return false;
    }
    if (segs[0] & 0xfe00) == 0xfc00 {
        return false;
    }
    if (segs[0] & 0xffc0) == 0xfe80 {
        return false;
    }
    if (segs[0] & 0xff00) == 0xff00 {
        return false;
    }
    if segs[0] == 0x2001 && segs[1] == 0x0db8 {
        return false;
    }
    if segs[0] == 0x0100 && segs[1] == 0 && segs[2] == 0 && segs[3] == 0 {
        return false;
    }
    if segs[0] == 0x2001 && (segs[1] & 0xfe00) == 0 {
        return false;
    }

    // IPv4-mapped IPv6: ::ffff:0:0/96
    if segs[0] == 0
        && segs[1] == 0
        && segs[2] == 0
        && segs[3] == 0
        && segs[4] == 0
        && segs[5] == 0xffff
    {
        let v4 = Ipv4Addr::new(
            (segs[6] >> 8) as u8,
            (segs[6] & 0xff) as u8,
            (segs[7] >> 8) as u8,
            (segs[7] & 0xff) as u8,
        );
        return is_safe_ipv4(v4);
    }

    // IPv4-compatible IPv6: ::<ipv4>
    if segs[0] == 0
        && segs[1] == 0
        && segs[2] == 0
        && segs[3] == 0
        && segs[4] == 0
        && segs[5] == 0
        && (segs[6] != 0 || segs[7] > 1)
    {
        let v4 = Ipv4Addr::new(
            (segs[6] >> 8) as u8,
            (segs[6] & 0xff) as u8,
            (segs[7] >> 8) as u8,
            (segs[7] & 0xff) as u8,
        );
        return is_safe_ipv4(v4);
    }

    // NAT64 prefix: 64:ff9b::/96
    if segs[0] == 0x0064
        && segs[1] == 0xff9b
        && segs[2] == 0
        && segs[3] == 0
        && segs[4] == 0
        && segs[5] == 0
    {
        let v4 = Ipv4Addr::new(
            (segs[6] >> 8) as u8,
            (segs[6] & 0xff) as u8,
            (segs[7] >> 8) as u8,
            (segs[7] & 0xff) as u8,
        );
        return is_safe_ipv4(v4);
    }

    true
}

/// Validate if an IP address is safe for article fetch (non-private, non-loopback, non-reserved).
pub fn is_safe_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_safe_ipv4(v4),
        IpAddr::V6(v6) => is_safe_ipv6(v6),
    }
}

/// Resolve a hostname into IP addresses and check each IP against SSRF rules.
pub async fn resolve_and_validate_host(
    host: &str,
    port: u16,
    mock_dns: Option<&std::collections::HashMap<String, Vec<IpAddr>>>,
) -> Result<Vec<IpAddr>, AppError> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        if !is_safe_ip(ip) {
            return Err(AppError::ArticleBlockedAddress(
                "IP address is in a blocked or reserved range".to_string(),
            ));
        }
        return Ok(vec![ip]);
    }

    let host_lower = host.to_lowercase();
    if host_lower == "localhost"
        || host_lower.ends_with(".localhost")
        || host_lower.ends_with(".local")
    {
        return Err(AppError::ArticleBlockedAddress(format!(
            "Blocked hostname '{}'",
            host
        )));
    }

    let ips = if let Some(map) = mock_dns {
        if let Some(mocked) = map.get(&host_lower) {
            mocked.clone()
        } else {
            lookup_host_ips(&host_lower, port).await?
        }
    } else {
        lookup_host_ips(&host_lower, port).await?
    };

    if ips.is_empty() {
        return Err(AppError::ArticleBlockedAddress(format!(
            "No IP addresses resolved for host '{}'",
            host
        )));
    }

    for ip in &ips {
        if !is_safe_ip(*ip) {
            return Err(AppError::ArticleBlockedAddress(format!(
                "Host '{}' resolved to blocked or reserved IP address",
                host
            )));
        }
    }

    Ok(ips)
}

async fn lookup_host_ips(host: &str, port: u16) -> Result<Vec<IpAddr>, AppError> {
    let socket_str = format!("{}:{}", host, port);
    let addrs = tokio::net::lookup_host(&socket_str).await.map_err(|e| {
        AppError::ArticleBlockedAddress(format!("Failed to resolve host '{}': {}", host, e))
    })?;
    Ok(addrs.map(|s| s.ip()).collect())
}

/// Safely fetch static HTML for an article with SSRF protection, manual redirect validation,
/// content-type checks, and strict response size budget.
///
/// NOTE on DNS Rebinding Residual Risk:
/// Reqwest connections establish standard TCP sockets by hostname. While we strictly resolve
/// and validate all DNS records before every request and every redirect hop, there is a theoretical
/// window between our pre-flight DNS validation and the socket connect if the authoritative DNS
/// server responds with a 0-TTL record that alternates between a public and private IP.
/// This residual risk is mitigated within standard dependencies by per-hop resolution,
/// redirect destination re-verification, and anonymous client isolation.
pub async fn fetch_article_html(
    target_url: &str,
    mock_dns: Option<&std::collections::HashMap<String, Vec<IpAddr>>>,
    allowed_hosts: Option<&[String]>,
) -> Result<String, AppError> {
    let trimmed = target_url.trim();
    if trimmed.is_empty() {
        return Err(AppError::ArticleInvalidUrl(
            "Article URL cannot be empty".to_string(),
        ));
    }

    let mut current_url = Url::parse(trimmed)
        .map_err(|e| AppError::ArticleInvalidUrl(format!("Invalid URL: {}", e)))?;

    // Validate scheme
    let scheme = current_url.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(AppError::ArticleUnsupportedScheme(format!(
            "Unsupported scheme '{}'. Only http and https are supported.",
            scheme
        )));
    }

    // Build credential-free client with no automatic redirects
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(DEFAULT_ARTICLE_TIMEOUT_SECS))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| AppError::ArticleHttp {
            status: 0,
            message: format!("Failed to build HTTP client: {}", e),
        })?;

    let mut redirect_count = 0;

    loop {
        // Validate scheme on every hop
        if current_url.scheme() != "http" && current_url.scheme() != "https" {
            return Err(AppError::ArticleUnsupportedScheme(format!(
                "Unsupported redirect scheme '{}'",
                current_url.scheme()
            )));
        }

        let host = current_url
            .host_str()
            .ok_or_else(|| AppError::ArticleInvalidUrl("URL is missing hostname".to_string()))?;
        let port = current_url.port_or_known_default().unwrap_or(80);

        // Check allowed_hosts bypass for test servers (e.g. WireMock on 127.0.0.1 in tests)
        let is_explicitly_allowed = allowed_hosts
            .map(|hosts| hosts.iter().any(|h| h.eq_ignore_ascii_case(host)))
            .unwrap_or(false);

        if !is_explicitly_allowed {
            resolve_and_validate_host(host, port, mock_dns).await?;
        }

        // Send request (credential-free, no token, no cookies)
        let is_wechat = current_url
            .host_str()
            .map(|h| h.eq_ignore_ascii_case("mp.weixin.qq.com"))
            .unwrap_or(false);

        let user_agent = if is_wechat {
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36".to_string()
        } else {
            format!(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) RepoShelf/{}",
                env!("CARGO_PKG_VERSION")
            )
        };

        let response = client
            .get(current_url.as_str())
            .header("User-Agent", user_agent)
            .header(
                "Accept",
                "text/html, application/xhtml+xml, application/xml;q=0.9, */*;q=0.8",
            )
            .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AppError::ArticleTimeout(format!(
                        "Request timed out after {}s",
                        DEFAULT_ARTICLE_TIMEOUT_SECS
                    ))
                } else {
                    AppError::ArticleHttp {
                        status: 0,
                        message: format!("Network request failed: {}", e),
                    }
                }
            })?;

        let status = response.status();

        // Handle redirects (301, 302, 303, 307, 308)
        if status.is_redirection() {
            redirect_count += 1;
            if redirect_count > MAX_ARTICLE_REDIRECTS {
                return Err(AppError::ArticleTooManyRedirects(format!(
                    "Exceeded maximum limit of {} redirects",
                    MAX_ARTICLE_REDIRECTS
                )));
            }

            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .ok_or_else(|| AppError::ArticleHttp {
                    status: status.as_u16(),
                    message: "Redirect response missing Location header".to_string(),
                })?
                .to_str()
                .map_err(|_| AppError::ArticleHttp {
                    status: status.as_u16(),
                    message: "Invalid characters in Location header".to_string(),
                })?;

            current_url = current_url.join(location).map_err(|e| {
                AppError::ArticleInvalidUrl(format!("Invalid redirect location: {}", e))
            })?;

            continue;
        }

        if !status.is_success() {
            return Err(AppError::ArticleHttp {
                status: status.as_u16(),
                message: format!("HTTP {}", status),
            });
        }

        // Check Content-Type header
        if let Some(ct_val) = response.headers().get(reqwest::header::CONTENT_TYPE) {
            let ct_str = ct_val.to_str().unwrap_or("").to_lowercase();
            let mime = ct_str.split(';').next().unwrap_or("").trim();

            if mime.starts_with("image/")
                || mime.starts_with("video/")
                || mime.starts_with("audio/")
                || mime == "application/pdf"
                || mime == "application/octet-stream"
                || mime == "application/zip"
            {
                return Err(AppError::ArticleUnsupportedContentType(format!(
                    "Unsupported content type '{}'",
                    mime
                )));
            }
        }

        // Check Content-Length header for early budget rejection
        if let Some(cl_val) = response.headers().get(reqwest::header::CONTENT_LENGTH) {
            if let Ok(cl_str) = cl_val.to_str() {
                if let Ok(len) = cl_str.parse::<usize>() {
                    if len > MAX_ARTICLE_BODY_BYTES {
                        return Err(AppError::ArticleResponseTooLarge(format!(
                            "Content-Length {} exceeds maximum budget of 2 MiB",
                            len
                        )));
                    }
                }
            }
        }

        // Stream response body up to 2 MiB
        let mut response = response;
        let mut body_bytes = Vec::new();

        while let Some(chunk) = response.chunk().await.map_err(|e| AppError::ArticleHttp {
            status: 0,
            message: format!("Failed to read response body: {}", e),
        })? {
            body_bytes.extend_from_slice(&chunk);
            if body_bytes.len() > MAX_ARTICLE_BODY_BYTES {
                return Err(AppError::ArticleResponseTooLarge(
                    "Response body exceeded maximum budget of 2 MiB".to_string(),
                ));
            }
        }

        // If Content-Type was missing or ambiguous, sniff first bytes for binary signatures
        if body_bytes.len() >= 4 {
            let prefix = &body_bytes[..body_bytes.len().min(512)];
            // PDF: %PDF-
            if prefix.starts_with(b"%PDF-")
                // ZIP: PK\x03\x04
                || prefix.starts_with(b"PK\x03\x04")
                // PNG: \x89PNG
                || prefix.starts_with(b"\x89PNG")
                // GIF: GIF8
                || prefix.starts_with(b"GIF8")
                // JPEG: \xFF\xD8\xFF
                || prefix.starts_with(b"\xFF\xD8\xFF")
                // Check for null bytes in initial chunk (binary content)
                || prefix.iter().any(|&b| b == 0)
            {
                return Err(AppError::ArticleUnsupportedContentType(
                    "Binary or unsupported content detected".to_string(),
                ));
            }
        }

        let html = String::from_utf8_lossy(&body_bytes).to_string();
        return Ok(html);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_ssrf_rejected_ipv4_addresses() {
        let blocked = [
            "0.0.0.0",
            "0.255.255.255",
            "10.0.0.1",
            "10.255.255.255",
            "100.64.0.1",
            "100.127.255.255",
            "127.0.0.1",
            "127.255.255.254",
            "169.254.169.254",
            "169.254.1.1",
            "172.16.0.1",
            "172.31.255.255",
            "192.0.0.1",
            "192.0.2.1",
            "192.168.1.1",
            "192.168.254.254",
            "198.18.0.1",
            "198.19.255.255",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "239.255.255.255",
            "240.0.0.1",
            "255.255.255.255",
        ];

        for ip_str in blocked {
            let ip = Ipv4Addr::from_str(ip_str).unwrap();
            assert!(
                !is_safe_ipv4(ip),
                "Expected IPv4 {} to be rejected by SSRF rules",
                ip_str
            );
            assert!(
                !is_safe_ip(IpAddr::V4(ip)),
                "Expected IpAddr::V4({}) to be rejected",
                ip_str
            );
        }
    }

    #[test]
    fn test_ssrf_allowed_ipv4_addresses() {
        let allowed = [
            "1.1.1.1",
            "8.8.8.8",
            "93.184.216.34",
            "140.82.121.4",
            "185.199.108.153",
        ];

        for ip_str in allowed {
            let ip = Ipv4Addr::from_str(ip_str).unwrap();
            assert!(
                is_safe_ipv4(ip),
                "Expected public IPv4 {} to be allowed",
                ip_str
            );
            assert!(
                is_safe_ip(IpAddr::V4(ip)),
                "Expected IpAddr::V4({}) to be allowed",
                ip_str
            );
        }
    }

    #[test]
    fn test_ssrf_rejected_ipv6_addresses() {
        let blocked = [
            "::",
            "::1",
            "fc00::1",
            "fd12:3456:789a::1",
            "fe80::1",
            "fe80::dead:beef",
            "ff02::1",
            "ff00::1",
            "2001:db8::1",
            "100::1",
            "2001::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::ffff:192.168.1.1",
            "::ffff:169.254.169.254",
            "64:ff9b::127.0.0.1",
        ];

        for ip_str in blocked {
            let ip = Ipv6Addr::from_str(ip_str).unwrap();
            assert!(
                !is_safe_ipv6(ip),
                "Expected IPv6 {} to be rejected by SSRF rules",
                ip_str
            );
            assert!(
                !is_safe_ip(IpAddr::V6(ip)),
                "Expected IpAddr::V6({}) to be rejected",
                ip_str
            );
        }
    }

    #[test]
    fn test_ssrf_allowed_ipv6_addresses() {
        let allowed = [
            "2606:4700:4700::1111",
            "2001:4860:4860::8888",
            "2a00:1450:4001:830::200e",
        ];

        for ip_str in allowed {
            let ip = Ipv6Addr::from_str(ip_str).unwrap();
            assert!(
                is_safe_ipv6(ip),
                "Expected public IPv6 {} to be allowed",
                ip_str
            );
            assert!(
                is_safe_ip(IpAddr::V6(ip)),
                "Expected IpAddr::V6({}) to be allowed",
                ip_str
            );
        }
    }
}
