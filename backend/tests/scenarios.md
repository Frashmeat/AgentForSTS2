# AgentTheSpire 端到端测试场景

> 模拟真实用户输入，从简单到复杂，覆盖全部核心路径。
> 每个场景记录：用户输入原文 / 预期 Planner 输出摘要 / 代码断言 / 已知风险点

---

## 场景一览

| # | 标题 | 模式 | 语言 | 难度 | 核心测试点 |
|---|------|------|------|------|------------|
| S01 | 基础攻击卡 | 单资产 | 中文 | ⭐ | `OnPlay`、`OnUpgrade`、`CardType.Attack` |
| S02 | 战斗开始遗物 | 单资产 | 英文 | ⭐ | `BeforeCombatStart`、`RelicRarity.Common` |
| S03 | 持续型 Buff | 单资产 | 中文 | ⭐⭐ | `PowerModel`、`AfterCardPlayed`、持续回合递减 |
| S04 | X 费全体攻击卡 | 单资产 | 英文 | ⭐⭐ | `HasEnergyCostX`、`TargetType.AllEnemies`、`CapturedXValue` |
| S05 | 计数遗物（ShowCounter） | 单资产 | 中文 | ⭐⭐ | `ShowCounter`、`DisplayAmount`、计数 + 奖励 |
| S06 | 无图自定义机制 | 单资产 | 英文 | ⭐⭐ | `custom_code`、Harmony 补丁、无图像流程 |
| S07 | 批量：卡牌 + Power（带依赖） | Mod 规划 | 中文 | ⭐⭐⭐ | `depends_on`、拓扑排序、Power 先于卡牌 |
| S08 | 留手回合末卡 | 单资产 | 英文 | ⭐⭐⭐ | `HasTurnEndInHandEffect`、`OnTurnEndInHand` |
| S09 | 5 资产完整小 Mod | Mod 规划 | 中文 | ⭐⭐⭐⭐ | 批量全流程、混合类型、并发图像生成 |
| S10 | 4 资产英文主题包（三级依赖） | Mod 规划 | 英文 | ⭐⭐⭐⭐⭐ | 依赖链、Power 被多项引用、复杂 batch |

---

## S01 — 基础攻击卡

**模式：** 单资产
**语言：** 中文
**资产类型：** `card`
**难度：** ⭐

### 用户输入

```
资产名称：IronStrike
资产类型：卡牌

设计描述：
一张普通攻击卡，花费1能量，对单个敌人造成6点伤害。
升级后造成9点伤害。
稀有度：普通。
```

### 预期 Planner 输出（若走 Mod 规划路径）

```json
{
  "type": "card",
  "name": "IronStrike",
  "implementation_notes": "Inherit CustomCardModel with cost=1, type=CardType.Attack, rarity=CardRarity.Common, target=TargetType.AnyEnemy. Override OnPlay to deal 6 damage. Override OnUpgrade to increase damage to 9.",
  "needs_image": true
}
```

### 代码断言

生成的 `IronStrike.cs` 应包含：

- `[Pool(...)]` 属性
- 继承 `CustomCardModel`
- 构造器 `cost: 1, type: CardType.Attack, rarity: CardRarity.Common, target: TargetType.AnyEnemy`
- `protected override async Task OnPlay(...)` 方法体含伤害逻辑
- `protected override void OnUpgrade()` 方法体
- 本地化文件 `cards.json` 含 `IRON_STRIKE.name`

### 风险点

- 新手最常见：忘记 `[Pool(...)]` 导致启动崩溃 → 测试确认属性存在
- `OnPlay` 方法签名必须是 `(PlayerChoiceContext, CardPlay)` 不能写错

---

## S02 — 战斗开始遗物

**模式：** 单资产
**语言：** 英文
**资产类型：** `relic`
**难度：** ⭐

### 用户输入

```
Asset Name: IronShield
Asset Type: Relic

Design Description:
A common relic. At the start of each combat, the player gains 4 Block.
The relic should flash when it triggers.
```

### 预期 Planner 输出

