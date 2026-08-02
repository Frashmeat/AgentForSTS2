use std::time::Duration;

use async_trait::async_trait;
use ats_kernel::PrimitiveId;
use ats_runtime::{
    CancellationToken, MediaClient, MediaError, MediaRequestSnapshot, MediaResponse,
};
use base64::Engine;
use reqwest::{Client, StatusCode, Url};
use serde_json::{Value, json};

use crate::ImageGenerationConfig;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum Protocol {
    Images,
    ChatCompletions,
}

#[derive(Debug, Clone)]
pub struct HttpMediaClient {
    client: Client,
    protocol: Protocol,
    base_url: Url,
    api_key: String,
    default_model: String,
}

impl HttpMediaClient {
    pub fn new(config: &ImageGenerationConfig) -> Result<Self, MediaError> {
        if config.api_key.trim().is_empty()
            || config.api_key.contains(['\r', '\n'])
            || config.base_url.contains(['\r', '\n'])
        {
            return Err(MediaError::Configuration);
        }
        let protocol = match config.protocol.trim().to_ascii_lowercase().as_str() {
            "" | "images_api" => Protocol::Images,
            "chat_completions" => Protocol::ChatCompletions,
            _ => return Err(MediaError::Configuration),
        };
        let base_url = Url::parse(if config.base_url.trim().is_empty() {
            "https://api.openai.com"
        } else {
            config.base_url.trim()
        })
        .map_err(|_| MediaError::Configuration)?;
        if !matches!(base_url.scheme(), "http" | "https") || base_url.host_str().is_none() {
            return Err(MediaError::Configuration);
        }
        let default_model = if config.model.trim().is_empty() {
            "dall-e-3".into()
        } else {
            config.model.trim().into()
        };
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|_| MediaError::Configuration)?;
        Ok(Self {
            client,
            protocol,
            base_url,
            api_key: config.api_key.trim().into(),
            default_model,
        })
    }

    fn endpoint(&self) -> Result<Url, MediaError> {
        let suffix = match self.protocol {
            Protocol::Images => "images/generations",
            Protocol::ChatCompletions => "chat/completions",
        };
        let mut base = self.base_url.as_str().trim_end_matches('/').to_owned();
        if !base.ends_with("/v1") {
            base.push_str("/v1");
        }
        Url::parse(&format!("{base}/{suffix}")).map_err(|_| MediaError::Configuration)
    }

    fn body(&self, snapshot: &MediaRequestSnapshot) -> Value {
        let request = snapshot.request();
        let model = request.model.as_deref().unwrap_or(&self.default_model);
        let (width, height) = request.width.zip(request.height).unwrap_or((1024, 1024));
        match self.protocol {
            Protocol::Images => json!({
                "model": model,
                "prompt": request.prompt,
                "n": 1,
                "size": format!("{width}x{height}"),
                "response_format": "b64_json",
            }),
            Protocol::ChatCompletions => {
                let (aspect_ratio, image_size) = image_dimensions(width, height);
                json!({
                    "model": model,
                    "messages": [{"role": "user", "content": request.prompt}],
                    "stream": false,
                    "extra_body": {"google": {"image_config": {
                        "aspect_ratio": aspect_ratio,
                        "image_size": image_size,
                    }}},
                })
            }
        }
    }

    async fn generate_inner(
        &self,
        snapshot: &MediaRequestSnapshot,
    ) -> Result<MediaResponse, MediaError> {
        if snapshot.request().media_type != "image/png" {
            return Err(MediaError::InvalidRequest);
        }
        let response = self
            .client
            .post(self.endpoint()?)
            .bearer_auth(&self.api_key)
            .json(&self.body(snapshot))
            .send()
            .await
            .map_err(|_| MediaError::Transport)?;
        let status = response.status();
        let retry_after_ms = response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .map(|seconds| seconds.saturating_mul(1_000));
        if !status.is_success() {
            return Err(match status {
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => MediaError::Authentication,
                StatusCode::TOO_MANY_REQUESTS => MediaError::RateLimited { retry_after_ms },
                value if value.is_server_error() => MediaError::Transport,
                _ => MediaError::Rejected,
            });
        }
        let value: Value = response
            .json()
            .await
            .map_err(|_| MediaError::InvalidResponse)?;
        let bytes = match self.protocol {
            Protocol::Images => decode_images_response(&value)?,
            Protocol::ChatCompletions => {
                let content = value
                    .get("choices")
                    .and_then(Value::as_array)
                    .and_then(|values| values.first())
                    .and_then(|choice| choice.get("message"))
                    .and_then(|message| message.get("content"))
                    .and_then(Value::as_str)
                    .ok_or(MediaError::InvalidResponse)?;
                self.decode_chat_content(content).await?
            }
        };
        let model = value
            .get("model")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty() && value.len() <= 256)
            .unwrap_or_else(|| {
                snapshot
                    .request()
                    .model
                    .as_deref()
                    .unwrap_or(&self.default_model)
            })
            .to_owned();
        let response = MediaResponse {
            provider: PrimitiveId::parse(match self.protocol {
                Protocol::Images => "media.openai-images",
                Protocol::ChatCompletions => "media.openai-chat",
            })
            .expect("built-in media Primitive ID is valid"),
            model,
            media_type: "image/png".into(),
            bytes,
        };
        response.validate()?;
        Ok(response)
    }

    async fn decode_chat_content(&self, content: &str) -> Result<Vec<u8>, MediaError> {
        if let Some(bytes) = decode_data_uri(content)? {
            return Ok(bytes);
        }
        let raw_url = markdown_image_url(content).or_else(|| {
            let trimmed = content.trim();
            trimmed.starts_with("http").then(|| {
                trimmed
                    .split_ascii_whitespace()
                    .next()
                    .unwrap_or(trimmed)
                    .to_owned()
            })
        });
        let url = raw_url
            .and_then(|value| Url::parse(&value).ok())
            .filter(|value| matches!(value.scheme(), "http" | "https"))
            .ok_or(MediaError::InvalidResponse)?;
        let response = self
            .client
            .get(url)
            .timeout(Duration::from_secs(60))
            .send()
            .await
            .map_err(|_| MediaError::Transport)?;
        if !response.status().is_success() {
            return Err(MediaError::InvalidResponse);
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| MediaError::Transport)?
            .to_vec();
        validate_media_bytes(bytes)
    }
}

