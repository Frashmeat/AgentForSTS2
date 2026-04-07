## ADDED Requirements

### Requirement: Settings API MUST Distinguish Saved Values from Effective Runtime Values

The settings API MUST expose enough information for clients to distinguish persisted configuration from the values that code-agent runtime will actually use.

#### Scenario: Settings response includes saved and effective LLM summaries

- **WHEN** 前端请求设置接口
- **THEN** 响应必须至少包含保存的 LLM 配置摘要
- **AND** 必须包含当前代码代理运行时的生效摘要
- **AND** 若存在外部覆盖来源或隔离告警，必须提供可展示的提示信息

#### Scenario: Masked secrets remain masked in both views

- **WHEN** 设置接口返回保存值和生效值摘要
- **THEN** 两个视图中的密钥类字段都必须继续脱敏
- **AND** 前端无需额外推断哪些字段可安全显示

### Requirement: Settings UI MUST Explain Runtime Divergence in User-Friendly Language

The settings UI MUST explain whether the saved configuration is the same as the effective runtime configuration so users can tell if their latest changes are actually being used.

#### Scenario: Settings panel shows no divergence when values are aligned

- **WHEN** 保存值与运行时生效值一致
- **THEN** 设置页应明确表现为“当前已生效”或等价状态
- **AND** 用户不需要通过运行结果反推配置是否成功

#### Scenario: Settings panel warns when external configuration may affect runtime

- **WHEN** 检测到外部 CLI 配置与应用值冲突，或当前仍存在运行时覆盖风险
- **THEN** 设置页必须展示清晰的中文提示
- **AND** 提示需说明问题属于“外部配置干扰”“已隔离但存在历史差异”或等价语义
