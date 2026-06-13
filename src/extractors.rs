use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::HeaderMap;
use subtle::ConstantTimeEq;

use crate::error::AppError;
use crate::state::AppState;

/// Maximum allowed length for a client UUID.
pub const MAX_CLIENT_UUID_LEN: usize = 64;

/// Extractor that validates the request carries a valid admin bearer token.
///
/// Reads the token from [`AppState`] and uses constant-time comparison
/// to prevent timing attacks. Returns the empty `AdminAuth` on success,
/// or [`AppError::Unauthorized`] / [`AppError::Internal`] on failure.
///
/// ```ignore
/// async fn admin_handler(_auth: AdminAuth) -> impl IntoResponse { ... }
/// ```
pub struct AdminAuth;

impl FromRequestParts<AppState> for AdminAuth {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = &state.admin_token;
        if token.is_empty() {
            return Err(AppError::Internal(
                "ADMIN_TOKEN not configured".to_string(),
            ));
        }

        let provided = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| {
                AppError::Unauthorized("Missing or invalid admin token".to_string())
            })?;

        // Constant-time comparison to prevent timing attacks.
        // We compare fixed-length by first checking length, then bytes.
        let ok = provided.len() == token.len()
            && provided.as_bytes().ct_eq(token.as_bytes()).into();

        if !ok {
            return Err(AppError::Unauthorized(
                "Invalid or missing admin token".to_string(),
            ));
        }

        Ok(AdminAuth)
    }
}

/// Extract the client IP address from trusted proxy headers or the socket.
///
/// Priority order:
/// 1. `Fly-Client-IP` — set by Fly.io edge proxy, not forgeable by clients.
/// 2. `X-Forwarded-For` — last entry (appended by the trusted reverse proxy).
///    Using the *last* entry prevents clients from injecting fake IPs by
///    prepending to the header.
/// 3. Direct peer socket address.
/// 4. `"unknown"` as a final fallback.
pub fn extract_client_ip(headers: &HeaderMap, addr: Option<std::net::SocketAddr>) -> String {
    // Fly.io sets this header at the edge; clients cannot forge it.
    if let Some(fly_ip) = headers.get("fly-client-ip")
        && let Ok(value) = fly_ip.to_str()
    {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    // Fallback: last entry in X-Forwarded-For (appended by trusted proxy).
    if let Some(forwarded) = headers.get("x-forwarded-for")
        && let Ok(value) = forwarded.to_str()
        && let Some(last_ip) = value.rsplit(',').next()
    {
        let trimmed = last_ip.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    addr.map(|a| a.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Validate that a model ID contains only safe characters.
///
/// Allowed: alphanumeric, `-`, `_`, `.`, `/` (for org/model format).
pub fn is_valid_model_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_alphanumeric() || "-_./".contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    use axum::http::header::HeaderName;

    fn header(name: &str, value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        let header_name: HeaderName = name.parse().unwrap();
        h.insert(header_name, value.parse().unwrap());
        h
    }

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (name, value) in pairs {
            let header_name: HeaderName = name.parse().unwrap();
            h.insert(header_name, value.parse().unwrap());
        }
        h
    }

    // -- Fly-Client-IP takes priority --

    #[test]
    fn fly_client_ip_preferred_over_xff() {
        let h = headers(&[
            ("fly-client-ip", "1.2.3.4"),
            ("x-forwarded-for", "5.6.7.8"),
        ]);
        assert_eq!(extract_client_ip(&h, None), "1.2.3.4");
    }

    #[test]
    fn fly_client_ip_single() {
        let h = header("fly-client-ip", "203.0.113.50");
        assert_eq!(extract_client_ip(&h, None), "203.0.113.50");
    }

    // -- X-Forwarded-For uses LAST entry --

    #[test]
    fn forwarded_for_uses_last_entry() {
        let h = header("x-forwarded-for", "spoofed-ip, 70.41.3.18, 150.172.238.178");
        assert_eq!(extract_client_ip(&h, None), "150.172.238.178");
    }

    #[test]
    fn forwarded_for_single_ip() {
        let h = header("x-forwarded-for", "203.0.113.50");
        assert_eq!(extract_client_ip(&h, None), "203.0.113.50");
    }

    #[test]
    fn forwarded_for_with_whitespace() {
        let h = header("x-forwarded-for", "fake, 203.0.113.50  ");
        assert_eq!(extract_client_ip(&h, None), "203.0.113.50");
    }

    #[test]
    fn forwarded_for_empty_falls_back_to_socket() {
        let h = header("x-forwarded-for", "");
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        assert_eq!(extract_client_ip(&h, Some(addr)), "127.0.0.1");
    }

    // -- Socket fallback --

    #[test]
    fn no_forwarded_for_uses_socket() {
        let h = HeaderMap::new();
        let addr: SocketAddr = "10.0.0.1:9090".parse().unwrap();
        assert_eq!(extract_client_ip(&h, Some(addr)), "10.0.0.1");
    }

    #[test]
    fn no_forwarded_for_no_socket_returns_unknown() {
        let h = HeaderMap::new();
        assert_eq!(extract_client_ip(&h, None), "unknown");
    }

    #[test]
    fn forwarded_for_whitespace_only_falls_back() {
        let h = header("x-forwarded-for", "   ");
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        assert_eq!(extract_client_ip(&h, Some(addr)), "127.0.0.1");
    }

    // -- model_id validation --

    #[test]
    fn valid_model_ids() {
        assert!(is_valid_model_id("meta-llama/Llama-3-8B"));
        assert!(is_valid_model_id("Qwen/Qwen2.5-7B"));
        assert!(is_valid_model_id("simple_model"));
    }

    #[test]
    fn invalid_model_ids() {
        assert!(!is_valid_model_id(""));
        assert!(!is_valid_model_id("has spaces"));
        assert!(!is_valid_model_id("<script>alert(1)</script>"));
        assert!(!is_valid_model_id("new\nline"));
    }
}
