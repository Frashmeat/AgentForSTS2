# 2026-04-29 Web控制面与Workstation执行面拆分方案

> 文档定位：本文用于讨论和冻结 Web 后端与 Workstation 执行节点的职责边界、派发协议和迁移步骤，不是已确认实施稿。
>
> 事实依据：基于 2026-04-29 当前代码静态调查、`single_generate/relic` 真实失败日志，以及人工确认的“Web 应专注记录与调度，具体工作逻辑交给工作站”口径整理。
>
> 权威入口：上级入口为 `docs/03-方案/后端专题/README.md`；当前接口事实以 `docs/02-现状/当前前后端接口文档.md` 为准。
>
> 最后更新：2026-04-29

## 0. 当前状态

- 当前状态：未开始，待讨论确认。
- 当前现状：
  - 仓库已有 `workstation` 与 `web` 两类运行角色。
  - 脚本层可以手工启动 workstation，例如 `backend/main_workstation.py`、`tools/start/start_workstation.bat`、`tools/split-local/start_split_local.ps1`。
  - `web` 后端当前没有在平台任务执行时自动启动 workstation，也没有把平台任务派发给 workstation。
  - `single_generate/relic` 当前由 Web 进程内的平台注册 workflow 执行：`single.asset.plan` 会在 Web 侧拼接 `platform_single_asset_server_user` prompt，再通过 LiteLLM 调用上游模型。
- 当前问题：
  - Web 后端承担了业务 prompt 拼装、STS2 知识拼接、模型调用等执行面职责。
  - 这与“Web 只负责谁在用、任务状态、额度、审计和调度”的口径不一致。
  - Web 侧完整 prompt 与工作台直接测试 key 的请求内容不同，导致排障容易混淆。
- 已确认口径：
  - Workstation 是 Web 同机托管执行节点，不是用户本机工作台。
  - Web 默认管理并启动一个 Workstation。
  - 凭据由 Web 下发执行期凭据。
  - Web 派发任务后第一版采用异步轮询接收结果，协议预留回调字段。
  - 当前项目尚未上线，不保留 Web runner 兼容路径，不提供 fallback。
  - 第一阶段文本任务与代码任务都迁移到 Workstation。
  - 第一阶段文本任务与代码任务都允许并发，默认并发数为 2。
  - Web 退出时自动关闭其托管启动的 Workstation。
  - 第一版异步结果回收先做轮询，协议预留回调。
  - 第一阶段必须冻结 Workstation 结果协议，尤其是 artifact 回传契约。
  - Workstation 与 Web 固定同机部署，第一阶段采用 loopback 监听 + 托管控制 token，不使用 mTLS。
  - Workstation 托管启动失败时，第一版先做必要诊断接口，不先做完整管理端页面。
  - 非同机执行节点身份策略后续再讨论，当前搁置。

## 1. 当前理解

用户确认的目标边界是：

- Web 后端专注控制面。
- Workstation 或执行节点负责具体工作逻辑。
- 业务 prompt、知识库检索、工作区读写、模型调用等执行细节不应继续堆到 Web 后端。

因此，本方案建议把服务器模式从“Web 进程内执行”逐步收口为“Web 派发，Workstation 执行”。

同机托管含义：

```text
同一台服务器
  - web backend: 127.0.0.1:7870
  - workstation backend: 127.0.0.1:7860
  - runtime/server-workspaces/*
  - runtime/uploads/*
  - runtime/config/*
```

Web 对外暴露平台接口；Workstation 默认只监听本机地址，作为 Web 托管的执行节点使用。

## 2. 目标

第一阶段目标：

1. 冻结 Web 控制面与 Workstation 执行面的职责边界。
2. 定义 Web 到 Workstation 的最小任务派发协议。
3. 让文本类与代码类平台任务都具备 Workstation 执行路径。
4. Web 不再新增业务 prompt 拼装能力。
5. Web 平台任务不再允许在 Web 进程内执行业务生成逻辑。

## 3. 非目标

第一阶段不做：

