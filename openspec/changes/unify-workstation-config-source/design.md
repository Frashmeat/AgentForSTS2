## Context

当前工作站应用配置至少同时出现在三个位置：

- 仓库根目录 `config.json`
- release 内 `runtime/workstation.config.json`
- release 内 `services/workstation/config.json`

其中：

- 后端配置加载逻辑当前会直接读取运行实例目录下的 `config.json`
- 打包/部署脚本会在不同目录写入配置，且部分路径存在“沿用旧 release 设置”的历史逻辑
- 文档虽然说明部分目标会同时写入 `services/workstation/config.json` 与 `runtime/workstation.config.json`，但这实际上制造了两个平级事实源
- Codex CLI 与 Claude CLI 还会继续读取各自用户级全局配置，这些配置不应该再被理解为项目配置本体

这次设计的目标不是立即重做整个设置系统，而是先冻结“应用级配置真源”与“外部 CLI 私有配置边界”，为后续实现提供不会继续漂移的基础。

## Goals / Non-Goals

**Goals**

- 将 `runtime/workstation.config.json` 固定为工作站应用级配置的唯一真源。
- 明确其他项目内配置文件只能是迁移输入、兼容镜像或待废弃路径，不再具备同级真源地位。
- 明确 Codex / Claude 用户全局配置不属于项目配置真源，只作为外部环境输入或风险来源存在。
- 为后续实现提供稳定的读取规则、写入规则和迁移顺序。

**Non-Goals**

- 本轮不重做整套设置 UI，也不一次性实现所有 CLI 运行时隔离细节。
- 本轮不把所有服务统一到新的多 profile 配置系统。
- 本轮不保留多个长期并行的项目级配置真源。
- 本轮不继续扩展 Docker 历史链路的独立配置能力。

## Decisions

### 1. `runtime/workstation.config.json` 是唯一应用级配置真源

后续工作站运行、hybrid/workstation/full 相关 release 产物、以及需要表达“当前实际运行配置”的脚本与文档，统一以 `runtime/workstation.config.json` 为准。

原因：

- 该路径天然位于运行时目录中，最贴近实际启动环境。
- 它比仓库根目录 `config.json` 更能代表“当前这个 release / 当前这个运行实例”的真实配置。
- 统一到 `runtime/` 可以避免把开发态模板和发布态运行配置混为一谈。

### 2. `services/workstation/config.json` 不再是独立真源

`services/workstation/config.json` 在迁移期只允许存在以下两种角色之一：

- 兼容占位文件，由唯一真源派生生成
- 明确废弃并移除

它不得再作为工作站后端的独立读取入口，不得与 `runtime/workstation.config.json` 同时作为平级事实源存在。

原因：

- 当前已经出现两者内容分叉并导致实际运行行为偏离预期。
- 服务目录更像部署内容布局，不适合作为长期配置真源。

### 3. 根目录 `config.json` 降级为开发输入或迁移来源

仓库根目录 `config.json` 不再被定义为工作站运行时真源。后续它只能承担以下职责之一：

- 本地开发辅助输入
- 安装/迁移脚本的来源模板
- 与 `config.example.json` 配套的用户编辑入口

但它不能继续与 `runtime/workstation.config.json` 并列作为“当前运行实例正在读取的配置”。

### 4. Docker 历史配置链路不再拥有独立所有权

历史 Docker / mixed deploy 脚本如果仍需为兼容场景服务，也只能消费同一真源并生成运行参数，不得再创建新的工作站配置真源副本。

原因：

- Docker 相关部署已经不是当前首选路径。
- 如果历史链路继续维护自己的配置副本，会再次制造分叉。

### 5. Codex / Claude 全局配置属于外部环境，不属于项目真源

`~/.codex/...`、Claude CLI 用户级设置等全局配置，不再被视为项目应用配置的一部分。项目只需要明确：

- 应用真源中哪些字段会注入到 CLI
- CLI 全局配置是否被隔离、兼容覆盖或存在风险
- 如何把这些信息以摘要形式展示给用户

项目不镜像、不复制、不双写这些 CLI 私有配置文件。

## Risks / Trade-offs

- [迁移风险] 现有脚本或运行实例可能仍直接读取 `services/workstation/config.json` -> 需要兼容期和迁移提示，不能直接静默删除。
- [心智迁移成本] 现有用户可能仍把根目录 `config.json` 视为唯一入口 -> 文档和设置摘要必须明确说明“开发输入”与“运行真源”的区别。
- [历史产物残留] 老 release 目录可能长期保留旧配置副本 -> 需要定义“遇到旧路径时如何提示或迁移”的规则。
- [范围联动] CLI runtime alignment 与配置真源统一高度相关 -> 本 change 只冻结真源与边界，不在本轮把所有 CLI 隔离细节一次做完。

## Migration Plan

1. 在 OpenSpec 中冻结“唯一真源 = `runtime/workstation.config.json`”的规则。
2. 调整配置加载逻辑，让工作站运行时首先且最终只从唯一真源读取。
3. 调整打包与部署脚本，只向唯一真源写入应用配置；旧路径仅保留兼容迁移策略。
4. 为根目录 `config.json`、旧 release 配置和历史 Docker 配置定义迁移/回退规则。
5. 在 CLI runtime alignment 与设置可观测性变更中复用该规则，避免再次引入多个项目级配置来源。

## Open Questions

- 迁移期是否需要继续自动从根目录 `config.json` 生成 `runtime/workstation.config.json`，还是只保留显式脚本入口？
- `services/workstation/config.json` 是保留一个由真源派生的兼容镜像，还是在切换后直接移除更合适？
- 设置页未来是否要同时展示“开发输入来源”和“当前运行真源”，还是只展示运行真源即可？
