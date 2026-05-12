# 04-决策（索引）

> 文档定位：本文是 `docs/04-决策/` 的目录索引，负责定位已经固化的边界说明、归属规则和判定模板。
>
> 权威入口：上级入口为 `docs/README.md`。
>
> 最后更新：2026-05-04

## 专题目录

- `后端/`：后端边界、服务归属、部署说明和判定模板。

## 关键文档

- [`后端/2026-04-02-工作站端与Web端边界说明.md`](./后端/2026-04-02-工作站端与Web端边界说明.md)
- [`后端/2026-04-02-新增接口与服务归属决策模板.md`](./后端/2026-04-02-新增接口与服务归属决策模板.md)
- [`后端/2026-04-03-web后端独立部署说明.md`](./后端/2026-04-03-web后端独立部署说明.md)：历史背景文档；当前部署主线已由 `package app / deploy app / stop app` 取代。

## Rust 重写后引入的 ADR（按编号）

- [`0001-rust-tauri-workspace-architecture.md`](./0001-rust-tauri-workspace-architecture.md) — 整体架构与 workspace 划分
- [`0002-frontend-and-api-double-adapter.md`](./0002-frontend-and-api-double-adapter.md) — 前端 `services/api.ts` 双适配（Tauri ↔ Web）
- [`0003-startup-and-shutdown-lifecycle.md`](./0003-startup-and-shutdown-lifecycle.md) — Tauri / ats-web 启动与停机的有序生命周期契约
