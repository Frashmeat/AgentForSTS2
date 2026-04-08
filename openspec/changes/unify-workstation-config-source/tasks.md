## 1. OpenSpec 与边界冻结

- [x] 1.1 建立 `unify-workstation-config-source` 的 proposal / design / tasks / spec 文档
- [x] 1.2 冻结 `runtime/workstation.config.json` 为唯一应用级配置真源
- [x] 1.3 明确根目录 `config.json`、`services/workstation/config.json`、历史 Docker 配置和 CLI 全局配置的边界

## 2. 配置读取与写入收敛

- [x] 2.1 调整工作站配置加载逻辑，只读取 `runtime/workstation.config.json`
- [x] 2.2 调整打包/部署脚本，只向唯一真源写入工作站应用配置
- [x] 2.3 为旧路径提供迁移提示或兼容策略，避免旧产物静默继续成为真源

## 3. CLI 与外部配置边界

- [x] 3.1 明确 Codex / Claude 用户全局配置不属于项目配置真源
- [ ] 3.2 在设置/诊断能力中只把这些全局配置作为外部来源或风险摘要呈现
- [ ] 3.3 让 `align-cli-runtime-config` 的后续实现复用本次真源规则

## 4. 文档与验证

- [x] 4.1 更新 `tools/README.md` 与相关运行文档，统一配置真源口径
- [x] 4.2 更新后端实施计划/实施基线文档，补充本次冻结约束
- [x] 4.3 做最小验证：确认同一 release 内不再存在两个会分叉的工作站应用配置真源