- 一次性迁移所有平台 runner。
- 复杂多执行节点调度、负载均衡和跨机器发现。
- 面向公网的不可信执行节点协议。
- 完整工作站自动安装、自动升级和远程运维。
- 大规模凭据下发体系。

## 4. 职责边界

### 4.1 Web 控制面职责

Web 负责：

- 用户认证与权限。
- 任务创建、排队、状态流转。
- 额度预留、扣减、返还。
- 执行配置和服务器凭据管理。
- 执行记录、事件、审计和结果落库。
- 派发任务到 Workstation。
- 接收 Workstation 的结构化结果。

Web 不负责：

- 拼接具体业务 prompt。
- 维护 STS2 业务知识注入细节。
- 直接调用模型完成业务生成。
- 直接处理工作区内的业务实现细节。

### 4.2 Workstation 执行面职责

Workstation 负责：

- 接收标准任务包。
- 根据 `job_type / item_type` 选择本地执行器。
- 拼接业务 prompt。
- 查询 STS2 guidance、code facts、lookup。
- 访问本地或服务器工作区。
- 调用模型或本地 agent。
- 产出结构化结果、日志摘要和错误载荷。

## 5. 建议执行链路

```text
用户提交任务
  -> Web 创建 job / job_item
  -> Web 预留额度并创建 ai_execution
  -> Web 选择或连接 Workstation
  -> Web 异步派发标准任务包
  -> Workstation 执行业务逻辑
  -> Web 通过轮询取得结构化结果
  -> Web 更新执行记录、任务状态、额度和事件
```

## 6. 最小协议草案

### 6.1 Web 派发请求

建议新增 Workstation 内部接口：

```text
POST /api/workstation/platform/executions
```

请求体草案：

```json
{
  "execution_id": 2203,
  "job_id": 2002,
  "job_item_id": 2103,
  "job_type": "single_generate",
  "item_type": "relic",
  "workflow_version": "2026.03.31",
  "step_protocol_version": "v1",
  "result_schema_version": "v1",
  "input_payload": {
    "item_name": "RelicName",
    "description": "用户原始描述",
    "server_project_ref": "server-workspace:xxx"
  },
  "execution_binding": {
    "agent_backend": "codex",
    "provider": "openai",
    "model": "gpt-5.4",
    "credential_ref": "server-credential:1",
    "auth_type": "api_key",
    "credential": "sk-...",
    "base_url": "https://api.openai.com/v1"
  },
  "callback": {
    "enabled": false,
    "url": "",
    "token_ref": ""
  }
}
```

派发响应草案：

```json
{
  "workstation_execution_id": "ws-exec-2203",
  "status": "accepted",
  "poll_url": "/api/workstation/platform/executions/ws-exec-2203"
}
```

### 6.2 Web 获取结果

第一版建议支持轮询接口：

```text
GET /api/workstation/platform/executions/{workstation_execution_id}
```

协议预留回调字段，但第一版不启用。后续可补回调：

```text
POST /api/internal/platform/executions/{execution_id}/result
```

轮询结果草案：

```json
{
  "status": "succeeded",
  "step_id": "single.relic.plan",
  "output_payload": {
    "text": "摘要内容",
    "analysis": "完整执行结果"
  },
  "error_summary": "",
  "error_payload": {}
}
```

失败结果草案：

```json
{
  "status": "failed_system",
  "step_id": "single.relic.plan",
  "output_payload": {},
  "error_summary": "上游或执行节点拒绝了这次请求",
  "error_payload": {
    "reason_code": "workstation_execution_failed",
    "upstream_reason_code": "upstream_request_blocked",
    "source": "workstation"
  }
}
```

### 6.3 异步状态

Workstation 执行状态建议先收口为：

| 状态 | 含义 |
| --- | --- |
| `accepted` | 已接收，尚未开始或正在排队 |
| `running` | 正在执行 |
| `succeeded` | 执行成功 |
| `failed_system` | 系统失败 |
| `failed_business` | 业务失败 |
| `cancelled` | 已取消 |