```json
{
  "type": "relic",
  "name": "IronShield",
  "implementation_notes": "Inherit RelicModel. Rarity = RelicRarity.Common. ShouldReceiveCombatHooks = true. Override BeforeCombatStart: call Owner.Creature.GainBlockInternal(4) then Flash().",
  "needs_image": true
}
```

### 代码断言

- 继承 `RelicModel`
- `public override RelicRarity Rarity => RelicRarity.Common;`
- `public override bool ShouldReceiveCombatHooks => true;`
- `BeforeCombatStart()` override 中调用 `GainBlockInternal` 和 `Flash()`
- 注入 `SharedRelicPool` 的 Harmony patch 存在
- 本地化 `relics.json` 含 `IRON_SHIELD.title`

### 风险点

- `ShouldReceiveCombatHooks` 漏掉 → hook 静默不触发，最隐蔽的 bug
- `BeforeCombatStart` 里访问 `Owner` 时 owner 还未绑定（极端边界）

---

## S03 — 持续型 Buff（Power）

**模式：** 单资产
**语言：** 中文
**资产类型：** `power`
**难度：** ⭐⭐

### 用户输入

```
资产名称：BlazePower
资产类型：Power

设计描述：
一个名叫「烈焰」的战斗 buff，持续3回合（每回合结束 -1，降到0自动移除）。
持有者每打出一张攻击牌时，额外对当前目标造成2点伤害。
是 buff 类型，叠加方式为计数型（Counter）。
```

### 预期 Planner 输出

```json
{
  "type": "power",
  "name": "BlazePower",
  "implementation_notes": "Inherit PowerModel. Type=PowerType.Buff, StackType=PowerStackType.Counter. Override AfterTurnEnd: if Owner.IsPlayer, decrement Amount (Amount-- triggers auto-removal at 0). Override AfterCardPlayed: if cardPlay.Player.Creature == Owner and card.Type == CardType.Attack, deal 2 extra damage to cardPlay.Target.",
  "needs_image": true
}
```

### 代码断言

- `public override PowerType Type => PowerType.Buff;`
- `public override PowerStackType StackType => PowerStackType.Counter;`
- `AfterTurnEnd` override 中有 `Amount--` 或等效递减逻辑
- `AfterCardPlayed` override 中有伤害触发逻辑
- 图标路径格式为 `atlases/power_atlas.sprites/blazepower.tres`（小写）
- 本地化 `powers.json` 含 `BLAZE_POWER.title`

### 风险点

- Power 图标路径是 atlas 打包格式，不是直接 `.png`
- `Amount--` 后引擎会自动检查是否 = 0，不需要手动 `RemoveInternal()`
- 多人游戏下 `Owner.IsPlayer` 判断很重要，防止误触发敌方逻辑

---

## S04 — X 费全体攻击卡

**模式：** 单资产
**语言：** 英文
**资产类型：** `card`
**难度：** ⭐⭐

### 用户输入

```
Asset Name: ChainLightning
Asset Type: Card

Design Description:
An Attack card with X energy cost. Deals 3 damage to ALL enemies for each energy spent.
So if the player has 3 energy, it deals 9 damage to each enemy.
On upgrade, the base damage per energy increases from 3 to 4.
Target: all enemies simultaneously.
```

### 预期 Planner 输出

```json
{
  "type": "card",
  "name": "ChainLightning",
  "implementation_notes": "Inherit CustomCardModel with cost=0 (X cost), HasEnergyCostX override returns true, type=CardType.Attack, target=TargetType.AllEnemies. In OnPlay, read EnergyCost.CapturedXValue for the X amount. Deal (3 * X) damage to each creature in cardPlay.Target's side enemies.",
  "needs_image": true
}
```

### 代码断言

- `protected override bool HasEnergyCostX => true;`
- `TargetType.AllEnemies` 在构造器中
- `OnPlay` 里引用 `EnergyCost.CapturedXValue` 或等效
- 升级分支改变倍率
- 本地化描述包含 X 变量引用（如 `[X]` 占位）

