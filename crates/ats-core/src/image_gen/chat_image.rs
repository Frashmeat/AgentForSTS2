//! 通过 `/v1/chat/completions` 生图的客户端（Gemini / Nano Banana 等）。
//!
//! 区别于标准 DALL-E 的 `/v1/images/generations`：此类模型的文生图能力通过
//! Chat Completions 接口暴露，图像配置经 `extra_body.google.image_config`
//! 透传给底层模型（LiteLLM / new-api 代理通用）。
//!
//! 响应格式与标准 chat completion 一致：`choices[0].message.content` 中包含
//! Markdown 图片链接或 data URL，从响应中二次提取图片字节。

use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue, RETRY_AFTER};
use serde::Deserialize;
use serde_json::Value;

use super::client::{
    GeneratedImage, ImageGenClient, ImageGenError, ImageGenRequest, ImageGenResponse,
};
use super::openai_images::map_http_error;

const DEFAULT_TIMEOUT_SECS: u64 = 300;

pub struct ChatImageClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl ChatImageClient {
    pub fn new(
        api_key: impl Into<String>,
        model: impl Into<String>,
        base_url: Option<String>,
    ) -> Result<Self, ImageGenError> {
        let api_key = api_key.into();
        if api_key.is_empty() {
            return Err(ImageGenError::Config("api_key is empty".into()));
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .map_err(|e| ImageGenError::Config(format!("build http client: {e}")))?;
        Ok(Self {
            http,
            base_url: base_url
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "https://api.openai.com".into()),
            api_key,
            model: model.into(),
        })
    }

    fn headers(&self) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        h.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.api_key))
                .expect("invalid auth header"),
        );
        h
    }

    fn chat_endpoint(&self) -> String {
        format!("{}/v1/chat/completions", self.base_url.trim_end_matches('/'))
    }

    fn build_body(&self, request: &ImageGenRequest) -> Value {
        let (ar, imgsz) = size_to_gemini_config(request.size.as_deref());
        let model = request
            .model
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.model.clone());
        serde_json::json!({
            "model": model,
            "messages": [{ "role": "user", "content": request.prompt }],
            "stream": false,
            "extra_body": {
                "google": {
                    "image_config": {
                        "aspect_ratio": ar,
                        "image_size": imgsz,
                    }
                }
            }
        })
    }
}

/// 将 DALL-E 尺寸字符串映射为 Gemini `aspect_ratio` + `image_size`。
fn size_to_gemini_config(size: Option<&str>) -> (String, String) {
    let size = size.unwrap_or("1024x1024");
    let parts: Vec<&str> = size.split('x').collect();
    let (w, h) = if parts.len() == 2 {
        let w: f64 = parts[0].parse().unwrap_or(1024.0);
        let h: f64 = parts[1].parse().unwrap_or(1024.0);
        (w, h)
    } else {
        (1024.0, 1024.0)
    };
    let ratio = w / h;
    let aspect_ratio = if (ratio - 1.0).abs() < 0.02 {
        "1:1".to_string()
    } else if (ratio - 16.0 / 9.0).abs() < 0.05 {
        "16:9".to_string()
    } else if (ratio - 9.0 / 16.0).abs() < 0.05 {
        "9:16".to_string()
    } else if (ratio - 4.0 / 3.0).abs() < 0.05 {
        "4:3".to_string()
    } else if (ratio - 3.0 / 4.0).abs() < 0.05 {
        "3:4".to_string()
    } else {
        "1:1".to_string()
    };
    let max_dim = w.max(h);
    let image_size = if max_dim <= 1536.0 {
        "1K"
    } else if max_dim <= 2048.0 {
        "2K"
    } else {
        "4K"
    }
    .to_string();
    (aspect_ratio, image_size)
}

