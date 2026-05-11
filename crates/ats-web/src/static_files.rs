//! 通过 rust-embed 把 `dist/`（前端构建产物）编译进二进制，
//! 启动后直接服务，免外置 web server。
//!
//! SPA fallback：未命中文件时返回 `index.html`，让 react-router 接管路由。

use axum::http::{StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../../dist"]
struct Asset;

pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    if !path.is_empty() {
        if let Some(content) = Asset::get(path) {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            return (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime.as_ref())],
                content.data.into_owned(),
            )
                .into_response();
        }
    }

    match Asset::get("index.html") {
        Some(content) => Html(String::from_utf8_lossy(&content.data).to_string()).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            "Frontend not built. Run `npm run build:web` first.",
        )
            .into_response(),
    }
}