#[async_trait]
impl MediaClient for HttpMediaClient {
    async fn generate(
        &self,
        request: MediaRequestSnapshot,
        cancellation: &CancellationToken,
    ) -> Result<MediaResponse, MediaError> {
        request.verify()?;
        if cancellation.is_cancelled() {
            return Err(MediaError::Cancelled);
        }
        tokio::select! {
            result = self.generate_inner(&request) => result,
            () = wait_cancelled(cancellation) => Err(MediaError::Cancelled),
        }
    }
}

fn decode_images_response(value: &Value) -> Result<Vec<u8>, MediaError> {
    let encoded = value
        .get("data")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
        .and_then(|image| image.get("b64_json"))
        .and_then(Value::as_str)
        .ok_or(MediaError::InvalidResponse)?;
    decode_base64(encoded)
}

fn decode_data_uri(content: &str) -> Result<Option<Vec<u8>>, MediaError> {
    let Some(start) = content.find("data:image/png;base64,") else {
        return Ok(None);
    };
    let encoded = &content[start + "data:image/png;base64,".len()..];
    let end = encoded
        .find(|character: char| character.is_whitespace() || matches!(character, '"' | ')'))
        .unwrap_or(encoded.len());
    decode_base64(&encoded[..end]).map(Some)
}

fn decode_base64(encoded: &str) -> Result<Vec<u8>, MediaError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|_| MediaError::InvalidResponse)?;
    validate_media_bytes(bytes)
}

fn validate_media_bytes(bytes: Vec<u8>) -> Result<Vec<u8>, MediaError> {
    if bytes.is_empty() || bytes.len() > 64 * 1024 * 1024 {
        Err(MediaError::InvalidResponse)
    } else {
        Ok(bytes)
    }
}

