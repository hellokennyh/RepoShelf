use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::error::AppError;

pub const MAX_SVG_BYTES: usize = 2 * 1024 * 1024; // 2 MiB (2,097,152 bytes)
pub const MAX_SVG_DIMENSION: f64 = 4096.0;
pub const MAX_SVG_PIXEL_AREA: f64 = 16_777_216.0; // 4096 * 4096
pub const MAX_SVG_ELEMENT_COUNT: usize = 20_000;

/// Checks CSS text inside `<style>` or `style="..."` for dangerous constructs.
fn check_safe_css(css: &str) -> Result<(), AppError> {
    let lower = css.to_ascii_lowercase();

    if lower.contains("@import") {
        return Err(AppError::InvalidInput(
            "@import is rejected in SVG CSS".to_string(),
        ));
    }

    if lower.contains("javascript:") || lower.contains("vbscript:") {
        return Err(AppError::InvalidInput(
            "Script URI is rejected in SVG CSS".to_string(),
        ));
    }

    // Check every url(...) occurrence
    let mut rest = lower.as_str();
    while let Some(pos) = rest.find("url(") {
        let after_url = &rest[pos + 4..];
        if let Some(end_pos) = after_url.find(')') {
            let inside = after_url[..end_pos]
                .trim()
                .trim_matches('\'')
                .trim_matches('"')
                .trim();

            // Safe internal same-document fragment references (e.g. url(#clip) or url(#grad)) are permitted.
            // External resources (http:, https:, //, data:, etc.) are strictly rejected.
            if !inside.starts_with('#') && !inside.is_empty() {
                return Err(AppError::InvalidInput(format!(
                    "External resource reference in SVG CSS url({}) is rejected",
                    inside
                )));
            }
            rest = &after_url[end_pos + 1..];
        } else {
            return Err(AppError::InvalidInput(
                "Malformed url() in SVG CSS".to_string(),
            ));
        }
    }

    Ok(())
}

fn parse_dimension(val: &str) -> Option<f64> {
    let trimmed = val.trim();
    let num_str = trimmed
        .trim_end_matches("px")
        .trim_end_matches("pt")
        .trim_end_matches("em")
        .trim_end_matches("rem")
        .trim();
    num_str.parse::<f64>().ok()
}