/// 从 chat completion 响应文本中提取图片字节。
async fn extract_image_from_content(
    http: &reqwest::Client,
    content: &str,
) -> Result<Vec<u8>, ImageGenError> {
    if let Some(start) = content.find("data:image/") {
        let payload = &content[start..];
        if let Some(comma) = payload.find(',') {
            let b64 = &payload[comma + 1..];
            let end = b64
                .find(|c: char| c.is_whitespace() || c == '"' || c == ')')
                .unwrap_or(b64.len());
            let clean = &b64[..end];
            return base64::engine::general_purpose::STANDARD
                .decode(clean)
                .map_err(|e| ImageGenError::Parse(format!("b64 decode: {e}")));
        }
    }

    let url_opt = content
        .find("![")
        .and_then(|img_start| {
            let after = &content[img_start..];
            let paren = after.find("](")?;
            let url_start = img_start + paren + 2;
            let url_rest = &content[url_start..];
            let end = url_rest.find(')')?;
            Some(&content[url_start..url_start + end])
        })
        .or_else(|| {
            let trimmed = content.trim();
            if trimmed.starts_with("http") {
                let end = trimmed
                    .find(|c: char| c.is_whitespace())
                    .unwrap_or(trimmed.len());
                Some(&trimmed[..end])
            } else {
                None
            }
        });

    if let Some(url) = url_opt {
        let resp = http
            .get(url)
            .timeout(Duration::from_secs(60))
            .send()
            .await
            .map_err(|e| ImageGenError::Transport(format!("image download: {e}")))?;
        if !resp.status().is_success() {
            return Err(ImageGenError::Parse(format!(
                "image download HTTP {}",
                resp.status()
            )));
        }
        return resp
            .bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| ImageGenError::Transport(format!("image download read: {e}")));
    }

    Err(ImageGenError::Parse(
        "chat response did not contain image URL or data URI".into(),
    ))
}

#[async_trait]
impl ImageGenClient for ChatImageClient {
    async fn generate(&self, request: ImageGenRequest) -> Result<ImageGenResponse, ImageGenError> {
        const MAX_ATTEMPTS: u32 = 3;
        const BASE_BACKOFF_MS: u64 = 2000;

        let mut last_msg: Option<String> = None;
        for attempt in 0..MAX_ATTEMPTS {
            if attempt > 0 {
                let ms = BASE_BACKOFF_MS * (1u64 << (attempt - 1));
                let jitter = (ms as f64 * 0.3) as u64;
                tokio::time::sleep(Duration::from_millis(ms + jitter)).await;
            }
            match self.try_generate(&request).await {
                Ok(resp) => return Ok(resp),
                Err(ImageGenError::Http { status, message })
                    if status == 524 || status == 502 || status == 503 =>
                {
                    last_msg = Some(format!("attempt {}/{MAX_ATTEMPTS}: {message}", attempt + 1));
                    continue;
                }
                Err(ImageGenError::Transport(ref msg))
                    if msg.contains("524")
                        || msg.contains("502")
                        || msg.contains("503")
                        || msg.contains("timeout") =>
                {
                    last_msg = Some(format!("attempt {}/{MAX_ATTEMPTS}: {msg}", attempt + 1));
                    continue;
                }
                Err(err) => return Err(err),
            }
        }
        Err(ImageGenError::Transport(last_msg.unwrap_or_else(|| "max retries exhausted".into())))
    }
}
impl ChatImageClient {
    async fn try_generate(&self, request: &ImageGenRequest) -> Result<ImageGenResponse, ImageGenError> {
        let body = self.build_body(request);
        let response = self
            .http
            .post(self.chat_endpoint())
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

        let parsed: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| ImageGenError::Parse(format!("response json: {e}")))?;

        let content = parsed
            .choices
            .first()
            .and_then(|c| c.message.content.as_deref())
            .unwrap_or("");

        if content.is_empty() {
            return Err(ImageGenError::Empty);
        }

        let bytes = extract_image_from_content(&self.http, content).await?;

        Ok(ImageGenResponse {
            model: parsed.model.clone().unwrap_or_else(|| self.model.clone()),
            images: vec![GeneratedImage {
                bytes,
                format_hint: "png".into(),
            }],
            revised_prompt: None,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    #[serde(default)]
    model: Option<String>,
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Debug, Deserialize)]
struct Message {
    #[serde(default)]
    content: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_to_gemini_1k_square() {
        let (ar, sz) = size_to_gemini_config(Some("1024x1024"));
        assert_eq!(ar, "1:1");
        assert_eq!(sz, "1K");
    }

    #[test]
    fn size_to_gemini_2k_16_9() {
        let (ar, sz) = size_to_gemini_config(Some("1920x1080"));
        assert_eq!(ar, "16:9");
        assert_eq!(sz, "2K");
    }

    #[test]
    fn size_to_gemini_4k_square() {
        let (ar, sz) = size_to_gemini_config(Some("4096x4096"));
        assert_eq!(ar, "1:1");
        assert_eq!(sz, "4K");
    }

    #[test]
    fn size_to_gemini_defaults_to_1k_square() {
        let (ar, sz) = size_to_gemini_config(None);
        assert_eq!(ar, "1:1");
        assert_eq!(sz, "1K");
    }
}
