# API / Service 拆分迁移实施计划

> **给执行型 Agent 的要求：** 必须使用 `superpowers:subagent-driven-development`（若支持子 agent）或 `superpowers:executing-plans` 来执行本计划。步骤统一使用 checkbox（`- [ ]`）语法跟踪。

**Goal:** 在不做一次性重写的前提下，将当前由 router 驱动的 FastAPI 后端，逐步迁移为面向平台模式任务的 API / Service / Runner / Repository 结构。

**Architecture:** 保留现有 FastAPI 单体以及当前 HTTP/WebSocket 入口，但把编排和持久化职责下沉到 application service 与 repository 后面。先引入 `job -> job_item -> ai_execution` 主链，再通过传输契约与 feature flag 让现有 workflow、batch、build/deploy、config、approval 流程逐步切到新结构，从而保证 rollout 可以渐进、可回退。

**Tech Stack:** Python、FastAPI、Uvicorn、Pydantic 风格 DTO/contract、现有 application container、现有基于 settings 的 migration flag、后端正在使用的 SQL migration 工具、pytest

## Decision Freeze

- Phase-1 范围包含 `admin` API，不延后到用户 API 迁移完成之后再做。
- 首发版本的状态枚举冻结为后端设计基线里定义的细粒度集合；实现和 migration 应直接使用这些枚举。
- 双路径迁移方案冻结为 `dual-write + read by flag`。
- Phase-1 的读权限仍以 legacy 读模型（`read legacy`）为准，直到 list/detail/items/events/quota/admin 视图的对齐校验和回滚测试都通过；迁移窗口内写权限采用双写。
- v1 返还矩阵（refund matrix）冻结用于实现。除非经过明确的设计评审，否则 phase-1 不再重新打开 refund 语义讨论。

## Decision Gates

- Gate A：在细粒度枚举和 v1 refund matrix 被视为冻结输入之前，不开始 schema 工作。
- Gate B：在 dual-write 对 legacy 响应完成 list/detail/items/events/quota/admin 视图对齐校验之前，不启用任何新的读路径。
- Gate C：在 legacy-only、`dual-write + read legacy`、split-read 三种模式的回滚测试通过之前，不切换 router 委托默认值。
- Gate D：在新读模型上的 polling 路径稳定之前，不启用统一事件 / WebSocket contract。

---

## File Map

- `backend/main.py`
  责任：组装 routers；最终需要把 platform API 与 legacy workflow 入口分开注册。
- `backend/routers/workflow.py`
  当前责任：单项任务传输 + 编排 + 执行 + 事件流；目标状态是精简成传输适配层。
- `backend/routers/batch_workflow.py`
  当前责任：批量传输 + 规划/编排 + 并发控制；目标状态是精简成传输适配层。
- `backend/routers/build_deploy.py`
  当前责任：构建/部署传输，并内嵌执行流程；目标状态是由构建传输层调用 application service / runner facade。
- `backend/routers/config_router.py`
  当前责任：配置传输；目标状态是 settings/query facade 与迁移开关入口。
- `backend/routers/approval_router.py`
  当前责任：审批传输；目标状态是 approval application service facade。
- `backend/app/composition/container.py`
  责任：单例/provider 组合根；后续将成为 repositories、services、runners 与 rollout toggle 的接线点。
- `backend/app/shared/infra/config/settings.py`
  责任：规范化 settings + migration flags；需要扩展以支持拆分 rollout 开关。
- `backend/app/modules/platform/contracts/*.py`
  责任：用户/管理员传输 DTO、事件 payload、runner step contract。
- `backend/app/modules/platform/application/services/*.py`
  责任：job 生命周期、编排、查询、额度/计费、approval/build facade。
- `backend/app/modules/platform/domain/repositories/*.py`
  责任：与已确认 job 主链及查询边界对齐的 repository interface。
- `backend/app/modules/platform/infra/persistence/models/*.py`
  责任：`jobs`、`job_items`、`ai_executions`、quota、events、artifacts、charges 的 ORM entity / table mapping。
