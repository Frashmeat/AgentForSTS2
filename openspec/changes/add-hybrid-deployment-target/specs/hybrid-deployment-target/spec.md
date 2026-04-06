## ADDED Requirements

### Requirement: Release Tooling MUST Expose a Dedicated Hybrid Deployment Target

The repository MUST expose a dedicated `hybrid` deployment target for the recommended production-shaped delivery model, and MUST NOT rely on `full` to express that model.

#### Scenario: Packaging hybrid release

- **WHEN** 用户执行 `package-release` 的 `hybrid` 目标
- **THEN** 产物必须包含独立前端静态资源、`workstation-backend` 与 launcher
- **AND** 产物不得包含 `web-backend`

#### Scenario: Full remains compatibility-only

- **WHEN** 用户查看脚本帮助或仓库文档
- **THEN** `full` 必须继续被描述为兼容态 / 联调态
- **AND** 不得再把 `full` 作为正式推荐部署目标

### Requirement: Hybrid Documentation MUST Describe the Dual-Side Deployment Boundary

Documentation for the recommended deployment model MUST describe the boundary between the user-side `hybrid` bundle and the separately deployed `web-backend`.

#### Scenario: User-side hybrid bundle

- **WHEN** 用户阅读 `hybrid` 相关文档
- **THEN** 文档必须明确用户侧 bundle 只包含前端、`workstation-backend` 和 launcher

#### Scenario: Server-side web deployment stays separate

- **WHEN** 用户阅读 `hybrid` 相关文档
- **THEN** 文档必须明确 `web-backend` 与数据库继续独立部署
