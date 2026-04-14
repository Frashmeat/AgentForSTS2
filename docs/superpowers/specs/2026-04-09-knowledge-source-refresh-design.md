# 知识库版本检查与手动更新设计

## 背景

当前工作站生成 mod 时，知识库主要依赖两类来源：

- 游戏侧：本机 `sts2_path` 对应的游戏安装目录和反编译结果
- Baselib 侧：仓库内静态保存的 `BaseLib.decompiled.cs`

现状问题是，这两类知识都可能在游戏或 Baselib 更新后过时，但系统当前缺少统一的版本检查、状态提示和刷新入口。用户已经明确确认本轮目标：

- 启动时自动检查知识库是否过期
- 提供手动更新知识库按钮
- 另外提供只读的“知识库说明”弹窗，明确来源与工程内位置

## 已确认边界

- 游戏版本来源取自 Steam 的 `app manifest / 安装版本文本`，不是 `sts2.dll` 文件版本。
- Baselib 版本来源取自 GitHub latest release tag：
  - <https://github.com/Alchyr/BaseLib-StS2/releases>
- 第一版 stale 判定只按“版本是否变化”处理，不严格比较 hash。
- 用户侧既要有“手动更新知识库”按钮，也要有单独的“知识库说明”弹窗。
- 说明弹窗只展示来源和工程内位置，不展开精细操作步骤。
- stale / missing 状态下提示用户先更新，但不强制阻断生成。

## 目标

在不改变现有工作站主流程的前提下，为本地知识库补齐以下能力：

1. 统一状态检查：启动后可判断当前知识库是 `checking`、`fresh`、`stale` 还是 `missing`
2. 手动更新：用户点击按钮后，可重新同步游戏与 Baselib 的本地知识缓存
3. 来源说明：用户可随时查看知识库真源与工程内缓存位置
4. prompt 收口：codegen 不再无条件声称仓库内 Baselib 副本是长期权威来源

## 设计总览

### 真源定义

- 游戏真源：
  - 自动检测到的 `sts2_path`
  - 由该安装所在 Steam 库的 `appmanifest_*.acf`（或等价安装版本文本）提供版本号
- Baselib 真源：
  - GitHub latest release tag
  - 优先下载 release 中的 `BaseLib.dll` 作为本地反编译输入

### 本地缓存布局

统一放在工程内：

- `runtime/knowledge/knowledge-manifest.json`
- `runtime/knowledge/game/`
- `runtime/knowledge/baselib/`
- `runtime/knowledge/cache/`

其中：

- `game/` 保存最新一次成功反编译的游戏源码
- `baselib/` 保存最新一次成功反编译的 Baselib 源码
- `cache/` 保存下载到本地的 Baselib release 资产

### 状态模型

统一状态：

- `checking`
- `fresh`
- `stale`
- `missing`
- `refreshing`
- `error`

状态合并规则：

- manifest 不存在，或核心本地缓存目录不存在：`missing`
- 游戏版本与 manifest 不一致，或 Baselib latest tag 与 manifest 不一致：`stale`
- 两侧都匹配：`fresh`
- 正在执行检查：`checking`
- 正在执行手动更新：`refreshing`
- 检查/更新异常但本地仍有旧缓存：保留旧缓存并报告 `stale` 或 `error`

## Manifest 结构

建议保存为 JSON：

```json
{
  "schema_version": 1,
  "status": "fresh",
  "generated_at": "2026-04-09T10:30:00+08:00",
  "game": {
    "sts2_path": "C:/Steam/steamapps/common/Slay the Spire 2",
    "version": "0.2.15",
    "version_source": "steam_app_manifest",
    "knowledge_path": "I:/WebCode/AgentTheSpire/runtime/knowledge/game",
    "decompiled_src_path": "I:/WebCode/AgentTheSpire/runtime/knowledge/game"
  },
  "baselib": {
    "release_tag": "v0.2.8",
    "release_published_at": "2026-04-07T20:27:01Z",
    "asset_name": "BaseLib.dll",
    "downloaded_file_path": "I:/WebCode/AgentTheSpire/runtime/knowledge/cache/BaseLib.dll",
    "knowledge_path": "I:/WebCode/AgentTheSpire/runtime/knowledge/baselib",
    "decompiled_src_path": "I:/WebCode/AgentTheSpire/runtime/knowledge/baselib"
  },
  "last_check": {
    "checked_at": "2026-04-09T10:35:00+08:00",
    "game_matches": true,
    "baselib_matches": false,
    "warnings": []
  }
}
```

