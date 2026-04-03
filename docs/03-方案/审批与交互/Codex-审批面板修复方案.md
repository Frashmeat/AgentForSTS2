# 修改计划：Codex 审批面板修复

## Context

用户使用 Codex 后端时遇到两个问题：
1. `AGENTS_CODEX.md` 的工作规范（包括沙箱失败处理指引）没有被注入到 Codex 的 prompt，导致 Codex 不遵守这些规范
2. 审批面板从未弹出，因为 `config.json` 没有 `execution_mode` 字段（默认 `legacy_direct`），且 SettingsPanel 没有该开关

用户期望：审批面板弹出 → 用户查看 Codex 将执行的操作预览 → 点击确认 → 系统继续调用 Codex 实际生成代码。

---

## 改动范围（5 个文件）

### Fix 1：注入 AGENTS_CODEX.md（后端）

**文件**：`backend/llm/agent_runner.py`

在 `build_agent_prompt()` 中，当 `backend == "codex"` 时，从仓库根目录读取 `AGENTS_CODEX.md` 并拼接到 prompt 最前面。

```python
_AGENTS_CODEX_PATH = Path(__file__).parent.parent.parent / "AGENTS_CODEX.md"

def _inject_agents_codex(prompt: str) -> str:
    if not _AGENTS_CODEX_PATH.exists():
        return prompt
    agents_codex = _AGENTS_CODEX_PATH.read_text(encoding="utf-8").strip()
    return f"{agents_codex}\n\n---\n\n{prompt}"

def build_agent_prompt(prompt: str, llm_cfg: dict, use_runtime_config: bool = False) -> str:
    effective_cfg = _with_latest_runtime_custom_prompt(llm_cfg) if use_runtime_config else llm_cfg
    backend = normalize_llm_config(effective_cfg).get("agent_backend", "claude")
    if backend == "codex":
        prompt = _inject_agents_codex(prompt)
    return append_global_ai_instructions(prompt, effective_cfg)
```

---

### Fix 2：审批后继续调 Codex（后端）

**文件**：`backend/routers/workflow.py`

当前 `approval_first` 分支在发送 `approval_pending` 后直接 `return`，WebSocket 关闭，Codex 永远不会被调用。

**修改**：移除 `return`，改为等待前端发送 `{"action": "approve_all"}` 消息，收到后继续执行 Code Agent。

需在 **3 处** 应用相同的模式：
- `ws_create()` 第 215 行
- `_ws_run_custom_code()` 第 273 行
- `_ws_run_with_provided_image()` 第 348 行

修改后的 approval_first 分支：

```python
if cfg["llm"].get("execution_mode") == "approval_first":
    summary, actions = await _plan_approval_requests(description, cfg["llm"], project_root)
    await _send_approval_pending(ws, summary, actions)
    # 等待用户审批决定（不再 return）
    raw = await ws.receive_text()
    decision = json.loads(raw)
    if decision.get("action") != "approve_all":
        await _send(ws, "done", {"success": False, "image_paths": [], "agent_output": "用户取消执行"})
        return
    await _send_stage(ws, "agent", "agent_running", "审批通过，开始生成代码...")
    await _send(ws, "progress", {"message": "审批通过，Code Agent 开始生成代码..."})
# 继续原有的 create_asset() / create_custom_code() 调用（代码不变）
```

---

### Fix 3：ApprovalPanel 增加"确认执行"按钮（前端组件）

**文件**：`frontend/src/components/ApprovalPanel.tsx`

添加可选 `onProceed` prop，当传入时，在面板底部显示"确认，开始生成代码"按钮：

```tsx
export function ApprovalPanel({
  summary,
  requests,
  busyActionId = null,
  onApprove,
  onReject,
  onExecute,
  onProceed,  // 新增
}: {
  // ...
  onProceed?: () => void;
}) {
  return (
    <div className="space-y-3">
      {/* 现有内容不变 */}
      ...
      {onProceed && (
        <button
          onClick={onProceed}
          className="w-full rounded-md bg-amber-500 px-3 py-2 text-sm font-medium text-white hover:bg-amber-600"
        >
          确认，开始生成代码
        </button>
      )}
    </div>
  );
}
```

---

### Fix 4：App.tsx 接入确认信号（前端）

**文件**：`frontend/src/App.tsx`

在 `ApprovalPanel` 调用处传入 `onProceed` 回调，通过 WebSocket 发送 `approve_all`：

```tsx
<ApprovalPanel
  summary={approvalSummary}
  requests={approvalRequests}
  busyActionId={approvalBusyActionId}
  onApprove={(actionId) => { void handleApprovalAction(actionId, approveApproval); }}
  onReject={(actionId) => { void handleApprovalAction(actionId, (id) => rejectApproval(id)); }}
  onExecute={(actionId) => { void handleApprovalAction(actionId, executeApproval); }}
  onProceed={() => { socket?.send({ action: "approve_all" }); }}  // 新增
/>
```

---

### Fix 5：SettingsPanel 增加执行模式开关（前端）

**文件**：`frontend/src/components/SettingsPanel.tsx`

在"代码代理后端"字段之后插入一个新的 `Field`：

```tsx
<Field
  label="代码执行模式"
  hint="审批模式：执行前展示操作预览，用户确认后再调用代理。推荐在使用 Codex 时开启。"
>
  <select
    value={cfg.llm?.execution_mode || "legacy_direct"}
    onChange={e => set(["llm", "execution_mode"], e.target.value)}
    className={selectCls}
  >
    <option value="legacy_direct">直接执行</option>
    <option value="approval_first">审批后执行</option>
  </select>
</Field>
```

---

## 数据流（修改后）

```
用户在 SettingsPanel 开启"审批后执行"
  ↓
用户开始创建资产（生图 → 选图 → 后处理完成）
  ↓
workflow.py 检测 execution_mode == "approval_first"
  ↓
LLM 生成 action list（Codex 将执行的操作预览）
  ↓
ws 发送 approval_pending 事件 → 前端显示 ApprovalPanel
  ↓
用户查看操作清单，点击"确认，开始生成代码"
  ↓
前端 ws.send({ action: "approve_all" })
  ↓
后端收到 approve_all → 继续执行 create_asset()
  ↓
调用 Codex CLI（此时 prompt 包含 AGENTS_CODEX.md 内容）
  ↓
流式推送 agent_stream 事件 → 正常完成
```

---

## 不在本次范围内

- BatchMode 的 approval 继续流程
- 图生失败重试机制
- InMemoryApprovalStore 持久化

---

## 验证方式

1. 在 SettingsPanel 将"代码执行模式"切换为"审批后执行"，保存
2. 创建一个新资产，完成选图后
3. **预期**：弹出"等待审批"面板，展示 Codex 将执行的操作清单
4. 点击"确认，开始生成代码"
5. **预期**：面板消失，agent_stream 开始输出（Codex 开始工作）
6. 检查 Codex 输出：应包含 AGENTS_CODEX.md 定义的格式（结果/变更/验证/风险）
