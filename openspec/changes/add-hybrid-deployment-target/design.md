## Context

当前项目有两种容易混淆的“full”：

- 运行时层面的 `create_app("full")`，表示兼容态聚合入口
- 发布层面的 `package/deploy full`，表示 workstation + web + postgres 的联调 bundle

但产品当前推荐给用户的正式形态并不是这两者，而是：

- 用户侧运行独立前端 + 本地 workstation
- 服务器侧单独运行 web-backend

这次设计的目标不是重定义 `full`，而是新增一个准确表达正式交付语义的目标 `hybrid`。

## Goals / Non-Goals

**Goals**

- 新增 `hybrid` 发布目标，表示用户侧“独立前端 + workstation-backend + launcher”
- 保持 `full` 作为兼容态 / 联调态，不破坏现有入口
- 让文档和脚本统一以 `hybrid` 表达正式推荐部署形态
- 维持现有 `runtime-config.js` 双后端分流方案不变

**Non-Goals**

- 不删除或重命名 `full`
- 不重构 `create_app("full")` 的运行时角色
- 不把 `web-backend` 打进用户侧 `hybrid` 安装包
- 不改变前端 runtime endpoint 的键名和解析方式

## Decisions

### 1. `hybrid` 是发布目标，不是新的后端运行时角色

`hybrid` 只出现在打包、部署和文档口径里，不新增 `backend/main_hybrid.py` 一类入口。

理由：

- 现有运行时角色边界已经清晰：`workstation` / `web` / `full`
- `hybrid` 的本质是交付编排，而不是新增后端职责域
- 避免把“部署拓扑”误做成“运行时角色”

### 2. `hybrid` 的内容固定为 `frontend + workstation + launcher`

`hybrid` 目标的 release bundle 固定包含：

- 独立前端静态资源
- `workstation-backend`
- `mod_template`
- launcher 脚本

不包含：

- `web-backend`
- postgres

理由：

- 这是用户侧实际需要拿到的交付物
- 平台 API 与数据库应继续独立部署
- 避免用户包被误解为“一包带整个平台”

### 3. `deploy hybrid` 默认只部署用户侧 bundle

`hybrid` 部署脚本默认只关注用户侧 release 的运行：

- 可选启动本地前端托管
- 可选启动或容器化运行 `workstation-backend`

不负责：

- 同时拉起 `web-backend`
- 同时拉起 postgres

如果需要平台能力，文档直接要求单独部署 `web` 目标。

### 4. 文档口径统一为“正式推荐 = hybrid，兼容联调 = full”

所有用户可见文档统一为：

- `full`: 历史兼容态 / 联调态
- `hybrid`: 正式推荐部署形态
- `workstation`: 只启动本地工作站后端
- `web`: 只启动平台 API
- `frontend`: 只发独立静态站点

## Risks / Trade-offs

- [目标数量增加] 脚本会多一个目标，需要同步文档和帮助信息
- [用户把 `hybrid` 与 `split-local` 混淆] 文档要明确：`split-local` 是本地启动方式，`hybrid` 是发布目标
- [用户期待 `hybrid` 自动带 web] 文档必须明确 `web-backend` 仍需独立部署
- [旧文档仍把 `full` 当正式形态] 需要集中更新 README、tools README 和决策文档

## Validation Strategy

- 打包脚本支持 `hybrid`，且产物目录不包含 `services/web`
- 帮助输出与 README 明确列出 `hybrid`
- OpenSpec 文档同步到新口径