Web 侧以 `execution_id` 为真源，Workstation 侧的 `workstation_execution_id` 只作为执行节点内的任务句柄。

### 6.4 Artifact 回传契约

第一阶段必须冻结代码类任务的 artifact 回传契约，避免 Web 无法稳定落库、展示和追溯结果。

Workstation 结果统一返回：

```json
{
  "status": "succeeded",
  "step_id": "build.project",
  "output_payload": {
    "text": "构建成功",
    "summary": "构建成功",
    "analysis": "",
    "artifacts": [
      {
        "artifact_type": "build_output",
        "storage_provider": "server_workspace",
        "object_key": "server-workspace:xxx/build/xxx.pck",
        "file_name": "xxx.pck",
        "mime_type": "application/octet-stream",
        "size_bytes": 12345,
        "result_summary": "构建产物"
      }
    ]
  },
  "error_summary": "",
  "error_payload": {}
}
```

冻结规则：

- `status` 沿用现有 `StepExecutionResult` 语义：`succeeded / failed_system / failed_business / cancelled`。
- `output_payload.artifacts` 继续复用 Web 侧现有 artifact 落库字段。
- `storage_provider` 第一阶段只允许：
  - `server_workspace`
  - `server_deploy`
  - `uploaded_asset`
- `object_key` 必须是 Web 能解析或定位的服务器侧引用，不能是执行节点私有临时路径。
- Workstation 可以在内部使用本机实际路径，但长期 artifact key 不使用裸文件系统路径。
- 失败时返回 `reason_code`，并带 `source = "workstation"`。
- 不返回凭据、不返回完整 prompt。

## 7. 配置草案

建议新增运行时配置：

```json
{
  "platform_execution": {
    "workstation_url": "http://127.0.0.1:7860",
    "auto_start": true,
    "control_token_env": "ATS_WORKSTATION_CONTROL_TOKEN",
    "dispatch_timeout_seconds": 10,
    "poll_interval_seconds": 2,
    "execution_timeout_seconds": 180,
    "max_concurrent_text": 2,
    "max_concurrent_code": 2,
    "max_concurrent_workspace_writes_per_ref": 1,
    "max_concurrent_deploy_per_target": 1
  }
}
```

当前项目尚未上线，不保留兼容运行模式：

- 不提供 `web_runner`。
- 不提供 `fallback_to_web_runner`。
- 不提供 `disabled` 执行模式。
- Web 平台任务唯一执行路径是 Web 托管的同机 Workstation。

### 7.1 内部准入策略

第一阶段采用 loopback 信任加托管控制 token：

- Workstation 只监听 `127.0.0.1`，不监听 `0.0.0.0`。
- Web 启动时生成或读取一个 `workstation_control_token`。
- Web 托管启动 Workstation 时，通过环境变量 `ATS_WORKSTATION_CONTROL_TOKEN` 注入 token。
- Web 调 Workstation 内部接口时带请求头：

```http
X-ATS-Workstation-Token: <token>
```

- Workstation 对内部执行接口和健康检查接口校验该 token，不通过返回 401。
- token 不写日志、不返回前端、不进入任务 payload。
- 如果 Web 发现 `workstation_url` 已有可用进程，但当前 token 无法通过健康检查，则认为端口上的 Workstation 不属于当前 Web，报错且不复用。

第一阶段不使用 mTLS。原因是 Workstation 与 Web 固定同机，跨网身份认证收益不足以抵消证书管理成本。后续如果支持非同机执行节点，再引入 mTLS 或更强节点身份机制。

### 7.2 托管诊断接口

第一版需要提供必要诊断接口，供管理端或排障脚本判断托管 Workstation 状态。先不要求完整管理端页面。

建议 Web 暴露：

```text
GET /api/admin/platform/workstation-runtime-status
```

返回字段草案：

```json
{
  "enabled": true,
  "workstation_url": "http://127.0.0.1:7860",
  "managed_by_current_web": true,
  "process_started": true,
  "pid": 12345,
  "healthy": true,
  "token_accepted": true,
  "last_health_check_at": "2026-04-29T12:00:00+08:00",
  "last_error_code": "",
  "last_error_message": ""
}
```

