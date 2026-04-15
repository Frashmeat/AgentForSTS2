# 服务器模式执行器真实调度链路

## Goal
打通服务器模式下平台执行编排的最小真实调度链路，让执行侧不再依赖外部手工传入 `provider / model / credential_ref`，而是能够基于任务上的 `selected_execution_profile_id` 从后台已启用且健康的凭据池中解析出实际命中快照，并为后续真正接入执行器提供稳定的后端真源。

## Requirements
- 补一个面向执行期的凭据解析能力，从 `job.selected_execution_profile_id` 解析出可执行的 `execution_profile + server_credential` 组合。
- 解析结果至少包含：
  - `provider`
  - `model`
  - `credential_ref`
  - `retry_attempt`
  - `switched_credential`
- 第一版调度只选择同组合下首个“已启用且健康”的凭据，排序规则沿用 `priority asc, id asc`。
- `credential_ref` 第一版不新增 schema，直接使用稳定内部引用格式，由现有 `server_credentials.id` 派生。
- 如所选 `execution_profile` 不存在、不可用，或没有可调度凭据，后端必须返回清晰错误，不得静默回退到空字符串或伪默认值。
- 真实明文凭据只允许在内部执行链使用，不得写入响应、日志、事件 payload 或异常文本。
- 同步更新实施计划、接口冻结稿和阶段总结，明确“执行器真实调度链路”的实际落地边界。

## Non-Goals
- 不在本次实现中补前端服务器模式选择或设置页体验。
- 不在本次实现中补完整的 `ak_sk` / 更多 provider 健康检测能力。
- 不在本次实现中强行接通 legacy `workflow` / `batch_workflow` 的全量运行时委托。
- 不在本次实现中落“同组合自动切换一次”的正式重试策略，除非调查证明现有失败重试链已具备稳定挂点。

## Acceptance Criteria
- [ ] 后端存在可复用的执行期凭据解析服务或仓储抽象。
- [ ] `ExecutionOrchestratorService` 在创建 `ai_execution` 时可以基于任务快照自动写入实际命中的 `provider / model / credential_ref`。
- [ ] 无可用配置或凭据时，服务层能给出稳定且可测试的失败结果。
- [ ] 第一版 `credential_ref` 生成规则在代码与文档中口径一致。
- [ ] 至少补齐执行编排服务与相关仓储的定向测试。
- [ ] 相关专题文档已同步回写，并和当前代码行为一致。

## Technical Notes
- 当前仓库事实是：`ExecutionOrchestratorService.start_execution()` 尚无真实业务调用点，`run_registered_steps()` 也只在测试中调用，因此本次实现应优先建立“调度真源”和“执行快照写入”能力，而不是假装已经接通完整运行时。
- 当前 `execution_profile` 已冻结为用户可见的 `CLI + model` 组合；`server_credentials` 已冻结为后台真实凭据池；两者之间的执行期解析应成为独立能力，而不是继续由 router 或外部调用者手填。
- 现有 `server_credentials` 表没有独立 `credential_ref` 字段；第一版建议直接生成稳定内部引用，如 `server-credential:{id}`，避免为了单个审计字段扩大 migration 范围。
- 若后续真正进入执行层需要明文 `credential / secret / base_url`，应在内部解析结果对象中携带，但不得进入 `ai_execution` 表之外的可见输出。
