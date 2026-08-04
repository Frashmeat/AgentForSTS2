# Order 3: Resource Prepare v2 Foundation

## Goal

把 Resource v1 的“声明 media type 后立即 selected”升级为真实 PNG 探测、master/derived 确定性派生、candidate/explicit selected 和可复算 provenance，为 Order 4 Resource Workbench 与 Relic 纵向 Gate 提供后端地基。

## Decisions

- ResourceAsset 破坏性升级 schema v2；`selectedVersion` 可空，新上传、AI、Pack default 与派生结果都先成为 candidate。
- ResourceVersion 保存真实 `width/height/hasAlpha`，media type 来自字节探测，不信任扩展名或请求声明。
- derived provenance 绑定 source role/version、transform Primitive/version、parameters hash 与 pinned Pack identity。
- `pack.resource-specs` v2 声明 master/derived role、exact dimensions、target path 和受限 transform operation；Pack 不携带脚本。
- Resource Prepare request/result 升级 v2；master 请求确定性产出 master 与全部直接派生 role candidates，单 role 请求作为显式覆盖候选。
- Adapter 使用锁文件中已存在的窄 `png` codec，支持 RGBA8 PNG 探测、确定性 resize 与 outline；不引入完整图像框架。
- 所有 transform/probe 在存储前完成；批量写入必须 staging + rollback，任一失败不改变既有 selected version。
- Order 3 不做 React Workbench、预览命令或 Item 绑定；这些属于 Order 4。

## Acceptance Criteria

- [x] PNG signature/CRC/解码错误、错误 media type、错误尺寸和缺 alpha 在落盘前 typed 失败。
- [x] Pack v2 master/derived graph、transform Primitive/version/parameters 经过验证。
- [x] master candidate 可确定性生成 normal/outline/big，重复输入得到相同版本 hash。
- [x] 新 candidate 不自动 selected；显式 select 后 selected 精确更新。
- [x] provenance 绑定 source version、transform version、Pack ID/hash 和 parameters hash。
- [x] 失败批次无部分 final、无 staging residue，且不改变已有 selected。
- [x] 上传、AI 与 Pack default 继续共享同一 candidate contract。
- [x] Single generation 只接受显式 selected 的精确版本。
- [x] Adapter/Feature/composition 集成和 workspace 回归通过。

## Excluded

- Resource Workbench、图片预览、覆盖/确认 UI。
- ItemDefinition resource binding 与 definition-hash-bound generation。
- 新完整候选、安装器、真实图片 Provider 质量和真实游戏验收。

## Completion Gate

针对性测试及 workspace test/check/clippy、frontend test/type/build、DAG、fmt/diff 全部通过后归档并按用户持续授权自动提交。

## Verification

- `cargo test --workspace --all-targets`: 126 passed。
- `cargo check --workspace --all-targets`: passed。
- `cargo clippy --workspace --all-targets -- -D warnings`: passed。
- `cargo check -p agentthespire-desktop --features ml-rembg`: passed。
- `cargo check -p agentthespire-desktop --features e2e`: passed。
- `npm run test:frontend`: 15 passed；仅有已占用 Vite HMR 端口的非失败警告。
- `npx tsc -b --pretty false`、`npm run build`: passed。
- Stage 2 DAG self/real、CLI catalog、rustfmt check、`git diff --check`: passed。
- 未执行 Resource Workbench 人工体验、真实图片 Provider、完整 release candidate、安装器或真实游戏验收；均属于后续 Order/独立授权门禁。
