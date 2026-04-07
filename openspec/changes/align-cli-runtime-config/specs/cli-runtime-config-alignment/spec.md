## ADDED Requirements

### Requirement: Workstation MUST Treat Application Settings as the Primary Source for CLI Runtime Configuration

When the workstation invokes Claude CLI or Codex CLI, the effective runtime configuration for code-agent execution MUST be derived from application-managed settings rather than being silently overridden by user-global CLI configuration files.

#### Scenario: Codex runtime ignores conflicting user-global provider settings

- **GIVEN** 应用设置中已保存代码代理使用的 `base_url`、认证信息和模型设置
- **AND** 用户主目录下存在与之冲突的 Codex 全局配置
- **WHEN** 工作站发起一次 Codex 代码代理任务
- **THEN** 实际运行必须以应用设置为准
- **AND** 不得因为 `~/.codex/config.toml` 中的 provider 或 base URL 配置而静默切换到其他服务

#### Scenario: Claude runtime receives the application model setting

- **GIVEN** 应用设置中将代码代理后端设为 Claude
- **AND** 应用设置中填写了 `llm.model`
- **WHEN** 工作站发起一次 Claude 代码代理任务
- **THEN** Claude CLI 必须按应用设置的模型运行
- **AND** 不得仅因用户全局 Claude 设置存在默认模型而忽略应用值

### Requirement: Workstation MUST Detect and Summarize External CLI Override Risks

The workstation MUST detect relevant external CLI configuration sources and expose a summary of whether they are isolated, compatible, or still risky for the current runtime.

#### Scenario: Runtime summary reports conflicting external config

- **WHEN** 当前机器存在与应用设置不一致的 Claude 或 Codex 用户级配置
- **THEN** 运行时摘要必须能标识该来源
- **AND** 必须说明该来源当前是“已隔离”“已兼容覆盖”或“仍有风险”

#### Scenario: Runtime summary remains available without exposing secrets

- **WHEN** 设置接口返回运行时生效摘要
- **THEN** 摘要中可以包含配置来源、模型名、base URL 和风险提示
- **AND** 不得直接暴露明文密钥或敏感 token