- `backend/app/modules/platform/infra/persistence/repositories/*.py`
  责任：写模型与读模型的 repository 实现。
- `backend/app/modules/platform/runner/*.py`
  责任：workflow registry、step dispatcher、execution adapters、build/deploy bridge、与传输无关的执行流。
- `backend/migrations/*`
  责任：主链、quota/billing 链、event 链、artifacts、兼容字段与 backfill 防护的 schema rollout。
- `backend/tests/platform/*`
  责任：repository、service、runner、router-contract 与 migration 的回归覆盖。

## Dual-Path Authority Model

- Phase 1：API 响应仍以 legacy 数据库读为准；新 platform 表接收双写，用于 backfill 与对齐验证。
- Phase 2：在对齐通过后，部分 API 通过 flag 切到新读路径，同时继续保持 dual-write，以保证回滚安全。
- Phase 3：只有在 split-read 经过生产环境 soak 且回滚演练成功之后，才退役 legacy 读路径。
- 在任何阶段，都不允许两个独立写路径以不同业务规则修改同一个逻辑字段；即使持久化层是双写，service 层的写语义也必须保持单一事实源。

## Chunk 1: Contracts And Persistence Backbone

### Task 1: 引入平台传输契约与迁移开关

**Files:**
- Modify: `backend/app/shared/infra/config/settings.py`
- Modify: `backend/app/composition/container.py`
- Modify: `backend/main.py`
- Create: `backend/app/modules/platform/contracts/job_commands.py`
- Create: `backend/app/modules/platform/contracts/job_queries.py`
- Create: `backend/app/modules/platform/contracts/events.py`
- Create: `backend/app/modules/platform/contracts/runner_contracts.py`
- Create: `backend/tests/platform/contracts/test_platform_contracts.py`
- Create: `backend/tests/platform/config/test_platform_migration_flags.py`

- [ ] **Step 1: 定义拆分 API 与 service 的 rollout 开关**
  增加规范化 settings 字段：`platform_jobs_api_enabled`、`platform_service_split_enabled`、`platform_runner_enabled`、`platform_events_v1_enabled`，同时保持现有 migration flags 不被破坏。

- [ ] **Step 2: 在组合根中注册 contract 与 service 占位项**
  扩展 `ApplicationContainer` 接线，使后续任务可以解析 platform repositories、services 与 runner adapters，而不需要再次修改 router 代码。

- [ ] **Step 3: 为 create/start/cancel/list/detail/events/quota/admin 视图建立传输契约**
  增加请求/响应 DTO 模块，把用户可见的 `job` / `job_item` payload 与内部 `ai_execution` payload 明确分开。

- [ ] **Step 4: 建立带版本的事件契约**
  定义一套同时供 polling 和未来 WebSocket 传输共享的事件 payload 模型，并允许包含可选的 admin-only execution 字段。

- [ ] **Step 5: 在 `main.py` 中为 router 注册加开关**
  保持当前 router 继续挂载，但预先加好切换点，后续可以把 platform router 并列挂载在 legacy 入口旁边，或放在其之前。

- [ ] **Step 6: 编写 contract 与 settings 测试**
  验证 DTO 序列化、admin/user 可见性边界，以及新增 migration flag 默认关闭的行为。

**Tests:**
- `backend/tests/platform/contracts/test_platform_contracts.py`
- `backend/tests/platform/config/test_platform_migration_flags.py`

**Commands:**
- `pytest backend/tests/platform/contracts/test_platform_contracts.py -q`
- `pytest backend/tests/platform/config/test_platform_migration_flags.py -q`

**Expected:**
- Contract 模块可以稳定编译并确定性序列化。
- 新 settings 默认处于兼容模式。
- Container 接线可以解析占位项，而不影响当前启动流程。

### Task 2: 为平台 job 主链增加持久化 schema

