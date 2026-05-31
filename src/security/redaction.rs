use regex::Regex;
use std::sync::OnceLock;

const REDACTED: &str = "[REDACTED]";

/// Redacts credential-bearing fields before text reaches UI notifications or logs.
pub fn redact_sensitive_values(input: &str) -> String {
    static QUERY_SECRET_RE: OnceLock<Regex> = OnceLock::new();

    let query_secret_re = QUERY_SECRET_RE.get_or_init(|| {
        Regex::new(
            r"(?i)(refresh_token|access_token|client_secret|client_id|code|id_token)=([^&\s,}\]]+)",
        )
        .expect("valid sensitive query regex")
    });

    query_secret_re
        .replace_all(input, |caps: &regex::Captures<'_>| {
            format!("{}={}", &caps[1], REDACTED)
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::redact_sensitive_values;

    #[test]
    fn redacts_oauth_query_parameters_from_provider_errors() {
        let message = "Unexpected at list, context: { uri: https://oauth2.googleapis.com/token?refresh_token=rt-secret&client_id=client-id&client_secret=cs-secret&grant_type=refresh_token }";

        let redacted = redact_sensitive_values(message);

        assert!(!redacted.contains("rt-secret"));
        assert!(!redacted.contains("client-id"));
        assert!(!redacted.contains("cs-secret"));
        assert!(redacted.contains("refresh_token=[REDACTED]"));
        assert!(redacted.contains("client_id=[REDACTED]"));
        assert!(redacted.contains("client_secret=[REDACTED]"));
        assert!(redacted.contains("grant_type=refresh_token"));
    }
}
