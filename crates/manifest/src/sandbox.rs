//! Sandbox artifact validation — the bounded-surface assertion.
//!
//! A sandbox is a generated, self-contained HTML replica whose navigable
//! surface must be exactly the route set its source recording visited (the
//! distinct `current_path` values in the manifest). That property is a hard
//! constraint, not a quality target, so it is enforced mechanically — by the
//! CLI before a push, and again by the server on receipt, since a pushed
//! artifact is client input and is never trusted.
//!
//! Contract validated here:
//! - one `<section data-route="…">` per allowed route, and nothing else may
//!   carry `data-route`;
//! - every `data-nav` target is in the allowed set;
//! - no external resource references (the artifact must be self-contained).

/// Collect the values of one attribute across the document. Naive scan — the
/// generator emits lowercase double-quoted attributes, and a false negative
/// here fails closed into a validation error, never into an unbounded sandbox.
fn attr_values<'a>(html: &'a str, attr: &str) -> Vec<&'a str> {
    let needle = format!("{attr}=\"");
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(pos) = rest.find(&needle) {
        let after = &rest[pos + needle.len()..];
        if let Some(end) = after.find('"') {
            out.push(&after[..end]);
            rest = &after[end..];
        } else {
            break;
        }
    }
    out
}

/// Validate a sandbox artifact against its allowed route set.
///
/// Returns the list of violations (empty = valid), so both the CLI and the
/// server can render them in their own error shapes.
pub fn validate_sandbox_html(html: &str, allowed_routes: &[String]) -> Vec<String> {
    let mut violations: Vec<String> = Vec::new();

    let sections = attr_values(html, "data-route");
    for route in allowed_routes {
        match sections.iter().filter(|s| *s == route).count() {
            1 => {}
            0 => violations.push(format!("missing section for route {route}")),
            n => violations.push(format!("route {route} rendered {n} times")),
        }
    }
    for section in &sections {
        if !allowed_routes.iter().any(|r| r == section) {
            violations.push(format!("invented route {section}"));
        }
    }

    for nav in attr_values(html, "data-nav") {
        if !allowed_routes.iter().any(|r| r == nav) {
            violations.push(format!("navigation target outside the route set: {nav}"));
        }
    }

    let lower = html.to_ascii_lowercase();
    for pattern in [
        "src=\"http",
        "src='http",
        "href=\"http",
        "href='http",
        "url(http",
        "@import",
    ] {
        if lower.contains(pattern) {
            violations.push(format!("external resource reference: {pattern}"));
        }
    }

    violations.truncate(10);
    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    fn routes(rs: &[&str]) -> Vec<String> {
        rs.iter().map(|s| s.to_string()).collect()
    }

    fn doc(body: &str) -> String {
        format!(
            "<!DOCTYPE html><html><head><style>.a{{color:red}}</style></head><body>{body}<script>var x=1;</script></body></html>"
        )
    }

    #[test]
    fn valid_document_passes() {
        let html = doc(
            r#"<nav><button data-nav="/dashboard">Home</button><a data-inert>Settings</a></nav>
               <section data-route="/dashboard">d</section>
               <section data-route="/dashboard/tours">t</section>"#,
        );
        assert!(
            validate_sandbox_html(&html, &routes(&["/dashboard", "/dashboard/tours"])).is_empty()
        );
    }

    #[test]
    fn missing_route_fails() {
        let html = doc(r#"<section data-route="/dashboard">d</section>"#);
        let violations = validate_sandbox_html(&html, &routes(&["/dashboard", "/settings"]));
        assert!(violations.iter().any(|v| v.contains("/settings")));
    }

    #[test]
    fn invented_route_fails() {
        let html = doc(r#"<section data-route="/dashboard">d</section>
               <section data-route="/billing">fabricated</section>"#);
        let violations = validate_sandbox_html(&html, &routes(&["/dashboard"]));
        assert!(
            violations
                .iter()
                .any(|v| v.contains("invented route /billing"))
        );
    }

    #[test]
    fn duplicate_route_fails() {
        let html = doc(r#"<section data-route="/dashboard">a</section>
               <section data-route="/dashboard">b</section>"#);
        let violations = validate_sandbox_html(&html, &routes(&["/dashboard"]));
        assert!(violations.iter().any(|v| v.contains("rendered 2 times")));
    }

    #[test]
    fn nav_outside_route_set_fails() {
        let html = doc(r#"<button data-nav="/settings">Settings</button>
               <section data-route="/dashboard">d</section>"#);
        let violations = validate_sandbox_html(&html, &routes(&["/dashboard"]));
        assert!(violations.iter().any(|v| v.contains("/settings")));
    }

    #[test]
    fn external_resources_fail() {
        let html = doc(
            r#"<section data-route="/dashboard"><img src="https://cdn.example.com/x.png"></section>"#,
        );
        assert!(!validate_sandbox_html(&html, &routes(&["/dashboard"])).is_empty());
    }
}