/// Validates and sanitizes an SVG document using an actual XML pull parser (`quick-xml`).
///
/// Enforces:
/// - Max bytes budget: 2 MiB
/// - Max element count: 20,000
/// - Rejection of `<script>`, `<foreignObject>`, `<iframe>`, `<object>`, `<embed>`, `<audio>`, `<video>`
/// - Rejection of event handler attributes (`onload`, `onclick`, or any `on*`)
/// - Rejection of external `href`, `xlink:href`, `src` (only internal fragments `#...` permitted)
/// - Rejection of `@import`, external `url(...)`, and `javascript:` in CSS / style attributes
/// - Rejection of XML DTD / external entity processing
/// - Dimensional budgets: width and height <= 4096, pixel area <= 16,777,216
/// - Structural XML validity
pub fn validate_and_sanitize_svg(svg_content: &str) -> Result<String, AppError> {
    if svg_content.len() > MAX_SVG_BYTES {
        return Err(AppError::InvalidInput(format!(
            "SVG exceeds maximum size budget of 2 MiB (got {} bytes)",
            svg_content.len()
        )));
    }

    let mut reader = Reader::from_str(svg_content);
    reader.config_mut().trim_text(false);
    reader.config_mut().check_end_names = true;

    let mut buf = Vec::new();
    let mut element_count = 0;
    let mut is_in_style_element = false;
    let mut root_svg_inspected = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::DocType(doctype)) => {
                let dt_str = String::from_utf8_lossy(doctype.as_ref()).to_ascii_lowercase();
                if dt_str.contains("entity")
                    || dt_str.contains("system")
                    || dt_str.contains("public")
                {
                    return Err(AppError::InvalidInput(
                        "XML DTD / external entities are rejected in SVG".to_string(),
                    ));
                }
            }

            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                element_count += 1;
                if element_count > MAX_SVG_ELEMENT_COUNT {
                    return Err(AppError::InvalidInput(format!(
                        "SVG element count exceeds maximum of {} (got {})",
                        MAX_SVG_ELEMENT_COUNT, element_count
                    )));
                }

                let tag_name_raw = e.name();
                let tag_name = String::from_utf8_lossy(tag_name_raw.as_ref()).to_ascii_lowercase();

                // Disallowed tags
                match tag_name.as_str() {
                    "script" | "foreignobject" | "iframe" | "object" | "embed" | "audio"
                    | "video" => {
                        return Err(AppError::InvalidInput(format!(
                            "Disallowed SVG element '<{}>' is rejected",
                            tag_name
                        )));
                    }
                    "style" => {
                        is_in_style_element = true;
                    }
                    _ => {}
                }

                // Inspect root <svg> dimensions and viewBox
                if !root_svg_inspected && tag_name == "svg" {
                    root_svg_inspected = true;
                    let mut width_val: Option<f64> = None;
                    let mut height_val: Option<f64> = None;
                    let mut viewbox_val: Option<String> = None;

                    for attr_res in e.attributes() {
                        let attr = attr_res.map_err(|e| {
                            AppError::InvalidInput(format!("Malformed SVG attribute: {}", e))
                        })?;
                        let k = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
                        let v = String::from_utf8_lossy(&attr.value).to_string();

                        if k == "width" {
                            width_val = parse_dimension(&v);
                        } else if k == "height" {
                            height_val = parse_dimension(&v);
                        } else if k == "viewbox" {
                            viewbox_val = Some(v);
                        }
                    }

                    if let Some(w) = width_val {
                        if w > MAX_SVG_DIMENSION || w < 0.0 {
                            return Err(AppError::InvalidInput(format!(
                                "SVG width {} exceeds maximum dimension limit of 4096",
                                w
                            )));
                        }
                    }

                    if let Some(h) = height_val {
                        if h > MAX_SVG_DIMENSION || h < 0.0 {
                            return Err(AppError::InvalidInput(format!(
                                "SVG height {} exceeds maximum dimension limit of 4096",
                                h
                            )));
                        }
                    }

                    if let (Some(w), Some(h)) = (width_val, height_val) {
                        if w * h > MAX_SVG_PIXEL_AREA {
                            return Err(AppError::InvalidInput(format!(
                                "SVG dimensions {}x{} exceed maximum pixel area limit",
                                w, h
                            )));
                        }
                    }

                    if let Some(vb) = viewbox_val {
                        let parts: Vec<&str> = vb.split_whitespace().collect();
                        if parts.len() == 4 {
                            if let (Ok(w), Ok(h)) =
                                (parts[2].parse::<f64>(), parts[3].parse::<f64>())
                            {
                                if w < 0.0
                                    || h < 0.0
                                    || w > MAX_SVG_DIMENSION
                                    || h > MAX_SVG_DIMENSION
                                {
                                    return Err(AppError::InvalidInput(format!(
                                        "SVG viewBox dimensions {}x{} exceed maximum limit of 4096",
                                        w, h
                                    )));
                                }
                                if w * h > MAX_SVG_PIXEL_AREA {
                                    return Err(AppError::InvalidInput(format!(
                                        "SVG viewBox pixel area {} exceeds maximum limit of 16,777,216",
                                        w * h
                                    )));
                                }
                            } else {
                                return Err(AppError::InvalidInput(
                                    "Malformed SVG viewBox attribute numbers".to_string(),
                                ));
                            }
                        } else {
                            return Err(AppError::InvalidInput(
                                "Malformed SVG viewBox attribute format".to_string(),
                            ));
                        }
                    }
                }

                // Inspect attributes on all elements
                for attr_res in e.attributes() {
                    let attr = attr_res.map_err(|e| {
                        AppError::InvalidInput(format!("Malformed SVG attribute: {}", e))
                    })?;
                    let k = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
                    let v = String::from_utf8_lossy(&attr.value).to_string();
                    let v_lower = v.to_ascii_lowercase();

                    // Reject any event handler attribute starting with "on"
                    if k.starts_with("on") {
                        return Err(AppError::InvalidInput(format!(
                            "Event handler attribute '{}' is rejected in SVG",
                            k
                        )));
                    }

                    // Reject dangerous URI schemes
                    if v_lower.contains("javascript:")
                        || v_lower.contains("vbscript:")
                        || v_lower.contains("data:text/html")
                        || v_lower.contains("file:")
                    {
                        return Err(AppError::InvalidInput(format!(
                            "Unsafe URI scheme in SVG attribute '{}' is rejected",
                            k
                        )));
                    }

                    // Reject external references in href, xlink:href, src
                    if k == "href" || k == "xlink:href" || k == "src" {
                        let trimmed_v = v.trim();
                        // Allow safe internal fragment references (e.g. "#id")
                        if !trimmed_v.is_empty() && !trimmed_v.starts_with('#') {
                            return Err(AppError::InvalidInput(format!(
                                "External resource reference in '{}' is rejected: {}",
                                k, trimmed_v
                            )));
                        }
                    }

                    // Check inline style attributes
                    if k == "style" {
                        check_safe_css(&v)?;
                    }
                }
            }

            Ok(Event::End(ref e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_ascii_lowercase();
                if tag_name == "style" {
                    is_in_style_element = false;
                }
            }

            Ok(Event::Text(ref t)) => {
                if is_in_style_element {
                    let text = String::from_utf8_lossy(t.as_ref());
                    check_safe_css(&text)?;
                }
            }

            Ok(Event::CData(ref c)) => {
                if is_in_style_element {
                    let text = String::from_utf8_lossy(c.as_ref());
                    check_safe_css(&text)?;
                }
            }

            Ok(Event::Eof) => break,

            Err(e) => {
                return Err(AppError::InvalidInput(format!("Malformed SVG XML: {}", e)));
            }

            _ => {}
        }
        buf.clear();
    }

    if !root_svg_inspected {
        return Err(AppError::InvalidInput(
            "Document does not contain a valid root <svg> element".to_string(),
        ));
    }

    Ok(svg_content.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_star_history_svg() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 400" width="800" height="400">
  <defs>
    <linearGradient id="grad1" x1="0%" y1="0%" x2="100%" y2="0%">
      <stop offset="0%" style="stop-color:rgb(255,255,0);stop-opacity:1" />
      <stop offset="100%" style="stop-color:rgb(255,0,0);stop-opacity:1" />
    </linearGradient>
  </defs>
  <path d="M 10 10 L 100 100" fill="url(#grad1)" stroke="#555" />
  <text x="50" y="50">Star History</text>
</svg>"##;

        let res = validate_and_sanitize_svg(svg);
        assert!(res.is_ok(), "Expected valid SVG to pass: {:?}", res);
    }

    #[test]
    fn test_reject_script() {
        let svg = r#"<svg viewBox="0 0 100 100"><script>alert(1)</script></svg>"#;
        assert!(validate_and_sanitize_svg(svg).is_err());
    }

    #[test]
    fn test_reject_foreign_object() {
        let svg = r#"<svg viewBox="0 0 100 100"><foreignObject width="100" height="100"><iframe src="https://evil.com"></iframe></foreignObject></svg>"#;
        assert!(validate_and_sanitize_svg(svg).is_err());
    }

    #[test]
    fn test_reject_onload_attribute() {
        let svg = r#"<svg viewBox="0 0 100 100" onload="alert(1)"></svg>"#;
        assert!(validate_and_sanitize_svg(svg).is_err());
    }

    #[test]
    fn test_reject_external_href() {
        let svg =
            r#"<svg viewBox="0 0 100 100"><image href="https://evil.example/pic.png"/></svg>"#;
        assert!(validate_and_sanitize_svg(svg).is_err());

        let svg_xlink = r#"<svg viewBox="0 0 100 100"><image xlink:href="https://evil.example/pic.png"/></svg>"#;
        assert!(validate_and_sanitize_svg(svg_xlink).is_err());
    }

    #[test]
    fn test_reject_css_url_external() {
        let svg = r#"<svg viewBox="0 0 100 100"><path style="fill: url('https://evil.example/test.png')"/></svg>"#;
        assert!(validate_and_sanitize_svg(svg).is_err());

        let svg_style_tag = r#"<svg viewBox="0 0 100 100"><style>@import url("https://evil.example/style.css");</style></svg>"#;
        assert!(validate_and_sanitize_svg(svg_style_tag).is_err());
    }

    #[test]
    fn test_reject_javascript_uri() {
        let svg = r#"<svg viewBox="0 0 100 100"><a href="javascript:alert(1)"><circle r="10"/></a></svg>"#;
        assert!(validate_and_sanitize_svg(svg).is_err());
    }

    #[test]
    fn test_reject_dtd_entities() {
        let svg =
            r#"<!DOCTYPE svg SYSTEM "http://evil.example/dtd"><svg viewBox="0 0 100 100"></svg>"#;
        assert!(validate_and_sanitize_svg(svg).is_err());
    }

    #[test]
    fn test_reject_oversize_dimensions() {
        let svg = r#"<svg viewBox="0 0 5000 5000" width="5000" height="5000"></svg>"#;
        assert!(validate_and_sanitize_svg(svg).is_err());
    }

    #[test]
    fn test_reject_malformed_svg() {
        let svg = r#"<svg viewBox="0 0 100 100"><path d="M 0 0"><unclosed></svg>"#;
        assert!(validate_and_sanitize_svg(svg).is_err());
    }
}
