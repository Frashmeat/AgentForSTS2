//! OpenAI Images API 客户端（DALL-E 系列 + 兼容代理）。
//!
//! 文档：<https://platform.openai.com/docs/api-reference/images>
//!
//! 设计取向：
//! - response_format 固定 "b64_json"，避免二次 HTTP 取图（少一跳网络）
//! - 同时兼容 new-api / one-api / litellm 等"OpenAI 协议代理"——它们用同一
//!   端点路径 `/v1/images/generations`
//! - 提供默认 size（如 "1024x1024"），request.size 优先级更高
//! - 火山方舟通过这些代理暴露时（一般是 "doubao-..." 模型名）自动 work

use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue, RETRY_AFTER};
use serde::Deserialize;
use serde_json::Value;

use super::client::{
    GeneratedImage, ImageGenClient, ImageGenError, ImageGenRequest, ImageGenResponse,
};

const DEFAULT_BASE_URL: &str = "https://api.openai.com";
const DEFAULT_TIMEOUT_SECS: u64 = 300;

pub struct OpenAiImagesClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    default_model: String,
    default_size: Option<String>,
}

impl OpenAiImagesClient {
    pub fn new(
        api_key: impl Into<String>,
        default_model: impl Into<String>,
        base_url: Option<String>,
        default_size: Option<String>,
    ) -> Result<Self, ImageGenError> {
        let api_key: String = api_key.into();
        let api_key = api_key.trim().to_string();
        if api_key.is_empty() {
            return Err(ImageGenError::Config(
                "OpenAI Images api_key is empty".into(),
            ));
        }
        // 提前校验 Bearer 头能否构造，避免 headers() 里 .expect() panic。
        if HeaderValue::from_str(&format!("Bearer {api_key}")).is_err() {
            return Err(ImageGenError::Config(
                "OpenAI Images api_key contains characters invalid for an HTTP header".into(),
            ));
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .map_err(|e| ImageGenError::Config(format!("build http client: {e}")))?;
        Ok(Self {
            http,
            base_url: base_url
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| DEFAULT_BASE_URL.into()),
            api_key,
            default_model: default_model.into(),
            default_size,
        })
    }

    fn headers(&self) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let bearer = format!("Bearer {}", self.api_key);
        h.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&bearer).expect("invalid auth header"),
        );
        h
    }

    fn build_body(&self, request: &ImageGenRequest) -> Value {
        let model = request
            .model
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.default_model.clone());
        let size = request
            .size
            .clone()
            .or_else(|| self.default_size.clone())
            .unwrap_or_else(|| "1024x1024".to_string());
        serde_json::json!({
            "model": model,
            "prompt": request.prompt,
            "n": request.n.max(1),
            "size": size,
            "response_format": "b64_json",
        })
    }
}

#[async_trait]
impl ImageGenClient for OpenAiImagesClient {
    async fn generate(&self, request: ImageGenRequest) -> Result<ImageGenResponse, ImageGenError> {
        let url = format!(
            "{}/v1/images/generations",
            self.base_url.trim_end_matches('/')
        );
        let body = self.build_body(&request);
        let response = self
            .http
            .post(&url)
            .headers(self.headers())
            .json(&body)
            .send()
            .await
            .map_err(|e| ImageGenError::Transport(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let retry_after = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok());
            let text = response.text().await.unwrap_or_default();
            return Err(map_http_error(status.as_u16(), retry_after, &text));
        }

        let parsed: OpenAiImagesResponse = response
            .json()
            .await
            .map_err(|e| ImageGenError::Parse(format!("response json: {e}")))?;

        if parsed.data.is_empty() {
            return Err(ImageGenError::Empty);
        }

        let mut images: Vec<GeneratedImage> = Vec::with_capacity(parsed.data.len());
        let mut revised_prompt: Option<String> = None;
        for d in parsed.data {
            if revised_prompt.is_none() && d.revised_prompt.is_some() {
                revised_prompt = d.revised_prompt.clone();
            }
            if let Some(b64) = d.b64_json {
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(b64.trim())
                    .map_err(|e| ImageGenError::Parse(format!("b64 decode: {e}")))?;
                images.push(GeneratedImage {
                    bytes: decoded,
                    format_hint: "png".into(),
                });
            }
            // 跳过 url-only 项目；这种情况下需要二次 HTTP 取图——本 stage 不实现
        }
        if images.is_empty() {
            return Err(ImageGenError::Empty);
        }

        Ok(ImageGenResponse {
            model: self.default_model.clone(),
            images,
            revised_prompt,
        })
    }
}

