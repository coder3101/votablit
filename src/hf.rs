use reqwest::Client;
use serde::Deserialize;
use url::Url;

use crate::constants::MAX_MODEL_SIZE_BYTES;
use crate::error::AppError;

/// Error type specific to HuggingFace validation.
#[derive(Debug)]
pub enum HfError {
    /// The URL is not a valid HuggingFace link.
    InvalidUrl,
    /// The model does not exist on HuggingFace.
    ModelNotFound,
    /// Could not reach the HuggingFace API.
    ReachabilityError(String),
    /// The model exceeds the maximum allowed size.
    TooLarge { size_gb: f64 },
    /// The model is missing config.json (likely GGUF or non-standard format).
    MissingConfigJson,
}

impl std::fmt::Display for HfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidUrl => write!(f, "Invalid HuggingFace URL"),
            Self::ModelNotFound => write!(f, "Model not found on HuggingFace"),
            Self::ReachabilityError(e) => write!(f, "Could not reach HuggingFace: {e}"),
            Self::TooLarge { size_gb } => {
                write!(
                    f,
                    "Model is {size_gb:.1} GB which exceeds the 70 GB limit"
                )
            }
            Self::MissingConfigJson => {
                write!(
                    f,
                    "Model is missing config.json — only full model architectures are allowed (no GGUF or quantized-only formats)"
                )
            }
        }
    }
}

impl From<HfError> for AppError {
    fn from(err: HfError) -> Self {
        match err {
            HfError::InvalidUrl | HfError::ModelNotFound => {
                AppError::BadRequest(err.to_string())
            }
            HfError::ReachabilityError(_) | HfError::TooLarge { .. }
            | HfError::MissingConfigJson => {
                AppError::BadRequest(err.to_string())
            }
        }
    }
}

/// A single entry returned by the HuggingFace `/tree/main` API.
#[derive(Debug, Deserialize)]
struct TreeEntry {
    #[allow(dead_code)]
    path: String,
    size: Option<u64>,
}

/// Extract the `org/model` ID from a HuggingFace URL.
///
/// Supports both `huggingface.co` and `hf.co` domains.
fn extract_model_id(url: &str) -> Result<String, HfError> {
    let parsed = Url::parse(url).map_err(|_| HfError::InvalidUrl)?;
    let host = parsed.host_str().unwrap_or("");
    if !host.contains("huggingface.co") && !host.contains("hf.co") {
        return Err(HfError::InvalidUrl);
    }

    let parts: Vec<&str> = parsed
        .path()
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() < 2 {
        return Err(HfError::InvalidUrl);
    }

    Ok(format!("{}/{}", parts[0], parts[1]))
}

/// Validate that a model ID (e.g. `org/model-name`) exists on HuggingFace
/// and is within the size limit.
///
/// Unlike [`validate_hf_link`], this takes a bare model ID rather than a URL.
pub async fn validate_model_on_hf(client: &Client, model_id: &str) -> Result<(), HfError> {
    let api_url = format!(
        "https://huggingface.co/api/models/{model_id}/tree/main"
    );

    let resp = client
        .get(&api_url)
        .send()
        .await
        .map_err(|e| HfError::ReachabilityError(e.to_string()))?;

    let status = resp.status();
    if status.as_u16() == 404 || status.as_u16() == 401 {
        return Err(HfError::ModelNotFound);
    }
    if !status.is_success() {
        return Err(HfError::ReachabilityError(format!("HTTP {status}")));
    }

    let entries: Vec<TreeEntry> = resp
        .json()
        .await
        .map_err(|e| HfError::ReachabilityError(e.to_string()))?;

    let total_bytes: u64 = entries.iter().filter_map(|e| e.size).sum();
    let size_gb = total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

    if total_bytes > MAX_MODEL_SIZE_BYTES {
        return Err(HfError::TooLarge { size_gb });
    }

    if !entries.iter().any(|e| e.path == "config.json") {
        return Err(HfError::MissingConfigJson);
    }

    Ok(())
}

/// Validate a HuggingFace link by checking reachability and model size.
///
/// Returns `(model_id, size_gb)` on success. The caller can use the model ID
/// for further processing and the size for logging or display.
///
/// Uses the provided [`Client`] for HTTP requests (shared via [`AppState`]).
pub async fn validate_hf_link(client: &Client, url: &str) -> Result<(String, f64), HfError> {
    let model_id = extract_model_id(url)?;

    let api_url = format!(
        "https://huggingface.co/api/models/{model_id}/tree/main"
    );

    let resp = client
        .get(&api_url)
        .send()
        .await
        .map_err(|e| HfError::ReachabilityError(e.to_string()))?;

    let status = resp.status();
    if status.as_u16() == 404 || status.as_u16() == 401 {
        return Err(HfError::ModelNotFound);
    }
    if !status.is_success() {
        return Err(HfError::ReachabilityError(format!("HTTP {status}")));
    }

    let entries: Vec<TreeEntry> = resp
        .json()
        .await
        .map_err(|e| HfError::ReachabilityError(e.to_string()))?;

    let total_bytes: u64 = entries.iter().filter_map(|e| e.size).sum();
    let size_gb = total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

    if total_bytes > MAX_MODEL_SIZE_BYTES {
        return Err(HfError::TooLarge { size_gb });
    }

    if !entries.iter().any(|e| e.path == "config.json") {
        return Err(HfError::MissingConfigJson);
    }

    Ok((model_id, size_gb))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_model_id_valid() {
        assert_eq!(
            extract_model_id("https://huggingface.co/Qwen/Qwen2.5-7B").unwrap(),
            "Qwen/Qwen2.5-7B"
        );
    }

    #[test]
    fn extract_model_id_hf_co() {
        assert_eq!(
            extract_model_id("https://hf.co/google/gemma-2-27b").unwrap(),
            "google/gemma-2-27b"
        );
    }

    #[test]
    fn extract_model_id_with_trailing_slash() {
        assert_eq!(
            extract_model_id("https://huggingface.co/meta-llama/Llama-3-8B/").unwrap(),
            "meta-llama/Llama-3-8B"
        );
    }

    #[test]
    fn extract_model_id_non_hf_domain() {
        assert!(extract_model_id("https://google.com/search").is_err());
    }

    #[test]
    fn extract_model_id_too_few_parts() {
        assert!(extract_model_id("https://huggingface.co/single").is_err());
    }

    #[test]
    fn extract_model_id_not_a_url() {
        assert!(extract_model_id("not-a-url").is_err());
    }
}
