## 1. OpenSpec 与契约

- [x] 1.1 完成 `unify-error-handling-contract` 的 proposal / design / spec 文档
- [x] 1.2 明确统一错误 envelope 字段与 HTTP / WebSocket 对齐边界

## 2. 后端统一错误出口

- [x] 2.1 新增后端统一错误模型与错误响应构造工具
- [x] 2.2 在应用装配层注册 `HTTPException` 与通用异常的全局处理器
- [x] 2.3 为工作站长流程的错误输出补齐共享字段集合
- [x] 2.4 为后端统一错误出口补充定向测试

## 3. 前端统一解析与展示

- [x] 3.1 扩展前端 HTTP 共享层，优先解析结构化错误 envelope
- [x] 3.2 扩展前端共享错误解析器，统一兼容 HTTP / WebSocket / 未知异常
- [x] 3.3 将高频页面与 controller 接入新的共享错误解析层
- [x] 3.4 为前端错误解析与展示兜底补充定向测试

## 4. 验证与收口

- [x] 4.1 验证 HTTP 已知业务错误、未知异常和空错误文本场景
- [x] 4.2 验证工作站 WebSocket `error` 事件的统一展示行为
- [x] 4.3 更新必要文档或接口说明，记录统一错误字段约定