### 风险点

- X 费卡的 `CapturedXValue` 在 `CardCmd.AutoPlay` 时自动填入；`OnPlay` 里直接读即可
- `TargetType.AllEnemies` 时 `cardPlay.Target` 可能是 null，需遍历 `CombatState.Enemies`

---

## S05 — 计数遗物（ShowCounter）

**模式：** 单资产
**语言：** 中文
**资产类型：** `relic`
**难度：** ⭐⭐

### 用户输入

```
资产名称：CardCounter
资产类型：遗物

设计描述：
一个罕见遗物，在遗物 UI 上显示一个计数器（ShowCounter = true）。
记录本次战斗中玩家打出的卡牌总数。
每打出5张牌，立刻获得3枚金币。
战斗胜利后计数器清零。
```

### 预期 Planner 输出

```json
{
  "type": "relic",
  "name": "CardCounter",
  "implementation_notes": "Inherit RelicModel. Rarity=RelicRarity.Rare. ShouldReceiveCombatHooks=true. Private field int _count. ShowCounter=true, DisplayAmount returns _count. Override AfterCardPlayed: increment _count, if _count % 5 == 0 give Owner 3 gold then Flash(). Override AfterCombatVictory: reset _count=0, InvokeDisplayAmountChanged().",
  "needs_image": true
}
```

### 代码断言

- `public override bool ShowCounter => true;`
- `public override int DisplayAmount => _count;`（或等效字段）
- `AfterCardPlayed` 中有模5判断和金币发放
- `AfterCombatVictory` 或 `AfterCombatEnd` 中重置计数
- `InvokeDisplayAmountChanged()` 在计数变化后调用（否则 UI 不刷新）

### 风险点

- 忘记调用 `InvokeDisplayAmountChanged()` → 计数在后台正确但 UI 不更新
- `AfterCardPlayed` 中要判断 `cardPlay.Player.Creature == Owner.Creature`，防止多人模式下其他玩家的牌也触发

---

## S06 — 无图自定义机制（custom_code）

**模式：** 单资产（走 `create_custom_code` 路径）
**语言：** 英文
**资产类型：** `custom_code`
**难度：** ⭐⭐

### 用户输入

```
Asset Name: CurseBacklash
Asset Type: Custom Code (no image)

Design Description:
A passive mechanic: whenever the player draws a Curse card into their hand,
they immediately take 2 unblockable damage.
No visual power icon needed. Implement as a relic or Harmony hook.
```

### 预期 Planner 输出

```json
{
  "type": "custom_code",
  "name": "CurseBacklash",
  "needs_image": false,
  "implementation_notes": "Implement as a RelicModel (needs_image=false variant) or a Harmony Postfix on AfterCardDrawn hook. If relic approach: ShouldReceiveCombatHooks=true, override AfterCardDrawn(ctx, card, fromHandDraw): if card.Rarity==CardRarity.Curse, call Owner.Creature.LoseHpInternal(2, unblockable ValueProp). No image needed."
}
```

### 代码断言（关键路径验证）

- 整个流程**不进入图像生成环节**（`needs_image = false`）
- `AfterCardDrawn` 或等效 hook 中有 `CardRarity.Curse` 判断
- 伤害调用使用 `LoseHpInternal` 而非 `DamageBlockInternal`（无视格挡）
- `dotnet publish` 编译通过

### 风险点

- 这个场景专门验证 `custom_code` 路径绕过图像生成的逻辑是否正常
- Planner 可能错误地将 `needs_image` 设为 true → 测试 Planner 输出

---

## S07 — 批量：卡牌 + Power（带依赖）

**模式：** Mod 规划
**语言：** 中文
**难度：** ⭐⭐⭐

### 用户输入

```
帮我制作一个「感染」主题的小 mod，包含两个资产：

1. 一个 debuff Power 叫「感染标记」（InfectionMark）：
   施加在敌人身上，每叠1层，回合结束时该敌人受到1点伤害。
   是 Debuff，Counter 型。

2. 一张技能牌叫「播种感染」（SpreadInfection）：
   花费1能量，给一个敌人施加3层感染标记。
   这张牌依赖感染标记 Power 先完成。
```

