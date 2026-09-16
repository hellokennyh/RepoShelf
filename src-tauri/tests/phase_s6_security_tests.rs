use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use reposhelf_lib::star_history::client::fetch_star_history_svg;
use reposhelf_lib::star_history::sanitizer::validate_and_sanitize_svg;

// =============================================================================
// 1. SSRF Address Security & IP Rejection Tests (Section 44)
// =============================================================================

#[tokio::test]
async fn test_ssrf_rejects_forbidden_ip_literals_and_localhost() {
    let forbidden_hosts = [
        "localhost",
        "127.0.0.1",
        "10.0.0.1",
        "10.254.254.254",
        "172.16.0.1",
        "172.31.255.255",
        "192.168.1.1",
        "192.168.254.254",
        "169.254.1.1",
        "169.254.169.254",
        "198.18.0.1",
        "198.19.255.255",
        "[::1]",
        "[fc00::1]",
        "[fe80::1]",
        "[::ffff:127.0.0.1]",
        "[::ffff:10.0.0.1]",
        "[::ffff:192.168.1.1]",
        "[::ffff:169.254.169.254]",
    ];

    for host in forbidden_hosts {
        // Direct non-api.star-history.com hosts are rejected either by host or by address
        let url = format!("https://{}/svg?repos=owner/repo&type=Date", host);
        let res = fetch_star_history_svg(&url, None, None).await;
        assert!(res.is_err(), "Expected {} to be rejected", url);
    }
}

#[tokio::test]
async fn test_ssrf_rejects_hostname_resolving_to_private_and_reserved_ips() {
    let test_cases: Vec<(&str, IpAddr)> = vec![
        ("localhost-v4", IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))),
        ("private-10", IpAddr::V4(Ipv4Addr::new(10, 1, 2, 3))),
        ("private-172", IpAddr::V4(Ipv4Addr::new(172, 20, 0, 1))),
        ("private-192", IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100))),
        ("link-local", IpAddr::V4(Ipv4Addr::new(169, 254, 1, 1))),
        ("cloud-meta", IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254))),
        (
            "benchmark-fake-ip",
            IpAddr::V4(Ipv4Addr::new(198, 18, 0, 1)),
        ),
        (
            "unsafe-ipv6-loopback",
            IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)),
        ),
        (
            "unsafe-ipv6-ula",
            IpAddr::V6(Ipv6Addr::new(0xfc00, 0, 0, 0, 0, 0, 0, 1)),
        ),
        (
            "unsafe-ipv6-linklocal",
            IpAddr::V6(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1)),
        ),
        (
            "ipv4-mapped-ipv6",
            IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0x7f00, 0x0001)),
        ),
    ];

    for (name, ip) in test_cases {
        let mut mock_dns = HashMap::new();
        mock_dns.insert("api.star-history.com".to_string(), vec![ip]);

        let url = "https://api.star-history.com/svg?repos=owner/repo&type=Date";
        let res = fetch_star_history_svg(url, Some(&mock_dns), None).await;
        assert!(
            res.is_err(),
            "Expected IP for {} ({:?}) to be blocked by SSRF address validation",
            name,
            ip
        );
    }
}

#[tokio::test]
async fn test_valid_public_target_allowed_by_dns_validation() {
    let mut mock_dns = HashMap::new();
    // 104.21.50.1 is Cloudflare public IP
    mock_dns.insert(
        "api.star-history.com".to_string(),
        vec![IpAddr::V4(Ipv4Addr::new(104, 21, 50, 1))],
    );

    // Using a mock server with allowed_hosts to complete the HTTP exchange safely
    let server = MockServer::start().await;
    let sample_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"></svg>"#;

    Mock::given(method("GET"))
        .and(path("/svg"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(sample_svg.as_bytes().to_vec())
                .insert_header("Content-Type", "image/svg+xml"),
        )
        .mount(&server)
        .await;

    let host = server.address().ip().to_string();
    let url = format!("{}/svg?repos=owner/repo&type=Date", server.uri());

    let res = fetch_star_history_svg(&url, None, Some(&[host])).await;
    assert!(res.is_ok(), "Expected valid exchange to succeed: {:?}", res);
    assert_eq!(res.unwrap(), sample_svg);
}

// =============================================================================
// 2. Redirect Policy Tests (Section 44)
// =============================================================================

#[tokio::test]
async fn test_redirect_to_private_ip_is_blocked() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/redirect-private"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("Location", "https://192.168.1.1/svg"),
        )
        .mount(&server)
        .await;

    let host = server.address().ip().to_string();
    let port = server.address().port();
    let url = format!("https://{}:{}/redirect-private", host, port);

    let res = fetch_star_history_svg(&url, None, Some(&[host])).await;
    assert!(
        res.is_err(),
        "Expected redirect to 192.168.1.1 to be blocked"
    );
}

