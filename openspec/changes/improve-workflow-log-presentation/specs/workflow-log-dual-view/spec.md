## ADDED Requirements

### Requirement: Workflow Logs MUST Support Both Raw Output and Optimized Output in One Shared Panel

The workstation MUST provide a shared log panel for workflow execution that lets users switch between raw output and optimized output without losing any original log content.

#### Scenario: Optimized view is the default

- **WHEN** 用户打开单资产、批量规划、Mod 修改、构建部署或日志分析的工作流日志面板
- **THEN** 面板必须默认显示“优化后输出”
- **AND** 用户必须可以切换到“原始输出”

#### Scenario: Raw view preserves original ordering

- **WHEN** 用户切换到“原始输出”
- **THEN** 面板必须按原始流式接收顺序展示日志
- **AND** 不得因优化视图规则而丢失原始文本

#### Scenario: Optimized view reduces noise without rewriting facts

- **WHEN** 面板显示“优化后输出”
- **THEN** 系统可以合并重复阶段提示、标记 stderr/失败/完成等关键条目
- **AND** 不得把原始事实改写成与原文含义不一致的内容

### Requirement: Workflow Log Presentation MUST Reuse a Shared Entry Contract Across Main Workflows

Main workflow surfaces MUST reuse one shared log entry contract instead of each feature inventing an incompatible log shape.

#### Scenario: Different workflow sources produce compatible log entries

- **WHEN** 日志来自工作流阶段、代码代理、构建部署或日志分析
- **THEN** 前端必须能使用同一组基础字段解析这些条目
- **AND** 不要求各来源输出完全相同的文案格式