诊断口只返回状态和错误摘要，不返回 token、凭据、完整 prompt 或执行期 payload。

## 7.3 托管启动策略

Web 启动后执行：

```text
检查 workstation_url 健康状态
  -> 已可用：复用现有 Workstation
  -> 不可用且 auto_start=true：启动 main_workstation.py
  -> 等待健康检查通过
  -> 开始派发平台任务
```

约束：

- 如果端口已被 AgentTheSpire Workstation 占用，直接复用。
- 如果端口被其他进程占用，报明确错误，不盲目杀进程。
- Workstation 日志单独落文件。
- 若 Workstation 由当前 Web 进程托管启动，Web 退出时自动关闭该 Workstation。
- 若 Workstation 是 Web 启动前已存在的可用进程，Web 默认只复用，不负责关闭。

## 7.4 第一版并发策略

第一版采用单 Workstation 实例：

| 任务类型 | 并发策略 |
| --- | --- |
| 文本规划与日志分析 | 允许并发，第一版默认 2 |
| 代码、资产、构建类任务 | 全局并发默认 2 |
| 同一 `server_project_ref` | 串行 |
| 同一 project / deploy target | 串行 |

Web 负责队列、锁和并发控制；Workstation 只执行 Web 派发的任务，不自行抢任务。

第一阶段不做多 Workstation 进程池。若后续需要扩展，再引入多个托管实例、worker registry、实例健康和调度策略。

## 8. 迁移范围

### 8.1 第一阶段迁移

第一阶段迁移文本类与代码类平台执行：

- `single_generate/relic`
- `single_generate/card`
- `single_generate/power`
- `single_generate/character`
- `batch_generate/relic`
- `batch_generate/card`
- `log_analysis`
- `batch.custom_code.plan`
- `code.generate`
- `asset.generate`
- `build.project`

原因：

- 当前问题正发生在 `single_generate/relic` 的文本规划阶段。
- 但目标边界是 Web 不执行任何平台业务 runner，因此代码、资产和构建类任务也必须迁移。
- 代码类任务迁移后，Web 才能真正只保留控制面职责。

### 8.2 暂缓迁移

暂缓：

- 多 Workstation 进程池。
- 跨机器执行节点注册与调度。
- 回调式结果回收的实际启用。

原因：

- 当前先用单 Workstation + Web 侧并发控制跑通主线。
- 回调需要鉴权、防重放、重试和幂等补偿，第一版只预留协议字段。

## 9. 凭据策略

已确认第一阶段由 Web 下发执行期凭据。

执行规则：

- Web 从服务器凭据池选择 route。
- Web 解密 credential。
- Web 通过同机 HTTP 请求发给 Workstation。
- Workstation 只在内存中使用，不落盘。
- Workstation 不在日志、错误载荷或结果中回显凭据。

允许记录：

- `credential_ref`
- `provider`
- `model`
- `base_url_configured`

禁止记录：

- `credential` 原文
- 完整 prompt
- 可能包含凭据的异常原始请求体

该策略仅适用于 Web 与 Workstation 同机受信部署。

## 10. Workstation 启动策略

已确认第一阶段 Web 默认启动一个同机 Workstation。

仍需细化：

- Workstation 健康检查接口。
- 子进程 stdout/stderr 日志落点。
- Web 重启时如何识别旧 Workstation 是否可复用。
- Web 退出时自动关闭托管 Workstation 的具体清理流程。
- 端口冲突时的错误码和管理端提示。

## 11. 风险

1. Workstation 在用户本机时，Web 服务器不能默认访问用户本机 `127.0.0.1`。
2. 如果 Web 与 Workstation 不同机，工作区路径、上传文件路径和构建产物路径不能直接共享。
3. 任务重复派发需要幂等保护。
4. 异步轮询需要处理超时、丢失、重复结果和执行节点重启。
5. 文本规划并发会增加同一凭据的上游并发压力，需要配合限流和错误分类。
6. 凭据明文下发必须限制在受信部署，并避免日志泄露。