### 预期 Planner 输出

```json
{
  "mod_name": "InfectionMod",
  "items": [
    {
      "id": "power_infection_mark",
      "type": "power",
      "name": "InfectionMark",
      "depends_on": []
    },
    {
      "id": "card_spread_infection",
      "type": "card",
      "name": "SpreadInfection",
      "depends_on": ["power_infection_mark"]
    }
  ]
}
```

### 代码断言

- 拓扑排序后 InfectionMark 在 SpreadInfection 之前执行
- `InfectionMark.cs`：`PowerType.Debuff`、`AfterTurnEnd` 中 `Owner.Creature.LoseHpInternal(Amount, ...)`
- `SpreadInfection.cs`：`OnPlay` 中引用 `InfectionMark`（通过 `ModelDb.Power<InfectionMark>()` 或等效）
- `depends_on` 确保代码生成顺序正确

### 风险点

- Planner 可能不生成 `depends_on`，导致 Code Agent 写 SpreadInfection 时 InfectionMark 类还不存在 → 编译失败
- 这是测试依赖管理最核心的场景

---

## S08 — 留手回合末卡

**模式：** 单资产
**语言：** 英文
**资产类型：** `card`
**难度：** ⭐⭐⭐

### 用户输入

```
Asset Name: TimedBomb
Asset Type: Card

Design Description:
A Power-type card (CardType.Power) that costs 2 energy.
The card does NOT go to exhaust — it stays in hand until end of turn.
When the turn ends with this card still in hand, it deals 5 damage to a random enemy,
then moves to the discard pile.
On upgrade: deals 8 damage instead of 5.

This card uses the HasTurnEndInHandEffect mechanic.
```

### 预期 Planner 输出

```json
{
  "type": "card",
  "name": "TimedBomb",
  "implementation_notes": "Inherit CustomCardModel. cost=2, type=CardType.Power, target=TargetType.RandomEnemy. Override HasTurnEndInHandEffect to return true. Override OnTurnEndInHand: deal 5 damage to a random enemy using CardCmd or CreatureCmd, then discard. Override OnUpgrade: change damage to 8. GetResultPileType should return PileType.Discard (not Exhaust).",
  "needs_image": true
}
```

### 代码断言

- `public override bool HasTurnEndInHandEffect => true;`
- `public override async Task OnTurnEndInHand(PlayerChoiceContext ctx)` 实现
- `CardType.Power` 类型（注意不是 `Skill`）
- `TargetType.RandomEnemy`
- 升级改变伤害值

### 风险点

- `HasTurnEndInHandEffect` 必须同时 override，光实现 `OnTurnEndInHand` 不会触发
- `CardType.Power` 类型卡在游戏里通常是"永久效果"语义，但这里用 Power 类型是视觉风格选择

---

## S09 — 5 资产完整小 Mod（批量全流程）

**模式：** Mod 规划
**语言：** 中文
**难度：** ⭐⭐⭐⭐

### 用户输入

```
帮我做一个「黑暗法师」主题的 mod，包含以下内容：

攻击牌 1：暗影飞刃（ShadowBlade），1费，对单个敌人造成5点伤害，升级后8点。普通卡。

攻击牌 2：吸血突击（VampiricStrike），1费，造成6点伤害，同时回复等同伤害量的一半生命（如伤害6则回血3）。罕见卡。

技能牌：诅咒汇聚（CurseGather），0费，检查手牌中的诅咒牌数量，每张诅咒获得1层力量（Strength），然后消耗。

遗物：黑暗契约（DarkPact），普通遗物。每场战斗开始时往牌堆里加入1张诅咒牌。作为补偿，获得1点临时能量（当回合有效）。

以上资产无明显依赖关系，可以并行处理。
```

### 预期 Planner 输出摘要

