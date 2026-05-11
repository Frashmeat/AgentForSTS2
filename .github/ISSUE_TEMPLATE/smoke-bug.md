---
name: 冒烟测试 bug 报告
about: Windows 桌面端 9 handler / 4 page 冒烟测试中发现的 bug
title: "[smoke] <handler 或 page> - <一句话症状>"
labels: ["bug", "smoke-test"]
assignees: []
---

<!--
冒烟测试清单：docs/03-方案/全栈重写/进行中/2026-05-11-Windows冒烟测试清单.md
收集脚本：scripts/collect-bug-report.ps1

提交前请先跑收集脚本生成诊断 zip，再附到本 issue。
.\scripts\collect-bug-report.ps1 -ProjectRoot <你的工程路径>
-->

## 在哪里出问题

- [ ] handler：text_generate / code_generate / asset_generate / batch_custom_code / build_project / package_project / single_asset_plan / log_analysis / knowledge_refresh
- [ ] page：Dashboard / ModEditor / BatchGeneration / LogAnalysis
- [ ] 其它（具体说明）：

## 复现步骤

1.
2.
3.

**是否稳定复现**：是 / 否（出现 ~N 次中的 M 次）

## 预期 vs 实际

**预期**：

**实际**：

## LLM 配置（不要贴 api_key）

- `llm.provider`：anthropic / openai / new_api / 其它
- `llm.model`：
- `llm.base_url`：（如果是第三方代理）
- `image_gen.provider`（如适用）：

## 环境

- Windows build：（运行 `winver` 看）
- 是否首次跑：是 / 否
- 工程路径含中文 / 长路径：是 / 否

## 诊断材料

请把 `scripts\collect-bug-report.ps1` 生成的 zip 拖进来：

<!-- 拖 bug-report-*.zip 到这里 -->

如果 console / devtools 里有 panic 或 stack trace，贴这里：

```
（panic 或错误堆栈）
```

## 其它备注

<!-- 任何你觉得相关的：之前跑过什么、是否切换过 provider、网络代理设置等 -->