## 12. 验收标准

第一阶段完成后应满足：

1. 文档明确 Web 控制面与 Workstation 执行面边界。
2. Web 不再为已迁移的文本规划任务拼接业务 prompt。
3. Workstation 能接收标准任务包，并通过轮询返回结构化结果。
4. `single_generate/relic` 可通过 Workstation 执行。
5. Web 能正确记录执行成功、失败、额度返还和事件。
6. Workstation 不在线时错误可解释，且不会误报为内容安全。
7. `docs/02-现状/当前前后端接口文档.md` 同步更新实际接口与错误码。

## 13. 建议讨论议题

已确认：

1. Workstation 是 Web 同机托管执行节点。
2. Web 默认启动一个 Workstation。
3. 凭据第一阶段由 Web 下发执行期凭据。
4. Workstation 结果回传第一版采用派发后异步轮询，协议预留回调。
5. 不保留 Web runner 兼容和 fallback。
6. 文本任务与代码任务第一阶段都迁移到 Workstation。
7. 文本任务与代码任务第一阶段都允许并发，默认并发数为 2。
8. Web 退出时自动关闭其托管启动的 Workstation。
9. 第一阶段冻结 Workstation 结果协议，artifact 回传复用 Web 侧现有落库字段。
10. Workstation 与 Web 固定同机，第一阶段使用 loopback 监听 + 托管控制 token，不使用 mTLS。
11. Workstation 托管启动失败时，第一版先做必要诊断接口，不先做完整管理端页面。
12. 非同机执行节点身份策略后续再讨论，当前搁置。

仍需讨论：

- 暂无。确认后可进入实施计划细化。

## 14. 当前建议

建议先冻结以下口径：

1. Web 是控制面，不再新增业务 prompt 拼装。
2. Workstation 是执行面，负责具体生成逻辑。
3. 第一阶段采用 Web 托管单 Workstation，并异步派发任务。
4. 文本任务与代码任务都迁移到 Workstation，默认并发数均为 2。
5. 默认不 fallback 到 Web runner，且不保留 Web runner 配置模式。
6. 第一版只实现轮询结果回收，协议预留回调。
7. 第一阶段冻结 artifact 回传契约，复用现有 Web artifact 落库字段。
8. 内部执行接口采用 `127.0.0.1` 监听和 `X-ATS-Workstation-Token` 准入。
9. 第一版提供 Workstation 托管状态诊断接口，管理端完整页面后置。

## 15. 实施计划拆分

### Task 1：冻结配置与内部协议 Contract

目标：

- 新增 `platform_execution` 配置结构。
- 冻结 Web 到 Workstation 的派发请求、轮询结果、错误载荷和 artifact 回传 contract。
- 冻结 `X-ATS-Workstation-Token` 内部准入规则。

建议改动：

- 新增或扩展平台执行配置 contract。
- 新增 Workstation 执行请求 / 状态 / 结果 contract。
- 补 contract 单元测试，覆盖合法请求、缺 token、非法状态、artifact 字段。

验收：

- Contract 可以表达文本、代码、资产、构建任务。
- Artifact 字段与 Web 现有落库字段一致。
- 不包含凭据日志字段或完整 prompt 字段。

### Task 2：Workstation 内部执行入口

目标：

- 在 `workstation` 角色新增内部执行入口。
- 支持接收任务、生成 `workstation_execution_id`、保存执行状态、提供轮询结果。
- 内部接口必须校验 `X-ATS-Workstation-Token`。

建议接口：

```text
POST /api/workstation/platform/executions
GET /api/workstation/platform/executions/{workstation_execution_id}
GET /api/workstation/internal/health
```

验收：

- 无 token 返回 401。
- token 错误返回 401。
- 合法派发返回 `accepted`。
- 轮询可返回 `accepted / running / succeeded / failed_system / failed_business / cancelled`。

### Task 3：Web 托管 Workstation 生命周期

目标：

