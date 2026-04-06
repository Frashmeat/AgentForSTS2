## Why

当前仓库已经明确推荐“独立前端 + workstation-backend + web-backend”作为正式部署形态，但发布脚本和文档仍只提供 `full / workstation / frontend / web` 四种目标。其中 `full` 继续承担兼容态 / 联调态语义，如果直接把正式推荐形态也称为 `full`，会导致运行时角色、发布目标和文档口径继续混淆。

需要新增一个明确的正式部署目标，把“用户侧安装包”和“服务器侧平台 API”这两个职责拆开表达：

- 用户侧拿到的是 `frontend + workstation + launcher`
- 服务器侧单独部署 `web-backend`

这样前端才能在同一个入口下同时使用工作站功能与 Web 功能，而不会继续误导为长期依赖 `full` 兼容态。

## What Changes

- 新增正式部署目标 `hybrid`，用于表达“独立前端 + workstation-backend + launcher”的用户侧交付形态。
- 保持 `full` 现有语义不变，继续仅表示兼容态 / 联调态 / 一体化验证包。
- 调整打包与部署脚本，使 `hybrid` 成为正式推荐目标之一。
- 更新仓库文档、决策文档和验收文档，明确 `full` 与 `hybrid` 的区别。

## Capabilities

### New Capabilities

- `hybrid-deployment-target`: 仓库必须提供一个独立于 `full` 的正式部署目标，用于同时支持工作站能力和 Web 平台能力。

### Modified Capabilities

- `independent-frontend-deployment`: 明确独立前端正式交付时优先使用 `hybrid` 目标，而不是继续复用 `full` 兼容态。

## Impact

- `tools/latest/package-release.ps1`
- `tools/latest/deploy-docker.ps1`
- `README.md`
- `tools/README.md`
- 后端部署决策文档与验收文档
