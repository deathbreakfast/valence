//! Redact credentials from connection endpoints and error text.

/// Remove URL userinfo before placing an endpoint in an error or log message.
///
/// Comma-separated multi-endpoint strings are redacted per segment (Redis cluster style).
#[must_use]
pub fn redact_endpoint(endpoint: &str) -> String {
    endpoint
        .split(',')
        .map(redact_single_endpoint)
        .collect::<Vec<_>>()
        .join(",")
}

fn redact_single_endpoint(endpoint: &str) -> String {
    let Some(scheme_end) = endpoint.find("://") else {
        return endpoint.to_string();
    };
    let authority_start = scheme_end + 3;
    let authority = &endpoint[authority_start..];
    let Some(userinfo_end) = authority.find('@') else {
        return endpoint.to_string();
    };
    let userinfo_end = authority_start + userinfo_end;
    let host_start = userinfo_end + 1;
    if endpoint[authority_start..userinfo_end].contains(['/', '?', '#']) {
        return endpoint.to_string();
    }
    format!(
        "{}***@{}",
        &endpoint[..authority_start],
        &endpoint[host_start..]
    )
}

/// Length of a URL-like prefix starting at `s` (scheme through host/path until whitespace).
fn consume_endpoint_prefix(s: &str) -> usize {
    let Some(scheme_end) = s.find("://") else {
        return s.len();
    };
    let after_scheme = scheme_end + 3;
    let rest = &s[after_scheme..];
    let end_rel = rest
        .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ')' || c == ']')
        .unwrap_or(rest.len());
    after_scheme + end_rel
}

/// Redact `scheme://userinfo@host` substrings embedded in free-form error text.
#[must_use]
pub fn redact_credentials_in_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if let Some(rel) = text[i..].find("://") {
            let abs = i + rel;
            let scheme_start = text[..abs]
                .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '+' || c == '.' || c == '-'))
                .map_or(i, |j| j + 1);
            out.push_str(&text[i..scheme_start]);
            let consumed = consume_endpoint_prefix(&text[scheme_start..]);
            let endpoint = &text[scheme_start..scheme_start + consumed];
            out.push_str(&redact_endpoint(endpoint));
            i = scheme_start + consumed;
        } else {
            out.push_str(&text[i..]);
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_url_userinfo() {
        let redacted = redact_endpoint("postgres://user:secret@host:5432/db");
        assert_eq!(redacted, "postgres://***@host:5432/db");
        assert!(!redacted.contains("secret"));
    }

    #[test]
    fn redacts_embedded_url_in_error_text() {
        let raw = "storage error: error communicating with database: postgres://u:p@localhost/db";
        let redacted = redact_credentials_in_text(raw);
        assert!(redacted.contains("postgres://***@localhost/db"));
        assert!(!redacted.contains("u:p@"));
    }

    #[test]
    fn leaves_urls_without_userinfo() {
        assert_eq!(
            redact_endpoint("redis://127.0.0.1:6379"),
            "redis://127.0.0.1:6379"
        );
    }
}
