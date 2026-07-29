//! BgRemoverChain：先试 ML 实现，失败 / 不可用时回退到启发式。
//!
//! 不依赖 `ml-rembg` feature —— ML 后端通过 `Option<Arc<dyn ImageProcClient>>`
//! 注入。Feature off 时直接传 None，行为等同于裸 SimpleBgRemover。
//!
//! 设计取向：handler 不关心是 ML 还是启发式，只看 trait。Primary 失败时 Chain
//! 记录 tracing::warn 并尝试启发式；fallback 失败仍向上传播，最终输出还必须通过
//! handler 的质量门禁，不能仅因为任一路径返回字节就交付。

use std::sync::Arc;

use async_trait::async_trait;

use super::{ImageProcClient, ImageProcError, SimpleBgRemover};

pub struct BgRemoverChain {
    primary: Option<Arc<dyn ImageProcClient>>,
    fallback: Arc<dyn ImageProcClient>,
}

impl BgRemoverChain {
    /// 装一个主+回退。`primary=None` 时只走 fallback（行为等同 fallback）。
    #[must_use]
    pub fn new(
        primary: Option<Arc<dyn ImageProcClient>>,
        fallback: Arc<dyn ImageProcClient>,
    ) -> Self {
        Self { primary, fallback }
    }

    /// 便捷构造：fallback 用默认 SimpleBgRemover。
    #[must_use]
    pub fn with_simple_fallback(primary: Option<Arc<dyn ImageProcClient>>) -> Self {
        Self {
            primary,
            fallback: Arc::new(SimpleBgRemover::default()),
        }
    }
}

#[async_trait]
impl ImageProcClient for BgRemoverChain {
    async fn remove_background(&self, input_png: &[u8]) -> Result<Vec<u8>, ImageProcError> {
        if let Some(p) = &self.primary {
            match p.remove_background(input_png).await {
                Ok(out) => return Ok(out),
                Err(err) => {
                    tracing::warn!("ML bg remover failed, falling back to heuristic: {err}");
                }
            }
        }
        self.fallback.remove_background(input_png).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct OkClient {
        marker: u8,
        calls: Mutex<u32>,
    }
    #[async_trait]
    impl ImageProcClient for OkClient {
        async fn remove_background(&self, _input: &[u8]) -> Result<Vec<u8>, ImageProcError> {
            *self.calls.lock().unwrap() += 1;
            Ok(vec![self.marker])
        }
    }

    struct FailingClient {
        calls: Mutex<u32>,
    }
    #[async_trait]
    impl ImageProcClient for FailingClient {
        async fn remove_background(&self, _input: &[u8]) -> Result<Vec<u8>, ImageProcError> {
            *self.calls.lock().unwrap() += 1;
            Err(ImageProcError::Decode("simulated".into()))
        }
    }

    fn ok_client(marker: u8) -> Arc<OkClient> {
        Arc::new(OkClient {
            marker,
            calls: Mutex::new(0),
        })
    }

    fn fail_client() -> Arc<FailingClient> {
        Arc::new(FailingClient {
            calls: Mutex::new(0),
        })
    }

    #[tokio::test]
    async fn primary_success_skips_fallback() {
        let primary = ok_client(1);
        let fallback = ok_client(2);
        let chain = BgRemoverChain::new(
            Some(primary.clone() as Arc<dyn ImageProcClient>),
            fallback.clone() as Arc<dyn ImageProcClient>,
        );
        let out = chain.remove_background(&[]).await.unwrap();
        assert_eq!(out, vec![1]);
        assert_eq!(*primary.calls.lock().unwrap(), 1);
        assert_eq!(*fallback.calls.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn primary_failure_falls_back() {
        let primary = fail_client();
        let fallback = ok_client(9);
        let chain = BgRemoverChain::new(
            Some(primary.clone() as Arc<dyn ImageProcClient>),
            fallback.clone() as Arc<dyn ImageProcClient>,
        );
        let out = chain.remove_background(&[]).await.unwrap();
        assert_eq!(out, vec![9]);
        assert_eq!(*primary.calls.lock().unwrap(), 1);
        assert_eq!(*fallback.calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn no_primary_goes_straight_to_fallback() {
        let fallback = ok_client(7);
        let chain = BgRemoverChain::new(None, fallback.clone() as Arc<dyn ImageProcClient>);
        let out = chain.remove_background(&[]).await.unwrap();
        assert_eq!(out, vec![7]);
        assert_eq!(*fallback.calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn fallback_failure_propagates() {
        let primary = fail_client();
        let fallback = fail_client();
        let chain = BgRemoverChain::new(
            Some(primary.clone() as Arc<dyn ImageProcClient>),
            fallback.clone() as Arc<dyn ImageProcClient>,
        );
        let err = chain.remove_background(&[]).await.unwrap_err();
        assert!(matches!(err, ImageProcError::Decode(_)));
    }
}