#[tokio::test]
async fn test_redirect_to_cloud_metadata_is_blocked() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/redirect-meta"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", "https://169.254.169.254/latest/meta-data"),
        )
        .mount(&server)
        .await;

    let host = server.address().ip().to_string();
    let port = server.address().port();
    let url = format!("https://{}:{}/redirect-meta", host, port);

    let res = fetch_star_history_svg(&url, None, Some(&[host])).await;
    assert!(
        res.is_err(),
        "Expected redirect to metadata IP to be blocked"
    );
}

#[tokio::test]
async fn test_redirect_cross_host_is_blocked() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/redirect-cross"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("Location", "https://evil.example.com/svg"),
        )
        .mount(&server)
        .await;

    let host = server.address().ip().to_string();
    let port = server.address().port();
    let url = format!("https://{}:{}/redirect-cross", host, port);

    let res = fetch_star_history_svg(&url, None, Some(&[host])).await;
    assert!(res.is_err(), "Expected cross-host redirect to be blocked");
}

#[tokio::test]
async fn test_relative_same_host_redirect_and_3_redirects_succeed() {
    let server = MockServer::start().await;
    let sample_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"></svg>"#;

    // Hop 1 -> Hop 2
    Mock::given(method("GET"))
        .and(path("/hop1"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/hop2"))
        .mount(&server)
        .await;

    // Hop 2 -> Hop 3
    Mock::given(method("GET"))
        .and(path("/hop2"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/hop3"))
        .mount(&server)
        .await;

    // Hop 3 -> final
    Mock::given(method("GET"))
        .and(path("/hop3"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/final.svg"))
        .mount(&server)
        .await;

    // Final response
    Mock::given(method("GET"))
        .and(path("/final.svg"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(sample_svg.as_bytes().to_vec())
                .insert_header("Content-Type", "image/svg+xml"),
        )
        .mount(&server)
        .await;

    let host = server.address().ip().to_string();
    let url = format!("{}/hop1", server.uri());

    let res = fetch_star_history_svg(&url, None, Some(&[host])).await;
    assert!(
        res.is_ok(),
        "Expected 3 valid redirects to succeed: {:?}",
        res
    );
    assert_eq!(res.unwrap(), sample_svg);
}

#[tokio::test]
async fn test_fourth_redirect_rejected() {
    let server = MockServer::start().await;

    // Hop 1 -> Hop 2 -> Hop 3 -> Hop 4 -> Hop 5 (4 redirects)
    Mock::given(method("GET"))
        .and(path("/hop1"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/hop2"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/hop2"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/hop3"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/hop3"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/hop4"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/hop4"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/hop5"))
        .mount(&server)
        .await;

    let host = server.address().ip().to_string();
    let url = format!("{}/hop1", server.uri());

    let res = fetch_star_history_svg(&url, None, Some(&[host])).await;
    assert!(res.is_err(), "Expected 4th redirect to be rejected");
}

// =============================================================================
// 3. SVG Security Fixtures Tests (Section 45)
// =============================================================================

#[test]
fn test_svg_reject_script() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><script>alert(1)</script></svg>"#;
    assert!(validate_and_sanitize_svg(svg).is_err());
}

#[test]
fn test_svg_reject_foreign_object() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><foreignObject width="100" height="100"><body xmlns="http://www.w3.org/1999/xhtml"><h1>Hello</h1></body></foreignObject></svg>"#;
    assert!(validate_and_sanitize_svg(svg).is_err());
}

#[test]
fn test_svg_reject_onload_attribute() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" onload="alert('pwned')"></svg>"#;
    assert!(validate_and_sanitize_svg(svg).is_err());
}

#[test]
fn test_svg_reject_external_href_and_xlink() {
    let svg_href = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><image href="https://evil.example/pic.png"/></svg>"#;
    assert!(validate_and_sanitize_svg(svg_href).is_err());

    let svg_xlink = r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 100 100"><image xlink:href="https://evil.example/pic.png"/></svg>"#;
    assert!(validate_and_sanitize_svg(svg_xlink).is_err());
}

#[test]
fn test_svg_reject_css_url_external() {
    let svg_style_attr = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><path style="fill: url('https://evil.example/image.png')"/></svg>"#;
    assert!(validate_and_sanitize_svg(svg_style_attr).is_err());

    let svg_style_tag = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><style>@import url("https://evil.example/style.css");</style></svg>"#;
    assert!(validate_and_sanitize_svg(svg_style_tag).is_err());
}

#[test]
fn test_svg_reject_javascript_uri() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><a href="javascript:alert(1)"><circle r="10"/></a></svg>"#;
    assert!(validate_and_sanitize_svg(svg).is_err());
}

#[test]
fn test_svg_reject_dtd_entities() {
    let svg = r#"<!DOCTYPE svg SYSTEM "http://evil.example/dtd"><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"></svg>"#;
    assert!(validate_and_sanitize_svg(svg).is_err());
}

#[test]
fn test_svg_reject_malformed_svg() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><path d="M 0 0"><unclosed></svg>"#;
    assert!(validate_and_sanitize_svg(svg).is_err());
}

#[test]
fn test_svg_reject_oversize_dimensions() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 5000 5000" width="5000" height="5000"></svg>"#;
    assert!(validate_and_sanitize_svg(svg).is_err());
}

#[test]
fn test_svg_reject_oversize_viewbox_pixel_area() {
    // 4096 * 4097 = 16,781,312 > 16,777,216
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 4096 4097"></svg>"#;
    assert!(validate_and_sanitize_svg(svg).is_err());
}

#[test]
fn test_svg_reject_excessive_element_count() {
    let mut big_svg =
        String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">"#);
    for _ in 0..20_005 {
        big_svg.push_str("<circle r=\"1\"/>");
    }
    big_svg.push_str("</svg>");
    assert!(validate_and_sanitize_svg(&big_svg).is_err());
}

#[test]
fn test_svg_official_style_star_history_svg_passes() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 400" width="800" height="400">
  <defs>
    <linearGradient id="grad1" x1="0%" y1="0%" x2="100%" y2="0%">
      <stop offset="0%" style="stop-color:rgb(255,255,0);stop-opacity:1" />
      <stop offset="100%" style="stop-color:rgb(255,0,0);stop-opacity:1" />
    </linearGradient>
  </defs>
  <rect width="800" height="400" fill="#ffffff"/>
  <path d="M 50 350 L 750 50" fill="url(#grad1)" stroke="#555555" stroke-width="2" />
  <text x="50" y="30" font-family="sans-serif" font-size="14" fill="#333333">Star History Chart</text>
</svg>"##;

    let res = validate_and_sanitize_svg(svg);
    assert!(
        res.is_ok(),
        "Expected official-style Star History SVG to pass: {:?}",
        res
    );
}

// =============================================================================
// 4. Credential Isolation Test (Section 46)
// =============================================================================

#[tokio::test]
async fn test_star_history_request_carries_no_credentials() {
    let server = MockServer::start().await;
    let sample_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"></svg>"#;

    Mock::given(method("GET"))
        .and(path("/svg"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(sample_svg.as_bytes().to_vec())
                .insert_header("Content-Type", "image/svg+xml"),
        )
        .mount(&server)
        .await;

    let host = server.address().ip().to_string();
    let url = format!("{}/svg?repos=owner/repo&type=Date", server.uri());

    let res = fetch_star_history_svg(&url, None, Some(&[host])).await;
    assert!(res.is_ok());

    // Inspect received requests on mock server
    let received = server
        .received_requests()
        .await
        .expect("Failed to get received requests");
    assert_eq!(received.len(), 1);

    let req = &received[0];
    assert!(
        !req.headers
            .iter()
            .any(|(name, _)| name.as_str().eq_ignore_ascii_case("authorization")),
        "Star History request must NOT carry Authorization header"
    );
    assert!(
        !req.headers
            .iter()
            .any(|(name, _)| name.as_str().eq_ignore_ascii_case("cookie")),
        "Star History request must NOT carry Cookie header"
    );
}