**Files:**
- Create: `backend/migrations/versions/20260327_01_create_platform_job_chain.py`
- Create: `backend/app/modules/platform/infra/persistence/models/job.py`
- Create: `backend/app/modules/platform/infra/persistence/models/job_item.py`
- Create: `backend/app/modules/platform/infra/persistence/models/ai_execution.py`
- Create: `backend/app/modules/platform/infra/persistence/models/execution_charge.py`
- Create: `backend/app/modules/platform/infra/persistence/models/quota_account.py`
- Create: `backend/app/modules/platform/infra/persistence/models/quota_bucket.py`
- Create: `backend/app/modules/platform/infra/persistence/models/usage_ledger.py`
- Create: `backend/app/modules/platform/infra/persistence/models/artifact.py`
- Create: `backend/app/modules/platform/infra/persistence/models/job_event.py`
- Create: `backend/tests/platform/migrations/test_platform_job_chain_schema.py`

- [ ] **Step 1: 在 migration 中编码已确认的表集合与状态枚举**
  创建 `jobs`、`job_items`、`ai_executions`、`execution_charges`、`quota_accounts`、`quota_buckets`、`usage_ledgers`、`artifacts`、`job_events` 表，并直接使用已经冻结的细粒度 v1 枚举。

- [ ] **Step 2: 实现 ORM / entity 映射**
  为每个聚合 / 事实表增加一个聚焦的 model 文件，并包含设计文档中要求的软删除 / 归档字段与时间戳约定。

- [ ] **Step 3: 为 execution 幂等增加带范围的 partial unique 约束**
  仅在 `request_idempotency_key` 非空时，强制 `user_id + job_item_id + request_idempotency_key` 唯一。

- [ ] **Step 4: 为首发查询路径增加索引**
  覆盖用户 job 列表 / 详情、item 分页、最新 execution 查询、bucket 查询与 event cursor 读取。

- [ ] **Step 5: 增加 migration 校验覆盖**
  断言表、索引和唯一约束存在，且语义与约定一致。

**Tests:**
- `backend/tests/platform/migrations/test_platform_job_chain_schema.py`

**Commands:**
- `pytest backend/tests/platform/migrations/test_platform_job_chain_schema.py -q`
- `alembic upgrade head`

**Expected:**
- Schema 能表达完整的 `job -> job_item -> ai_execution` 主链，以及 quota、billing、artifacts、events。
- 幂等与查询索引由数据库保证，而不是只靠应用代码。

### Task 3: 实现 repository interface 与首轮持久化 adapter

**Files:**
- Create: `backend/app/modules/platform/domain/repositories/job_repository.py`
- Create: `backend/app/modules/platform/domain/repositories/job_query_repository.py`
- Create: `backend/app/modules/platform/domain/repositories/quota_query_repository.py`
- Create: `backend/app/modules/platform/domain/repositories/ai_execution_repository.py`
- Create: `backend/app/modules/platform/domain/repositories/execution_charge_repository.py`
- Create: `backend/app/modules/platform/domain/repositories/quota_account_repository.py`
- Create: `backend/app/modules/platform/domain/repositories/usage_ledger_repository.py`
- Create: `backend/app/modules/platform/domain/repositories/artifact_repository.py`
- Create: `backend/app/modules/platform/domain/repositories/job_event_repository.py`
- Create: `backend/app/modules/platform/domain/repositories/admin_query_repositories.py`
- Create: `backend/app/modules/platform/infra/persistence/repositories/*.py`
- Modify: `backend/app/composition/container.py`
- Create: `backend/tests/platform/repositories/test_job_repository.py`
- Create: `backend/tests/platform/repositories/test_execution_and_quota_repositories.py`
- Create: `backend/tests/platform/repositories/test_job_query_repositories.py`

- [ ] **Step 1: 把 repository 草案落成代码接口**
  建立 protocol / ABC 风格的 repository 定义，与既定的读写拆分一致，并确保 `AIExecution` 不泄漏到用户侧 repository。

- [ ] **Step 2: 优先实现写 repository**
  先完成 `JobRepository`、`AIExecutionRepository`、`ExecutionChargeRepository`、`QuotaAccountRepository`、`UsageLedgerRepository`、`JobEventRepository`。