- Web 启动时检查 Workstation。
- 不存在时自动启动一个同机 Workstation。
- Web 退出时关闭当前 Web 托管启动的 Workstation。
- 若端口已有 Workstation，则用 token 健康检查决定是否复用。

建议改动：

- 新增托管进程服务，例如 `ManagedWorkstationService`。
- 子进程 stdout/stderr 单独落日志。
- 启动时注入 `ATS_WORKSTATION_CONTROL_TOKEN`。
- 增加端口占用、健康检查失败、token 不匹配错误分类。

验收：

- Web 可自动启动 Workstation。
- Web 可复用 token 匹配的已有 Workstation。
- Web 不复用 token 不匹配的进程。
- Web 正常退出时关闭自己托管的 Workstation。

### Task 4：Web 侧 Workstation 派发客户端与轮询器

目标：

- Web 不再直接调用平台业务 runner。
- Web 将任务派发给 Workstation。
- Web 轮询 Workstation 结果并转换为现有 `StepExecutionResult`。

建议改动：

- 新增 Workstation execution client。
- 新增轮询、超时、取消和错误映射逻辑。
- Web 侧保留队列、额度、执行记录、事件和 artifact 落库职责。

验收：

- 派发失败可写入明确错误码。
- 轮询超时可退款并记录失败。
- Workstation 返回 artifact 时，Web 正确落库。
- Workstation 上游错误不会被误报为“内容安全策略”。

### Task 5：迁移文本类任务到 Workstation

目标：

- `single.asset.plan`、`batch.custom_code.plan`、`log.analyze` 在 Workstation 内执行。
- Web 不再拼接 `platform_single_asset_server_user`。

范围：

- `single_generate/relic`
- `single_generate/card`
- `single_generate/power`
- `single_generate/character`
- `batch_generate/relic`
- `batch_generate/card`
- `log_analysis`
- `batch.custom_code.plan`

验收：

- 文本任务由 Workstation 拼 prompt 和调用模型。
- Web 只记录结果。
- 文本任务并发默认 2。

### Task 6：迁移代码、资产与构建类任务到 Workstation

目标：

- `code.generate`、`asset.generate`、`build.project` 在 Workstation 内执行。
- Web 保留 workspace lock、deploy target lock、artifact 落库和执行状态真源。

并发规则：

- 代码类全局并发默认 2。
- 同一 `server_project_ref` 串行。
- 同一 project / deploy target 串行。

验收：

- 不同 workspace 可并发执行。
- 同一 workspace 不会并发写入。
- 构建产物通过 artifact contract 回传并落库。

### Task 7：托管状态诊断接口

目标：

- Web 提供必要诊断口，不先做完整管理端页面。

建议接口：

```text
GET /api/admin/platform/workstation-runtime-status
```

验收：

- 返回托管状态、pid、健康状态、token 是否通过、最近健康检查时间和错误摘要。
- 不返回 token、凭据、完整 prompt 或执行期 payload。

### Task 8：文档同步与定向验证

目标：

- 同步接口文档和后端专题索引。
- 补充定向测试建议。

必须更新：

- `docs/02-现状/当前前后端接口文档.md`
- 本文或后续实施稿
- 如沉淀新约定，回写 `.trellis/spec/backend/`

建议验证：

```powershell
pytest backend/tests/platform/runner -q
pytest backend/tests/platform/services -q
pytest backend/tests/platform/routers/test_platform_admin_http.py -q
```

如涉及进程托管和启动脚本，再补专门的托管 Workstation 测试。

## 16. 建议实施顺序

1. Task 1：Contract。
2. Task 2：Workstation 内部执行入口。
3. Task 3：Web 托管 Workstation 生命周期。
4. Task 4：Web 派发客户端与轮询器。
5. Task 5：迁移文本类任务。
6. Task 6：迁移代码、资产与构建类任务。
7. Task 7：托管状态诊断接口。
8. Task 8：文档同步与定向验证。

每完成一个 Task 后先做定向验证，不默认跑全量 build 或全量测试。
