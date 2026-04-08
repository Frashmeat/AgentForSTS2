## ADDED Requirements

### Requirement: CLI Global Configuration MUST NOT Be Treated as Project Configuration Truth

Codex CLI and Claude CLI user-global configuration files MUST be treated as external environment inputs rather than project-managed configuration truth.

#### Scenario: Codex global config is outside project truth boundary

- **WHEN** 系统检测到用户主目录中的 Codex 全局配置
- **THEN** 项目不得把该文件当作工作站应用配置真源
- **AND** 只能把它归类为外部环境输入、兼容覆盖来源或风险来源

#### Scenario: Claude global config is outside project truth boundary

- **WHEN** 系统检测到 Claude CLI 的用户级设置
- **THEN** 项目不得把该设置当作工作站应用配置真源
- **AND** 只能把它归类为外部环境输入、兼容覆盖来源或风险来源

### Requirement: Runtime Diagnostics MUST Explain the Boundary Between Project Config and CLI Global Config

Runtime-facing diagnostics and settings summaries MUST explain which values come from the project source of truth and which values are only detected from external CLI configuration.

#### Scenario: Settings summary distinguishes project truth from external sources

- **WHEN** 设置接口或诊断信息展示当前工作站配置摘要
- **THEN** 必须能区分“来自 `runtime/workstation.config.json` 的项目配置”与“来自 CLI 全局配置的外部来源”
- **AND** 不得把两者混成同一层级的项目配置列表

#### Scenario: External CLI config may be reported without becoming authoritative

- **WHEN** 检测到外部 CLI 配置与项目真源存在差异
- **THEN** 系统可以展示提示、风险或隔离状态
- **AND** 不得因此重新赋予外部 CLI 配置项目真源地位