5个 items，全部 `needs_image: true`，`depends_on: []`（无依赖），拓扑排序后顺序任意。

### 代码断言

- 5 个独立 C# 文件均被创建
- `CurseGather`：`OnPlay` 中遍历手牌统计 `CardRarity.Curse` 数量，应用 Strength Power
- `VampiricStrike`：`OnPlay` 中伤害量 → 回血量（除以2）
- `DarkPact`：`BeforeCombatStart` 中使用 `CardCmd` 或 `CardPileCmd` 添加诅咒牌
- 5 个条目的本地化 json 条目均存在
- `dotnet publish` 通过

### 风险点

- 批量并发生图（最多2个并发信号量）：测试 5 个资产的图像生成不会因信号量死锁
- Code Agent 串行执行 5 次：耗时较长，WebSocket 超时风险
- Strength Power 的 apply 方式需要使用真实 API（不能凭空调用）

---

## S10 — Soul Harvester 主题包（三级依赖链）

**模式：** Mod 规划
**语言：** 英文
**难度：** ⭐⭐⭐⭐⭐

### 用户输入

```
Create a 'Soul Harvester' themed mod with 4 assets:

1. Power — SoulMark (debuff, Counter type):
   Enemies with Soul Mark take 20% extra damage per stack (additive).
   Applied to enemies only.

2. Relic — HarvesterScythe (Rare):
   Whenever the player kills an enemy, apply 2 stacks of SoulMark to a random living enemy.
   Depends on SoulMark existing.

3. Relic — SoulChalice (Uncommon):
   At the start of each combat, apply 1 SoulMark to all enemies.
   Depends on SoulMark existing.

4. Card — SoulRend (1 cost Attack, Uncommon):
   Deal 8 damage to one enemy. If the target has any SoulMark stacks, deal double damage (16).
   Depends on SoulMark existing.

Dependency summary:
- SoulMark: no dependencies
- HarvesterScythe, SoulChalice, SoulRend: all depend on SoulMark
```

### 预期 Planner 输出

```json
{
  "mod_name": "SoulHarvester",
  "items": [
    { "id": "power_soul_mark",       "depends_on": [] },
    { "id": "relic_harvester_scythe","depends_on": ["power_soul_mark"] },
    { "id": "relic_soul_chalice",    "depends_on": ["power_soul_mark"] },
    { "id": "card_soul_rend",        "depends_on": ["power_soul_mark"] }
  ]
}
```

拓扑排序后：SoulMark 排第一，其余三项顺序任意但均在 SoulMark 之后。

### 代码断言

**SoulMark（Power）：**
- `PowerType.Debuff`, `PowerStackType.Counter`
- `TryModifyPowerAmountReceived` 或 `BeforeDamageReceived` hook 中乘以 `(1 + 0.2 * Amount)` 倍率

**HarvesterScythe（Relic）：**
- `AfterDeath(ctx, creature, ...)` override
- 调用 `ModelDb.Power<SoulMark>()` 获取 canonical Power，ToMutable + Apply 流程
- `Flash()` 触发动画

**SoulChalice（Relic）：**
- `BeforeCombatStart()` override
- 遍历 `Owner.Creature.CombatState.Enemies`，为每个 apply 1 层 SoulMark

**SoulRend（Card）：**
- `OnPlay` 中检查 `cardPlay.Target.HasPower<SoulMark>()`
- 伤害值条件分支：8 or 16

**全局：**
- 拓扑排序验证：SoulMark 的代码文件在其他三项之前生成
- 4 个本地化条目均存在
- 编译通过

### 风险点

- 三个资产同时依赖 SoulMark：批量 WebSocket 的 `item_done_events` 并发等待逻辑压力测试
- `BeforeDamageReceived` hook 修改伤害量的正确签名：`(ctx, target, amount, props, dealer, cardSource)` —— 参数顺序易搞错
- SoulMark 作为 Debuff，`Owner` 是敌方 Creature；`Owner.IsPlayer` 为 false，逻辑要区分清楚

---

## 测试执行指南

