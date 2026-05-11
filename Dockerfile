# syntax=docker/dockerfile:1.7
# AgentTheSpire — ats-web 多阶段构建。
# 构建上下文：rust/ 目录。

# ---- Stage 1: 前端构建 ----------------------------------------------------
FROM node:lts AS frontend
WORKDIR /app
COPY package*.json ./
RUN npm ci
COPY src/ src/
COPY public/ public/
COPY index.html vite.config.ts tsconfig.json tailwind.config.js postcss.config.js ./
RUN npm run build:web

# ---- Stage 2: Rust 构建（musl 静态链接） ----------------------------------
FROM rust:1-alpine AS backend
RUN apk add --no-cache musl-dev pkgconfig
WORKDIR /app

# workspace + 所有 member 的 Cargo.toml 都要拷过来，否则 cargo 无法解析依赖图。
# src-tauri 的源也要带上（只是为了 workspace 解析，不会编译——我们只 build -p ats-web）。
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY src-tauri/Cargo.toml src-tauri/Cargo.toml
COPY src-tauri/build.rs src-tauri/build.rs
COPY src-tauri/src/ src-tauri/src/

# 内嵌前端产物
COPY --from=frontend /app/dist dist/

RUN cargo build -p ats-web --release

# ---- Stage 3: 运行时 ------------------------------------------------------
FROM alpine:latest
RUN apk add --no-cache ca-certificates
COPY --from=backend /app/target/release/ats-web /usr/local/bin/ats-web
EXPOSE 7860
ENV ATS_HOST=0.0.0.0
ENV ATS_PORT=7860
ENTRYPOINT ["/usr/local/bin/ats-web"]
