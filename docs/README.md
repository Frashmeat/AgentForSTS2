# 文档知识库总索引

> 文档定位：`docs/` 的唯一导航入口，区分稳定架构、当前事实、当前方案和历史材料。
>
> 不包含：任务 PRD、执行日志、candidate 账本或验收原始证据。
>
> 权威规则：协作与文档治理以仓库根目录 `PROJECT_SPEC.md` 为准。
>
> 最后更新：2026-08-18

## 1. 阅读顺序

1. 先读根目录 `PROJECT_SPEC.md`，确认协作、授权和验证边界。
2. 读 [`01-总览/项目架构总览.md`](./01-总览/项目架构总览.md)，理解当前系统边界。
3. 读 [`02-现状/当前进度说明.md`](./02-现状/当前进度说明.md)，确认已实现能力和剩余缺口。
4. 读 [`03-当前方案/当前方案.md`](./03-当前方案/当前方案.md)，确认当前阶段范围和执行顺序。
5. 按当前方案进入相关专题；只有追溯背景时才进入 `90-归档/`。

当前任务的状态、PRD 和验收过程从 `.trellis/tasks/<task>/` 读取。稳定、可执行的后端合同从 `.trellis/spec/backend/` 读取，不由 `docs/` 重复维护。

## 2. 当前入口

| 文档 | 负责回答 | 不负责 |
| --- | --- | --- |
| [`项目架构总览`](./01-总览/项目架构总览.md) | 系统分层、依赖方向、主要数据和端到端流程 | 当前候选状态 |
| [`Prompt / 文本资源总览`](./01-总览/Prompt-文本资源总览.md) | Recipe、Pack guidance、Truth 和模型请求的装配边界 | Provider 单次故障流水 |
| [`当前进度说明`](./02-现状/当前进度说明.md) | 当前代码能力、精确 catalog/hash、机器证据和剩余门禁 | 详细实现协议 |
| [`当前方案`](./03-当前方案/当前方案.md) | 当前阶段目标、范围、约束、顺序和完成条件 | 历史候选账本 |
| [`Typed Behavior IR 与 Game Pack 确定性生成架构`](./03-当前方案/Typed-Behavior-IR与Game-Pack确定性生成架构方案.md) | AI 语义 IR、确定性 Game Adapter、诊断路由、请求预算和破坏性 cutover | 当前已实现能力 |
| [`通用执行内核、Game Pipeline Provider 与 Game Pack 边界`](./03-当前方案/通用Mod流水线与Game-Pack边界.md) | 不同游戏如何复用可靠性内核并保留各自生成/构建链路 | 当前已经实现的 Provider 代码合同 |

## 3. 稳定合同

下列文件属于 Trellis 稳定 spec，与当前代码一起维护：

| 文档 | 合同范围 |
| --- | --- |
| `.trellis/spec/backend/stage2-contracts.md` | Stage 2 DAG、持久化 schema、Composition 和恢复不变量 |
| `.trellis/spec/backend/error-handling.md` | typed failure、取消、Shell 映射和脱敏 |
| `.trellis/spec/backend/quality-guidelines.md` | 实现约束、场景验证和必需门禁 |

`docs/` 解释系统和当前方向；stable spec 定义可执行约束；Trellis task 保存进行中的状态与证据。三者不复制同一份时间线。

## 4. 长期参考

以下文档仍指导当前架构，但不是判断任务状态的首要入口：

| 文档 | 定位 |
| --- | --- |
| [`通用执行内核、Game Pipeline Provider 与 Game Pack 边界`](./03-当前方案/通用Mod流水线与Game-Pack边界.md) | Core、Pipeline Provider、Game Adapter 与 Pack 的长期边界；Provider foundation 已完成，当前进入 Typed Behavior IR 与确定性 Adapter 改造 |

已完成的阶段方案已移入归档：

| 归档文档 | 历史范围 |
| --- | --- |
| [`Game Pack Stage 1 职责清单与迁移设计`](./90-归档/已完成基线/2026-07-30-Game-Pack-Stage-1职责清单与迁移设计.md) | Stage 1 职责迁移与当时验收结论 |
| [`桌面后端运行时加固与发布收口方案`](./90-归档/已完成基线/2026-07-31-桌面后端运行时加固与发布收口方案.md) | 运行时、锁、取消和 release verification 收口基线 |
| [`Stage 2 分层能力与资源架构`](./90-归档/已完成基线/2026-08-02-Stage-2分层能力与资源架构方案.md) | Feature、Pack、Truth、Workspace、Resource 和 Shell 的 Stage 2 已完成基线 |
| [`可恢复分阶段 Mod 生成`](./90-归档/已完成基线/2026-08-10-可恢复分阶段Mod生成架构方案.md) | ExecutionGraph、checkpoint、claim、resume 和 commit protocol 的已完成设计 |
| [`统一生成反馈闭环`](./90-归档/已完成基线/2026-08-12-统一生成反馈闭环架构方案.md) | strict output、typed diagnosis、semantic feedback 和安全停止语义的已完成设计 |
| [`批次生成自动修复与单项调整`](./90-归档/已完成基线/2026-08-13-批次生成自动修复与单项调整架构方案.md) | 多 Item 自动技术修复和生成后单 Item 调整的已完成设计 |
| [`模型输出预算显式配置方案`](./90-归档/已完成基线/2026-08-17-模型输出预算显式配置方案.md) | Recipe/Provider 输出上限、Graph-pinned limits 和 401/403 分类的已完成设计 |

其中的日期、候选和状态只代表对应阶段；发生冲突时，以当前入口、stable spec 和代码为准。

## 5. 目录结构

```text
docs/
  README.md
  01-总览/       稳定架构与资源地图
  02-现状/       当前实现事实与缺口
  03-当前方案/   当前阶段入口、当前专题合同与长期边界
  90-归档/       已完成基线、被替代快照和仅供追溯的历史材料
```

归档材料不代表当前实现。旧 Python/Web 主线、Rust 重写过程、历史审查、旧规范和早期记录均只能用于追溯。

## 6. 维护规则

- 稳定架构或职责变化更新 `01-总览/`；当前能力和缺口更新 `02-现状/`；阶段方向和专题合同更新 `03-当前方案/`。
- Prompt、Recipe、Pack guidance 或 Truth 装配变化同步更新 Prompt / 文本资源总览。
- 任务 PRD、状态、审查、candidate 和验证过程写入 `.trellis/tasks/<task>/`，不在 `docs/` 新建平行过程文档。
- 后端可执行合同更新 `.trellis/spec/backend/`；`docs/` 只保留可读解释和入口。
- 被替代的文档移动到 `90-归档/`，不与当前入口并列。
- `docs/` 只保留本文件一个 `README.md`；新增稳定文档前先判断能否更新现有权威文档。
- 文件移动或改名时，同步更新 `PROJECT_SPEC.md`、本索引和仓库内引用。
