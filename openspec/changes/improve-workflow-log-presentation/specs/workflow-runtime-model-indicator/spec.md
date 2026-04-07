## ADDED Requirements

### Requirement: Workflow Log Panel MUST Show the Current Effective Model Name

The workflow log panel MUST display the current effective model name used by code-agent execution so users can tell which model is actually running.

#### Scenario: Agent execution updates the displayed model name

- **WHEN** 工作流进入 Claude 或 Codex 代码代理执行阶段
- **THEN** 日志面板必须显示当前实际调用的模型名
- **AND** 模型名不得仅依赖前端保存配置的静态值推断

#### Scenario: Non-agent stage does not erase model context unexpectedly

- **WHEN** 工作流进入暂未调用模型的阶段
- **THEN** 系统必须采用稳定规则展示模型状态
- **AND** 该规则应表现为“继续显示最近一次有效模型”或“明确提示当前阶段未调用模型”中的一种

### Requirement: Model Display MUST Be Shared Across Main Workflow Surfaces

Model display behavior MUST remain consistent across the main workflow surfaces that reuse the shared log panel.

#### Scenario: Different workflow pages show the same model semantics

- **WHEN** 用户在单资产、批量规划、Mod 修改、构建部署或日志分析之间切换
- **THEN** 共享日志面板中的模型显示语义必须保持一致
- **AND** 不得出现某个页面显示“设置模型”、另一个页面显示“实际运行模型”的混杂行为
