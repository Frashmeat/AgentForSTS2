# 平台后端 Rollout 清单

## 目标

- 在不移除 legacy 入口的前提下，逐步启用平台后端主链。
- 保证平台 API、legacy compat service 和 runner 开关可以独立观察、逐步回退。

## 上线前检查

- 确认数据库已创建且已执行当前平台 schema 对应 SQL / migration。
- 确认 `platform.db_session_factory` 可解析，数据库连接可正常打开。
- 确认平台表存在最小基线数据：
  - `quota_accounts`
  - `quota_buckets`
  - `jobs`
  - `job_items`
  - `job_events`
- 确认本轮定向测试至少已通过：
  - `backend/tests/platform/e2e/test_platform_rollout_flags.py`
  - `backend/tests/platform/e2e/test_platform_job_lifecycle.py`
  - 当前 e2e 已覆盖成功链路、cancel、quota exhausted、refund 与 rollout flags

## 开关启用顺序

1. 启用 `platform_jobs_api_enabled`
2. 验证 `/api/platform/jobs` 与 `/api/platform/quota` 可访问
3. 启用 `platform_service_split_enabled`
4. 验证 `/api/admin/*` 查询接口可访问
5. 启用 `platform_runner_enabled`
6. 验证 `workflow` / `batch_workflow` 在 compat service 路径下可写入平台主链
7. 视需要启用 `platform_events_v1_enabled`
8. 视需要启用 `platform_step_protocol_enabled`

## 每步观察点

- `platform_jobs_api_enabled`
  - 观察 `/api/platform/jobs` 不再返回 `404`
  - 若数据库未配置，预期返回 `503`
- `platform_service_split_enabled`
  - 观察 `/api/admin/quota/refunds` 不再返回 `404`
  - 若数据库未配置，预期返回 `503`
- `platform_runner_enabled`
  - 观察 `workflow` / `batch_workflow` compat service 已绑定 `session_factory`
  - 观察 `custom_code` 最小闭环会生成 `job.created -> job.queued -> job.item.completed -> job.completed`
  - 观察无可用 quota 时，平台主链会把 job 标记为 `quota_exhausted`、item 标记为 `quota_skipped`
- `platform_events_v1_enabled`
  - 当前阶段为配置占位，启用后应保证不破坏现有入口
- `platform_step_protocol_enabled`
  - 当前阶段为配置占位，启用后应保证不破坏现有入口

## 建议烟测

1. 调用 `POST /api/platform/jobs` 创建任务
2. 调用 `POST /api/platform/jobs/{job_id}/start` 启动任务
3. 调用 `GET /api/platform/jobs/{job_id}`、`/items`、`/events` 验证主链写入
4. 调用 `GET /api/platform/quota` 验证 quota 查询
5. 调用 `GET /api/admin/jobs/{job_id}/executions` 或 `GET /api/admin/audit/events` 验证管理员查询
6. 对一个已 reserve 的 execution 执行 refund，验证 `/api/platform/quota` 的 `refunded` 与 `/api/admin/quota/refunds` 一致
7. 对 `ws/create` 发送 `custom_code` 请求，确认 compat service 走平台 runner 路径

## 回退顺序

1. 先关闭 `platform_runner_enabled`
2. 再关闭 `platform_service_split_enabled`
3. 最后关闭 `platform_jobs_api_enabled`
4. `platform_events_v1_enabled` 与 `platform_step_protocol_enabled` 作为占位开关，可单独关闭

## 回退判定

- `workflow` / `batch_workflow` 出现非预期回落到 legacy 或平台主链写入失败
- 平台 API 出现持续 `5xx`
- 管理员查询接口返回结构异常
- 事件链缺失 `job.created` / `job.queued` / `job.completed`
- quota exhausted 后 job/item 状态与查询结果不一致
- refund 后 `/api/platform/quota` 与 `/api/admin/quota/refunds` 结果不一致

## 当前已知边界

- `workflow` 当前只对 `custom_code` 走平台主链深化
- `batch_workflow` 当前只对 `start_with_plan + custom_code + needs_image=false` 走平台主链深化
- 生图、审批优先、复杂 batch 依赖编排仍保留 legacy 回退
- `platform_events_v1_enabled` 与 `platform_step_protocol_enabled` 当前主要用于 rollout 占位，不代表已有独立新入口