### 单资产场景（S01-S06, S08）
1. 启动 AgentTheSpire（`uvicorn main:app` + `npm run dev`）
2. 选择"单资产"Tab
3. 填入对应场景的资产名称、类型、设计描述
4. 执行并对照"代码断言"检查生成的 `.cs` 文件

### 批量场景（S07, S09, S10）
1. 选择"Mod 规划"Tab
2. 粘贴对应的"用户输入"文本
3. 在"审阅计划"阶段检查：
   - `implementation_notes` 是否包含真实 API 名（`OnPlay`、`PowerModel` 等）
   - `depends_on` 是否正确
4. 确认后执行，对照"代码断言"检查

### 快速冒烟测试顺序
建议执行顺序：`S02 → S01 → S06 → S03 → S07 → S09`
（先验证最简路径，再覆盖批量和依赖）

---

## 已知边界情况

| 情况 | 对应场景 | 预期行为 |
|------|----------|----------|
| `needs_image=false` 的资产直接进代码生成，跳过图像环节 | S06 | 不出现 prompt_preview 事件 |
| 所有依赖同一 Power 的三个资产并发等待 | S10 | SoulMark done event 广播，三者同时解锁 |
| 批量中有一项失败，其余继续 | S09（注入错误测试） | `item_error` 事件，其他 item 不受影响，`error_count` = 1 |
| Planner 输出缺少 `depends_on` | S07（负面测试） | Code Agent 遇到编译错误，`dotnet publish` 失败后报错 |

---

## Chunk 6 迁移回归补充场景

### S11 — 单资产审批通过 / 拒绝

**模式：** 单资产
**目标：**
- `approval_first` 模式下先收到 `approval_pending`
- 批准后继续推送 `agent_stream` / `done`
- 拒绝后直接返回失败态，不继续执行代码生成

**检查点：**
- `approval_pending.summary` 与 `requests[]` 非空
- 前端审批面板状态会从 `pending` 切到 `approved` / `rejected`
- 拒绝路径最终 `done.success == false`

### S12 — 批量依赖流程 + 审批通过 / 拒绝

**模式：** Mod 规划
**目标：**
- 依赖组在 `item_image_ready` 后等待组内资源就绪
- 组审批通过后进入 `item_agent_stream`
- 任一组被拒绝时，仅该组终止，`batch_done` 仍能汇总 `success_count` / `error_count`

**检查点：**
- `depends_on` 排序稳定
- `item_approval_pending` 带 `item_id`
- `batch_done` 统计与实际成功/失败数一致

### S13 — 图像供应商切换

**模式：** 单资产 / Mod 规划
**目标：**
- `image_gen.provider` 在 `bfl` / `volcengine` / `wanxiang` 间切换后仍能生成 prompt 并进入出图
- prompt adapter 的 provider 映射不破坏透明背景资产流程

**检查点：**
- relic / power 仍走透明背景路径
- `prompt_preview` 与 `image_ready` 事件协议不变
- 供应商切换不影响后续 postprocess 输出路径

### S14 — Claude / Codex backend 切换

**模式：** 单资产 / Mod 规划
**目标：**
- `llm.agent_backend` 在 `claude` / `codex` 间切换后，审批规划、代码生成、构建修复入口都可继续运行
- 统一 `agent_runner` / `text_runner` 不因 backend 切换退回旧直连逻辑

**检查点：**
- planner 输出结构一致
- `agent_stream` 有连续输出
- 构建失败时仍能进入统一修复回路

### 推荐迁移演练顺序

1. 关闭全部 migration flags，执行 `S01 + S07 + S11`，确认 legacy 默认安全。
2. 仅开启 `use_unified_ws_contract`，执行 `S11 + S12`，确认前端事件契约无回归。
3. 仅开启 `use_modular_single_workflow`，执行 `S01 + S13 + S14`。
4. 仅开启 `use_modular_batch_workflow`，执行 `S07 + S12 + S14`。
5. 三个开关全开后，再跑一次 `S09 + S10 + S13 + S14` 作为收尾验证。