fn markdown_image_url(content: &str) -> Option<String> {
    let image = content.find("![")?;
    let suffix = &content[image..];
    let opening = suffix.find("](")? + image + 2;
    let closing = content[opening..].find(')')? + opening;
    Some(content[opening..closing].to_owned())
}

fn image_dimensions(width: u32, height: u32) -> (&'static str, &'static str) {
    let ratio = f64::from(width) / f64::from(height);
    let aspect = [
        (1.0, "1:1"),
        (16.0 / 9.0, "16:9"),
        (9.0 / 16.0, "9:16"),
        (4.0 / 3.0, "4:3"),
        (3.0 / 4.0, "3:4"),
    ]
    .into_iter()
    .min_by(|(left, _), (right, _)| (ratio - left).abs().total_cmp(&(ratio - right).abs()))
    .map_or("1:1", |(_, value)| value);
    let size = match width.max(height) {
        0..=1536 => "1K",
        1537..=2048 => "2K",
        _ => "4K",
    };
    (aspect, size)
}

async fn wait_cancelled(cancellation: &CancellationToken) {
    while !cancellation.is_cancelled() {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[cfg(test)]
mod tests {
    use ats_runtime::{MediaRequest, MediaRequestSnapshot};

    use super::*;

    fn config(protocol: &str, base_url: &str) -> ImageGenerationConfig {
        ImageGenerationConfig {
            provider: "openai".into(),
            protocol: protocol.into(),
            model: "fixture-image".into(),
            api_key: "fixture-key".into(),
            base_url: base_url.into(),
            size: String::new(),
        }
    }

    fn snapshot() -> MediaRequestSnapshot {
        MediaRequestSnapshot::new(MediaRequest {
            prompt: "fixture image".into(),
            logical_role: "relic.normal".into(),
            media_type: "image/png".into(),
            width: Some(1920),
            height: Some(1080),
            model: None,
        })
        .unwrap()
    }

    #[test]
    fn validates_protocol_configuration_and_normalizes_endpoints() {
        assert!(HttpMediaClient::new(&config("unknown", "https://example.test")).is_err());
        assert!(HttpMediaClient::new(&config("images_api", "file:///tmp")).is_err());
        let images =
            HttpMediaClient::new(&config("images_api", "https://example.test/v1")).unwrap();
        assert_eq!(
            images.endpoint().unwrap().as_str(),
            "https://example.test/v1/images/generations"
        );
        let chat =
            HttpMediaClient::new(&config("chat_completions", "https://example.test")).unwrap();
        assert_eq!(
            chat.endpoint().unwrap().as_str(),
            "https://example.test/v1/chat/completions"
        );
    }

    #[test]
    fn builds_protocol_specific_requests_from_typed_dimensions() {
        let images = HttpMediaClient::new(&config("images_api", "https://example.test")).unwrap();
        assert_eq!(images.body(&snapshot())["size"], "1920x1080");
        let chat =
            HttpMediaClient::new(&config("chat_completions", "https://example.test")).unwrap();
        assert_eq!(
            chat.body(&snapshot())["extra_body"]["google"]["image_config"]["aspect_ratio"],
            "16:9"
        );
        assert_eq!(
            chat.body(&snapshot())["extra_body"]["google"]["image_config"]["image_size"],
            "2K"
        );
    }

    #[test]
    fn decodes_bounded_images_and_chat_data_without_exposing_provider_text() {
        let encoded = base64::engine::general_purpose::STANDARD.encode(b"fixture-png");
        assert_eq!(
            decode_images_response(&json!({"data": [{"b64_json": encoded}]})).unwrap(),
            b"fixture-png"
        );
        assert_eq!(
            decode_data_uri(&format!("![image](data:image/png;base64,{encoded})"))
                .unwrap()
                .unwrap(),
            b"fixture-png"
        );
        assert!(decode_images_response(&json!({"error": "SECRET"})).is_err());
    }
}