- [ ] **Step 3: 实现读模型 repository**
  增加用户侧 list/detail/items/events/quota 查询 repository，以及 admin 侧 executions、billing、event/audit 视图查询 repository。

- [ ] **Step 4: 在 container 中注册实现**
  用稳定 key 把具体持久化 adapter 接到容器后面，后续如需更换实现，不必改 service 层。

- [ ] **Step 5: 覆盖事务关键行为的 repository 测试**
  测试行锁辅助、幂等查询、quota bucket 选择、事件追加顺序，以及 user/admin 可见性隔离。

**Tests:**
- `backend/tests/platform/repositories/test_job_repository.py`
- `backend/tests/platform/repositories/test_execution_and_quota_repositories.py`
- `backend/tests/platform/repositories/test_job_query_repositories.py`

**Commands:**
- `pytest backend/tests/platform/repositories/test_job_repository.py -q`
- `pytest backend/tests/platform/repositories/test_execution_and_quota_repositories.py -q`
- `pytest backend/tests/platform/repositories/test_job_query_repositories.py -q`

**Expected:**
- Service 层可通过 repository 加载 / 更新聚合，不再需要在 router 里直接写 SQL。
- 查询路径不再复用写模型。

## Chunk 2: Services And Runner Extraction

### Task 4: 围绕已确认生命周期建立 application services

**Files:**
- Create: `backend/app/modules/platform/application/services/job_application_service.py`
- Create: `backend/app/modules/platform/application/services/job_query_service.py`
- Create: `backend/app/modules/platform/application/services/execution_orchestrator_service.py`
- Create: `backend/app/modules/platform/application/services/quota_billing_service.py`
- Create: `backend/app/modules/platform/application/services/event_service.py`
- Create: `backend/app/modules/platform/application/services/admin_query_service.py`
- Create: `backend/app/modules/platform/application/services/approval_service.py`
- Create: `backend/app/modules/platform/application/services/build_deploy_service.py`
- Modify: `backend/app/composition/container.py`
- Create: `backend/tests/platform/services/test_job_application_service.py`
- Create: `backend/tests/platform/services/test_execution_orchestrator_service.py`
- Create: `backend/tests/platform/services/test_job_query_service.py`
- Create: `backend/tests/platform/services/test_quota_billing_service.py`

- [ ] **Step 1: 实现 `JobApplicationService`**
  支持 create-draft、start、cancel 流程，并明确事务边界与事件发射。

- [ ] **Step 2: 实现 `QuotaBillingService`**
  封装 reserve / capture / refund 规则，让 router 和 runner 不再直接接触 ledger 逻辑；phase 1 只实现已经冻结的 v1 refund matrix。

- [ ] **Step 3: 实现 `ExecutionOrchestratorService`**
  负责检查取消 / quota、创建 execution、更新 job 聚合，并把实际执行委托给 runner 层。

- [ ] **Step 4: 实现查询服务**
  增加 `JobQueryService` 和 `AdminQueryService`，从读 repository 与事件可见性规则中组装 user/admin 响应。

- [ ] **Step 5: 增加 approval/build facade**
  为当前 approval/build 流程包一层 facade，使 `approval_router.py` 和 `build_deploy.py` 可以退化为纯传输层调用方。

- [ ] **Step 6: 在 container 中注册 services 并补 service 测试**
  覆盖 draft 到 running 的转换、quota 用尽、取消语义、refund 决策以及 admin/user 数据隔离。

**Tests:**
- `backend/tests/platform/services/test_job_application_service.py`
- `backend/tests/platform/services/test_execution_orchestrator_service.py`
- `backend/tests/platform/services/test_job_query_service.py`
- `backend/tests/platform/services/test_quota_billing_service.py`

**Commands:**
- `pytest backend/tests/platform/services/test_job_application_service.py -q`
- `pytest backend/tests/platform/services/test_execution_orchestrator_service.py -q`
- `pytest backend/tests/platform/services/test_job_query_service.py -q`
- `pytest backend/tests/platform/services/test_quota_billing_service.py -q`