fn map_http_error(status: u16, retry_after: Option<u64>, body: &str) -> ImageGenError {
    let parsed: Option<OpenAiErrorBody> = serde_json::from_str(body).ok();
    let message = parsed
        .map(|p| p.error.message)
        .unwrap_or_else(|| body.chars().take(500).collect());
    match status {
        401 | 403 => ImageGenError::Auth(message),
        429 => ImageGenError::RateLimit {
            retry_after_secs: retry_after,
            message,
        },
        _ => ImageGenError::Http { status, message },
    }
}

#[derive(Debug, Deserialize)]
struct OpenAiImagesResponse {
    data: Vec<OpenAiImageItem>,
}

#[derive(Debug, Deserialize)]
struct OpenAiImageItem {
    #[serde(default)]
    b64_json: Option<String>,
    #[serde(default)]
    revised_prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiErrorBody {
    error: OpenAiErrorInner,
}

#[derive(Debug, Deserialize)]
struct OpenAiErrorInner {
    message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_empty_api_key() {
        match OpenAiImagesClient::new("", "dall-e-3", None, None) {
            Err(ImageGenError::Config(_)) => {}
            Err(other) => panic!("expected Config error, got {other:?}"),
            Ok(_) => panic!("expected error for empty api_key"),
        }
    }

    #[test]
    fn build_body_uses_request_size_when_provided() {
        let client =
            OpenAiImagesClient::new("k", "dall-e-3", None, Some("512x512".into())).unwrap();
        let req = ImageGenRequest {
            prompt: "a cat".into(),
            n: 1,
            size: Some("1792x1024".into()),
            model: None,
        };
        let body = client.build_body(&req);
        assert_eq!(body["size"], "1792x1024");
        assert_eq!(body["prompt"], "a cat");
        assert_eq!(body["model"], "dall-e-3");
        assert_eq!(body["response_format"], "b64_json");
    }

    #[test]
    fn build_body_falls_back_to_default_size() {
        let client =
            OpenAiImagesClient::new("k", "dall-e-3", None, Some("512x512".into())).unwrap();
        let req = ImageGenRequest {
            prompt: "a dog".into(),
            n: 1,
            size: None,
            model: None,
        };
        let body = client.build_body(&req);
        assert_eq!(body["size"], "512x512");
    }

    #[test]
    fn build_body_defaults_size_when_none_configured() {
        let client = OpenAiImagesClient::new("k", "dall-e-3", None, None).unwrap();
        let req = ImageGenRequest {
            prompt: "x".into(),
            n: 1,
            size: None,
            model: None,
        };
        let body = client.build_body(&req);
        assert_eq!(body["size"], "1024x1024");
    }

    #[test]
    fn build_body_respects_request_model_override() {
        let client = OpenAiImagesClient::new("k", "dall-e-3", None, None).unwrap();
        let req = ImageGenRequest {
            prompt: "x".into(),
            n: 1,
            size: None,
            model: Some("doubao-seed-1.6".into()),
        };
        let body = client.build_body(&req);
        assert_eq!(body["model"], "doubao-seed-1.6");
    }

    #[test]
    fn build_body_clamps_n_to_at_least_one() {
        let client = OpenAiImagesClient::new("k", "dall-e-3", None, None).unwrap();
        let req = ImageGenRequest {
            prompt: "x".into(),
            n: 0,
            size: None,
            model: None,
        };
        let body = client.build_body(&req);
        assert_eq!(body["n"], 1);
    }

    #[test]
    fn map_http_error_classifies_status_codes() {
        let auth = map_http_error(401, None, r#"{"error":{"message":"bad key"}}"#);
        assert!(matches!(auth, ImageGenError::Auth(msg) if msg.contains("bad key")));
        let rate = map_http_error(429, Some(5), r#"{"error":{"message":"slow down"}}"#);
        match rate {
            ImageGenError::RateLimit {
                retry_after_secs,
                message,
            } => {
                assert_eq!(retry_after_secs, Some(5));
                assert!(message.contains("slow down"));
            }
            other => panic!("expected RateLimit, got {other:?}"),
        }
        let server = map_http_error(500, None, "boom");
        assert!(matches!(server, ImageGenError::Http { status: 500, .. }));
    }

    #[test]
    fn new_rejects_empty_and_invalid_api_key() {
        assert!(matches!(
            OpenAiImagesClient::new("", "dall-e-3", None, None),
            Err(ImageGenError::Config(_))
        ));
        // 内嵌换行的 key 旧实现会在 headers() 里 panic；现在 new() 直接拒。
        assert!(matches!(
            OpenAiImagesClient::new("sk-with\nnewline", "dall-e-3", None, None),
            Err(ImageGenError::Config(_))
        ));
    }
}