## 启动检查流程

1. 应用启动后，后台异步执行知识库检查
2. 读取本地 manifest
3. 游戏侧：
   - 读取当前配置的 `sts2_path`
   - 反推 Steam 库路径
   - 读取 `appmanifest_*.acf` 或等价安装版本文本
   - 提取与 `Slay the Spire 2` 对应的版本信息
4. Baselib 侧：
   - 请求 GitHub latest release API
   - 获取 latest tag 与资产清单
5. 与 manifest 对比并给出统一状态

## 手动更新流程

1. 用户点击“更新知识库”
2. 后端创建更新任务，前端轮询任务进度
3. 游戏侧更新：
   - 找到当前游戏 DLL
   - 运行 `ilspycmd` 反编译到 `runtime/knowledge/game/`
   - 记录当前 Steam 安装版本
4. Baselib 侧更新：
   - 获取 latest release
   - 下载 `BaseLib.dll`
   - 反编译到 `runtime/knowledge/baselib/`
   - 记录 release tag
5. 两侧都成功后，覆盖写入 manifest
6. 若其中一侧失败：
   - 不覆盖上一次成功基线
   - 返回失败状态和错误说明

## 前端交互

### 设置页

新增“知识库状态”卡片，展示：

- 当前状态
- 游戏版本
- Baselib release tag
- 上次检查时间
- 上次成功更新时间

按钮：

- `检查更新`
- `更新知识库`
- `查看知识库说明`

### 生成页

当状态为 `stale` 或 `missing` 时，在单资产生成入口附近展示提示条：

- 文案：当前知识库可能不是最新版本，建议先更新
- 操作：
  - `立即更新`
  - `查看说明`

### 说明弹窗

仅展示：

- 游戏知识库来源：
  - 自动检测到的 `sts2_path`
  - Steam `app manifest / 安装版本文本`
- Baselib 知识库来源：
  - <https://github.com/Alchyr/BaseLib-StS2/releases>
- 工程内位置：
  - `runtime/knowledge/knowledge-manifest.json`
  - `runtime/knowledge/game/`
  - `runtime/knowledge/baselib/`
  - `runtime/knowledge/cache/`

## Prompt 与知识路径收口

当前 codegen prompt 中存在“仓库内 Baselib 副本是 authoritative”的绝对表述，需要调整。

目标口径：

- codegen 优先读取 manifest 指向的本地 Baselib 反编译缓存
- 游戏反编译路径优先读取 manifest 指向的游戏反编译缓存
- 若知识库状态为 `stale`，prompt 中显式注明“当前知识缓存可能不是最新版本”
- 仓库内的 `backend/agents/baselib_src/BaseLib.decompiled.cs` 仅作为 fallback，而不是长期事实真源

## 风险

- Steam 安装版本文本在不同安装布局下可能存在多个路径，需要主路径加 fallback。
- 若上游内容变化但版本号未变化，第一版无法识别。
- GitHub latest release 请求失败时，需要确保系统仍能保留旧缓存并提示风险，而不是直接中断全部生成流程。

## 验收标准

- 启动后能读取并展示知识库状态
- 状态能正确反映游戏版本和 Baselib latest tag 的变化
- 用户可从设置页和生成页手动触发更新
- 用户可查看知识库说明弹窗，且能看到真源和工程内位置
- codegen prompt 不再无条件把仓库内 Baselib 副本声明为长期权威来源