**Expected:**
- Router 代码可以把生命周期规则委托给 service。
- Quota 与 refund 行为可以在 HTTP/WebSocket handler 之外复用和测试。

### Task 5: 从 legacy WebSocket 流程中抽出 workflow runner 抽象

**Files:**
- Create: `backend/app/modules/platform/runner/workflow_registry.py`
- Create: `backend/app/modules/platform/runner/workflow_runner.py`
- Create: `backend/app/modules/platform/runner/step_dispatcher.py`
- Create: `backend/app/modules/platform/runner/execution_adapter.py`
- Create: `backend/app/modules/platform/runner/build_deploy_adapter.py`
- Create: `backend/app/modules/platform/runner/approval_adapter.py`
- Modify: `backend/app/composition/container.py`
- Create: `backend/tests/platform/runner/test_workflow_registry.py`
- Create: `backend/tests/platform/runner/test_workflow_runner.py`
- Create: `backend/tests/platform/runner/test_execution_adapter.py`

- [ ] **Step 1: 定义 workflow registry 与 step-dispatch 边界**
  把 `job_type` / `item_type` 映射到共享 workflow 定义，并显式定义 step payload contract。

- [ ] **Step 2: 围绕当前可调用资产抽出 execution adapter**
  把现有 image/code/build/approval 执行链包在 runner interface 后面，使 application service 不再 import router helper。

- [ ] **Step 3: 实现汇报到统一事件模型的 runner**
  确保 polling 与未来 WebSocket 交付都消费同一套 stage/event 结构。

- [ ] **Step 4: 增加兼容 shim**
  在新 orchestrator 与 runner 通过开关验证前，保持 legacy 流程仍可调用。

- [ ] **Step 5: 测试 runner 行为**
  覆盖 workflow 选择、step dispatch、`failed_business` / `failed_system` 映射以及事件发射顺序。

**Tests:**
- `backend/tests/platform/runner/test_workflow_registry.py`
- `backend/tests/platform/runner/test_workflow_runner.py`
- `backend/tests/platform/runner/test_execution_adapter.py`

**Commands:**
- `pytest backend/tests/platform/runner/test_workflow_registry.py -q`
- `pytest backend/tests/platform/runner/test_workflow_runner.py -q`
- `pytest backend/tests/platform/runner/test_execution_adapter.py -q`

**Expected:**
- 平台执行流变成与传输无关的结构。
- 现有 workflow 资产可以被复用，而不是整体重写。

## Chunk 3: Router Migration And Rollout

### Task 6: 在 legacy 路由旁边增加 platform job API

**Files:**
- Create: `backend/routers/platform_jobs.py`
- Create: `backend/routers/platform_admin.py`
- Modify: `backend/main.py`
- Modify: `backend/app/composition/container.py`
- Create: `backend/tests/platform/routers/test_platform_jobs_http.py`
- Create: `backend/tests/platform/routers/test_platform_admin_http.py`

- [ ] **Step 1: 实现用户侧 HTTP handler**
  增加 `POST /api/platform/jobs`、`POST /api/platform/jobs/{jobId}/start`、`POST /api/platform/jobs/{jobId}/cancel`、`GET /api/platform/jobs`、`GET /api/platform/jobs/{jobId}`、`GET /api/platform/jobs/{jobId}/items`、`GET /api/platform/jobs/{jobId}/events`、`GET /api/platform/quota`。

- [ ] **Step 2: 实现管理员侧 HTTP handler**
  增加 execution、billing/refund、audit 端点，用于暴露 admin-only 读模型，但不把这些信息泄漏到用户 API 中；`admin` 范围是 phase-1 基线，不是后续 rollout 项。

- [ ] **Step 3: 从 container 中解析 service**
  保持 handler 精简：认证 / 上下文解析、DTO 转换、service 调用、DTO 响应。

- [ ] **Step 4: 通过 settings 控制 router 注册**
  只有在 rollout flag 开启时才挂载新的 platform router，但此时仍不移除 legacy routes。

