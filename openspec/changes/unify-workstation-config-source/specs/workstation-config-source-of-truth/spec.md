## ADDED Requirements

### Requirement: Workstation MUST Use a Single Application Configuration Source of Truth

The workstation runtime MUST treat `runtime/workstation.config.json` as the only application-managed source of truth for workstation configuration.

#### Scenario: Runtime loads workstation config from runtime directory

- **WHEN** 工作站后端启动并加载应用配置
- **THEN** 它必须从 `runtime/workstation.config.json` 读取工作站应用配置
- **AND** 不得再把仓库根目录 `config.json` 或 `services/workstation/config.json` 视为同级运行时真源

#### Scenario: Release bundle does not contain two independent workstation config truths

- **WHEN** 打包或部署生成一个 `workstation` / `hybrid` / `full` 相关 release
- **THEN** release 中必须只有一个工作站应用配置真源
- **AND** 任何兼容文件都必须明确来自该真源派生

### Requirement: Project-Level Configuration Roles MUST Be Explicitly Separated

The project MUST explicitly separate runtime source-of-truth config, development input config, and compatibility artifacts so users can tell which file actually controls workstation runtime.

#### Scenario: Root config is treated as development input only

- **WHEN** 用户或脚本读取仓库根目录 `config.json`
- **THEN** 文档和实现必须将其说明为开发输入、迁移来源或模板相关入口
- **AND** 不得再把它描述为当前 release 运行实例的唯一真源

#### Scenario: Service-directory config is compatibility-only

- **WHEN** `services/workstation/config.json` 在迁移期仍然存在
- **THEN** 它必须被视为兼容占位文件或待废弃文件
- **AND** 不得继续拥有独立配置所有权
