# ADR-0003 — 启动 / 停机生命周期

| 项 | 值 |
| --- | --- |
| 日期 | 2026-05-12 |
| 状态 | 采纳 |
| 作用域 | `src-tauri` 桌面壳 + `ats-web` HTTP 服务器 |
| 相关代码 | `src-tauri/src/lib.rs`、`crates/ats-web/src/main.rs`、`src-tauri/src/commands/image_proc_state.rs` |

---

## 1. 背景

Stage 5 装配收口要求双壳启动 / 停机有可预测的顺序，避免"路由先 bind 但 db 还没 ping
通"、"app 退出但 LLM 流还在写文件"等竞态。本 ADR 把当前实现冻成契约，便于 N+1
个新模块（ML prewarm / queue worker / 后续 sqlx）按位插入。

---

## 2. 决策

### 2.1 ats-web 启动顺序（HTTP server / Web 角色）

```
1. tracing_subscriber 初始化（最先；后续所有日志都靠它）
2. clap 解析 CLI args（host / port / config）
3. Settings::load(config_path)              // figment 三层 merge
4. settings.validate_for_role(Role::Web)    // 缺 db.url 等字段会让 config_status.loaded=false
5. 装 AppState（config_status + runtime_dir + settings_snapshot）
6. 装 Router（health → knowledge → planning → codegen → llm，外加 cors / static_files）
7. TcpListener::bind(host:port)             // 失败立刻返回 main 的 ? 退出码 1
8. tracing::info!("ats-web listening on http://...")
9. axum::serve(...).with_graceful_shutdown(shutdown_signal())
   ↓ 直到收到信号
10. tracing::info!("ats-web shut down cleanly")
```

**未来插入位**（按依赖序）：
- 第 5 步后：`PgPool::connect(...)` + `migrate!().run(&pool).await`（Stage 3.1a）
- 第 6 步前：`spawn queue worker`（Stage 3.4）
- 第 9 步后、main 退出前：`worker.stop().await + pool.close().await`

### 2.2 ats-web 停机顺序

`shutdown_signal()` 监听两路信号：
- `tokio::signal::ctrl_c()`：跨平台
- Unix 上 `SignalKind::terminate()`：docker stop / systemctl 用的

任一触发 → `axum::serve` 进入"不再 accept 新连接 + 等 in-flight 请求 100%
完成"。当前没有 worker / db 要 drain，等 axum 自身退就够。

### 2.3 src-tauri 启动顺序（桌面壳 / Workstation 角色）

```
1. Settings::load(None)                     // 桌面端不暴露 --config，让 figment 走默认
2. settings.validate_for_role(Role::Workstation)
3. AppDataPaths::resolve() + ensure_dirs()  // %APPDATA%/AgentTheSpire/ 等
4. 构造 Arc<ImageProcState>（用于跨 setup ↔ command 共享）
5. tauri::Builder::default()
     .plugin(shell/fs/dialog/process)       // 顺序无要求，互不依赖
     .setup(|app| {
         // 后台 spawn ML prewarm —— 不阻塞 main：feature off 立刻置 Failed，
         // feature on 下载 ~5MB 模型 + 加载 ort session 走 spawn_blocking。
         tauri::async_runtime::spawn(prewarm(state, app_data_root));
         Ok(())
     })
     .manage(AppConfig)                     // config + status
     .manage(AppPaths)                      // recents_path 等
     .manage(ActiveProject::new())          // 内部 Mutex<Option<ProjectFolder>>
     .manage(image_proc_state)              // Arc<ImageProcState>
     .invoke_handler(generate_handler![...]) // ~45 command
     .run(generate_context!())              // 阻塞直到窗口关闭
```

**契约**：
- `setup` 里**只能 spawn**，不能 await 同步阻塞 —— 阻塞会导致 webview 加载延迟可见。
- 任何"启动期非致命预热"（ML / 缓存预热 / index 重建）都走 `setup → spawn`
  并把 status 写到 `manage()` 出去的 State，让前端轮询拿到进度。
- 致命错误（不能 ensure_dirs、不能解析 config）应该 `eprintln + 继续`，不要
  panic；让 webview 起来后用 HealthCard 告诉用户。

### 2.4 src-tauri 停机顺序

由 Tauri 内核接管 —— 用户点窗口 X / Cmd+Q / `Window::close()` 触发，
`tauri::Builder::run()` 返回，进程退出。当前没有显式 cleanup hook；
若后续有"需要 drain 的"（如真有后台 queue worker 持久化写盘），
要走 `app_handle.run_on_main_thread` + drop guard 模式。

---

## 3. 反过来不行的选择

### 3.1 为什么 prewarm 不放在 main thread 同步执行

ML 模型下载（首启 5MB）+ ort session 加载（~500ms）会让 webview 多
0.5-3s 才显示。setup 里的 spawn 让窗口立即出现，prewarm 状态通过
`image_proc_status` 命令异步暴露，UX 远好于"白屏几秒"。

### 3.2 为什么 ats-web 不用 worker pool 而是 axum 自身 accept loop

桌面单实例用户场景，axum 内部的 tokio 调度足够；引 worker pool
（如 deadpool / bb8）只在多 worker 节点 + 显式排队语义时才有意义，
那是 Stage 3.4 Web 轨的事。

### 3.3 为什么 Ctrl+C / SIGTERM 都监听，不只监听 Ctrl+C

docker / k8s / systemd 默认发 SIGTERM；只装 Ctrl+C 会让 docker stop
超时后被 SIGKILL，长任务请求被硬切。Unix 双信号才完整。

---

## 4. 修改契约的成本

| 改动方向 | 影响面 |
| --- | --- |
| 加新"启动期同步"步骤 | 该步骤必须能在 < 100ms 完成；否则改 setup spawn |
| 加新 manage 状态 | 同步加入 invoke_handler 暴露的 command 签名（State 提取） |
| 改 axum 路由树 | 同时检查 src-tauri 的 generate_handler! 列表，保持双壳对等 |
| 砍掉 graceful_shutdown | 长任务请求会被 SIGKILL；只在性能压测 / 强制 kill 场景才能这么做 |

---

## 5. 引用

- ats-web 实现：`crates/ats-web/src/main.rs:49+`（含 `shutdown_signal()` 定义）
- src-tauri 实现：`src-tauri/src/lib.rs:31+`（`tauri::Builder::default().setup(...)`）
- prewarm 任务：`src-tauri/src/commands/image_proc_state.rs:75+`
- 健康字段（启动后 UI 可见）：`crates/ats-core/src/health.rs:22+` `ReadinessFlags`