- [ ] **Step 5: 增加 API 测试**
  覆盖 happy path、启动前取消、quota 用尽、资源缺失、admin 可见性检查。

**Tests:**
- `backend/tests/platform/routers/test_platform_jobs_http.py`
- `backend/tests/platform/routers/test_platform_admin_http.py`

**Commands:**
- `pytest backend/tests/platform/routers/test_platform_jobs_http.py -q`
- `pytest backend/tests/platform/routers/test_platform_admin_http.py -q`

**Expected:**
- 可以得到一套干净的 platform API，而无需立刻切掉现有 routes。
- 用户 / 管理员 contract 在传输边界被强制执行。

### Task 7: 通过委托 service 与 runner facade 让 legacy router 变薄

**Files:**
- Modify: `backend/routers/workflow.py`
- Modify: `backend/routers/batch_workflow.py`
- Modify: `backend/routers/build_deploy.py`
- Modify: `backend/routers/config_router.py`
- Modify: `backend/routers/approval_router.py`
- Modify: `backend/app/composition/container.py`
- Modify: `backend/app/shared/infra/config/settings.py`
- Create: `backend/tests/platform/routers/test_workflow_router_compat.py`
- Create: `backend/tests/platform/routers/test_batch_router_compat.py`
- Create: `backend/tests/platform/routers/test_build_deploy_router_compat.py`
- Create: `backend/tests/platform/routers/test_approval_and_config_router_compat.py`

- [ ] **Step 1: 替换 `workflow.py` 中内嵌的编排逻辑**
  把生命周期、事件整形和启动执行的决策迁移到 platform services / runner，同时在统一 contract flag 开启前，继续保持当前 WebSocket contract 不变。

- [ ] **Step 2: 替换 `batch_workflow.py` 中内嵌的编排逻辑**
  保留现有 batch UX，但把计划执行、审批、quota 检查和 item 状态推进迁入 services 与 runner adapters。

- [ ] **Step 3: 委托 build/deploy 与 approval 路由**
  让 `build_deploy.py` 和 `approval_router.py` 通过 service facade 工作，而不是直接触碰运行时内部实现。

- [ ] **Step 4: 把 config 路由变成 settings/query 端点**
  使用 `config_router.py` 暴露 rollout 状态与可变 settings，而不是继续协调业务逻辑。

- [ ] **Step 5: 增加兼容覆盖**
  验证 flag 关闭时 legacy routes 仍可工作；flag 打开时 split-path 委托行为正确。

**Tests:**
- `backend/tests/platform/routers/test_workflow_router_compat.py`
- `backend/tests/platform/routers/test_batch_router_compat.py`
- `backend/tests/platform/routers/test_build_deploy_router_compat.py`
- `backend/tests/platform/routers/test_approval_and_config_router_compat.py`

**Commands:**
- `pytest backend/tests/platform/routers/test_workflow_router_compat.py -q`
- `pytest backend/tests/platform/routers/test_batch_router_compat.py -q`
- `pytest backend/tests/platform/routers/test_build_deploy_router_compat.py -q`
- `pytest backend/tests/platform/routers/test_approval_and_config_router_compat.py -q`

**Expected:**
- Legacy 入口在迁移过程中仍可用。
- Router 文件向“仅负责传输”的职责收缩。

### Task 8: 落地迁移检查点、数据 backfill hook 与端到端验证

**Files:**
- Create: `backend/migrations/versions/20260327_02_platform_backfill_and_switches.py`
- Create: `backend/app/modules/platform/application/services/migration_rollout_service.py`
- Modify: `backend/app/composition/container.py`
- Modify: `backend/app/shared/infra/config/settings.py`
- Modify: `backend/main.py`
- Create: `backend/tests/platform/e2e/test_platform_job_lifecycle.py`
- Create: `backend/tests/platform/e2e/test_platform_rollout_flags.py`
- Create: `docs/superpowers/checklists/platform-service-split-rollout.md`

