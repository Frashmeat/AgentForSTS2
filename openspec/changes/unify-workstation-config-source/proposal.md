## Why

当前仓库中与工作站运行相关的配置来源已经发生分叉：

- 仓库根目录存在 `config.json`
- release 产物中存在 `runtime/workstation.config.json`
- release 产物中还存在 `services/workstation/config.json`
- 历史 Docker / mixed deploy 脚本会在不同位置写入或沿用配置
- Codex CLI 与 Claude CLI 还各自存在用户全局配置

这导致“界面显示配置”“打包写入配置”“工作站实际读取配置”“CLI 自身全局配置”之间缺少单一所有权，用户和开发者都很难判断当前到底哪一份配置在生效。当前已经出现 release 内 `runtime/workstation.config.json` 与 `services/workstation/config.json` 不一致，并最终导致运行时 backend 选择偏离预期。

需要先把应用级配置真源收敛到一个固定位置，再明确哪些文件只是派生产物、哪些配置属于外部 CLI 私有范围，后续的实现和排障才能稳定进行。

## What Changes

- 规定 `runtime/workstation.config.json` 为工作站应用级配置的唯一真源。
- 调整工作站配置读取、打包和部署约定，禁止继续把 `services/workstation/config.json` 作为独立配置真源。
- 明确仓库根目录 `config.json`、历史 Docker 配置和 release 服务目录配置在迁移期的角色与退场路径。
- 明确 Codex CLI / Claude CLI 的用户全局配置不属于项目应用配置真源，只能作为外部环境或覆盖风险被检测与提示。

## Capabilities

### New Capabilities

- `workstation-config-source-of-truth`: 工作站必须以单一配置文件作为应用级配置真源，避免根目录、release runtime、service 目录和历史部署链路并存多个同级配置来源。
- `cli-global-config-boundary`: 项目必须明确区分“应用配置真源”和“Codex/Claude CLI 的用户全局配置”，避免把 CLI 私有配置误当作项目配置来源。

### Modified Capabilities

- `cli-runtime-config-alignment`: CLI 运行时配置对齐能力必须建立在单一应用配置真源之上，而不是继续接受多个项目内配置文件并存。

## Impact

- `backend/app/shared/infra/config/settings.py`
- `tools/latest/package-release.ps1`
- `tools/latest/deploy-docker.ps1`
- `tools/README.md`
- 根目录 `config.json` 与 release 内工作站配置文件的生成/读取规则
- 与 Claude CLI / Codex CLI 配置边界相关的设置文档与运行时摘要能力