- [ ] **Step 1: 增加 rollout / backfill service hook**
  建立一个专门的 service，负责按正确顺序开启 flags、校验 schema 就绪情况、执行 decision gates，并为新查询补齐所需的兼容元数据 backfill。

- [ ] **Step 2: 增加第二个 migration 用于兼容 / backfill 支持**
  把在 service 实现过程中发现的视图 / 物化字段、nullable-to-required 转换或默认值 backfill，一并收口到这里。

- [ ] **Step 3: 编写端到端生命周期测试**
  覆盖 create -> start -> execute -> finish、取消、quota 用尽、refund 流程和 event polling。

- [ ] **Step 4: 编写 rollout flag 测试**
  验证系统在 legacy-only、`dual-write + read legacy` 和 split-read 模式下行为正确。

- [ ] **Step 5: 增加 operator checklist**
  记录启用顺序：先 schema，再 dual-write 接线，再 platform API，再 router delegation，再 split reads，最后统一 contract。

- [ ] **Step 6: 增加 rollback 测试矩阵**
  显式验证从 split-read 回退到 `read legacy`、从 router delegation 回退到 legacy handler，以及从 dual-write 回退到 legacy-only 读权限时，不发生数据丢失或 admin/user contract 漂移。

**Tests:**
- `backend/tests/platform/e2e/test_platform_job_lifecycle.py`
- `backend/tests/platform/e2e/test_platform_rollout_flags.py`

**Commands:**
- `pytest backend/tests/platform/e2e/test_platform_job_lifecycle.py -q`
- `pytest backend/tests/platform/e2e/test_platform_rollout_flags.py -q`
- `pytest backend/tests/platform -q`

**Expected:**
- 迁移可以分阶段、可回退地启用，而不是一次性切换。
- 工程与运维都有清晰的 rollout / rollback 检查清单。

## Rollback Test Matrix

| Scenario | Starting mode | Rollback target | Must verify |
| --- | --- | --- | --- |
| 读路径回滚 | dual-write + split-read | dual-write + `read legacy` | list/detail/items/events/quota/admin 响应与 legacy 基线一致 |
| Router 回滚 | delegated legacy routers | legacy routers without service delegation | legacy HTTP/WebSocket contract 仍成立；没有孤儿写入 |
| 全量迁移暂停 | dual-write + `read legacy` | legacy-only mode | 新写入可以被干净暂停；legacy 读取仍然正确 |
| Admin 面回滚 | split-read admin APIs | legacy-backed admin reads | execution/refund/audit 的可见性与鉴权边界保持不变 |
| 事件契约回滚 | unified event contract on | polling-only legacy-compatible contract | cursor 顺序、payload 兼容性与客户端 fallback 均成功 |

## Rollout Order

1. 先落 contracts 与 settings flags，且所有新开关默认关闭。
2. 再落 schema 与 repository 实现，此时 dual-write 接线仍默认关闭。
3. 落 application services 与 runner abstraction，然后启用 dual-write，同时所有读仍保持在 legacy（`read legacy`）。
4. 在 flag 后面增加新的 `/api/platform/*` 用户 / 管理员 API，此时仍从 legacy-backed 数据源读取，直到对齐被证明通过。
5. 在兼容模式下把 legacy routers 委托给 services / runner，同时继续保持 `read legacy` 为默认读权限。
6. 只有在对齐与回滚矩阵都通过后，才通过 flag 开启 split reads。
7. 只有在 split-read polling 与兼容测试通过后，才开启统一事件 / WebSocket contract。

## Open Decisions To Resolve During Execution

- `job_type`、`item_type`、`event_type` 的最终枚举冻结。
- `workflow_version`、`step_protocol_version`、`result_schema_version` 的精确持久化位置。
- phase-two 的 runner 隔离，在 split 稳定后，是继续留在进程内，还是拆成独立 worker 进程。

计划已完成，并归档到 `docs/90-归档/后端专题/已停用-2026-03-27-API与Service拆分迁移实施计划.md`。准备开始执行了吗？

