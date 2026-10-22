# PR #490 / 题组支持 / 媒体播放修复：本地验证记录

## 范围与状态

本次实施包含导入完整性、题组/共享材料全链路、Bilibili 代理与有限等待状态。修改保留在工作树，没有提交、推送、合并或触发 GitHub CI。本记录不包含用户材料、数据库、原始日志或媒体签名地址。

接口和迁移说明：

- `docs/dev/qbank-import-integrity.md`
- `docs/dev/qbank-question-groups.md`
- `docs/dev/qbank-bundle-v2.md`
- `docs/dev/media-learning/README.md` 第 6 节

## 环境与复现规则

本机 Node 22.23.3 / npm 10.9.9 / Rust 1.99.0；CI 固定 Rust 1.98.1，并使用 nextest。本机没有 nextest，以下结果是本地实际验证，不是远端 CI 同构结论。

- 前端复用既有 node_modules，不重新安装或升级依赖。
- Cargo 在 `src-tauri/` 执行，统一串行调度，`CARGO_BUILD_JOBS=2`。
- 桌面默认特性复用现有 target；不要无故设置 RUSTFLAGS / CARGO_TARGET_DIR / CARGO_PROFILE_* 改变缓存指纹。
- 前端完整测试使用 forks 和 `--maxWorkers=2`；不要 singleFork，也不要把 watch 入口当门禁。
- 所有数据库/图片测试仅使用合成 fixture、临时目录及迁移后的隔离数据库。
- lib 测试的编译、运行与集成测试 target 分开记录；`--no-run` 不是测试通过。

## 实际结果

| 验证 | 结果 |
| --- | --- |
| 完整 Vitest | 1,551 文件通过，12,345 tests passed；4 skipped / 5 todo |
| 双类型检查 | tsc 与 tsgo 均通过 |
| 完整 Rust lib | 6,887 passed / 0 failed / 9 ignored；四线程，395.95s |
| `question_group_contract_tests` | 6/6，通过约束、FTS 与完整性可见性 |
| `question_bundle_v2_tests` | 17/17，真实迁移库/repo 与图片字节往返 |
| `sync_weird_tests` | 70/70，通过 |
| 正式迁移 gate | 194 passed，`--min-tests 50` 达标；包含历史 fixture 经生产 coordinator 升级 |
| 迁移静态/脚本 | 131 迁移文件锁检查通过；50 脚本测试通过 |
| 构建/权限/Android 脚本契约 | 120/120，通过 |
| Clippy | `--all-targets -- -D clippy::correctness` 通过，仍有 warning |
| rustfmt / diff whitespace / licenses | 通过 |
| Android | aarch64 / API21 / release / `mobile-slim` cargo check 通过；不代表设备运行 |
| 前端生产构建 | 在全功能测试绿灯后使用独立输出目录构建成功 |
| 构建启动检查 | 生产入口执行，React 替换 boot placeholder，通过 |

### 性能用例

`w69_ten_thousand_tiny_records` 在没有 Cargo/Vitest/其它测试并发时运行 3 次：18.258832080s、18.419315679s、18.794380218s，均通过原始 `<30s` 门槛。没有修改断言或 nextest 独占配置。

main 与 PR #490 原基线的同步路径和依赖没有源码差异；本机重复通过不证明远端 30.20s 的原因已消失，不把重跑变绿当根因修复。

### 备份与两路流转

- JSON v2 往返包含真实 base64 图片字节、hash/size 验证、删除记录、ID 重映射及学习计数；失败整包回滚。
- RowSync 回归真实调用 `apply_downloaded_changes`，覆盖 child-first 跨批暂存/drain、缺成员不可见、显式墓碑后维护 manifest。组采用整行 LWW，避免 version 与 manifest 被逐字段拼接。
- 全量备份用例 `data_governance::backup::tests::full_backup_restore_roundtrips_question_group_material_blob` 真实执行 `backup_with_assets` 和 `restore_with_assets(..., true)`；恢复后的题组关系、材料图片 hash/字节与 FTS 均通过。

## 本次发现并修正的测试陷阱

1. 新备份测试的 `--exact` 初次少了 `data_governance::` 前缀，返回 0 tests。检查 summary 后立即识别，没有按绿灯计入；先 `--list` 确认再实际执行。
2. `VfsDatabase::new` 本身不等于完整生产迁移 fixture。备份 fixture 改为复用 `setup_migrated_test_db` 的真实迁移结果，而非手工补表。
3. `BackupManager::restore()` 仅恢复数据库。要验证图片闭包必须调用 `restore_with_assets(..., true)`，且 manager 的 app root 与 slots/slotA 不能混淆。
4. external-content FTS 更新要用 OLD 字段的特殊 delete 命令。普通 DELETE 会保留旧 token；新增更新/软删/恢复/硬删和 integrity-check 回归。
5. mockito 可复用端口，进程缓存即使在串行测试也可能跨用例命中。Bilibili fixture 改为独立 bvid，不用全局清缓存影响其它并发测试。
6. 带签名 query 的 mock 必须匹配 query；签名仍保留在合成请求中，用于验证 trace 不泄漏。
7. 题组 `_with_conn` 多语句写入必须由真实事务承载；删除前 CAS，单删子题同事务维护 manifest。随机、统计与 AI 路径不能另造绕过组可见性规则的 SQL。
8. shell 管道可能吞掉 Cargo 退出码。最终验证保留原退出码和完整本地日志，不以 tail/grep 的 0 退出码报通过。

## 构建警告与验收边界

生产构建成功；Vite 仍有大 chunk 提示。按 CI 同样的 `check-bundle-size --warn-only` 模式，关键入口/vendor 预算通过，但总 JS gzip 约 10,250.4 KiB，超过历史 8,560.6 KiB 限额（比历史 baseline +23.3%）。本轮未放宽预算，也未在没有对照构建证据时把增长全部归因于本次改动。

没有执行 Android 真机首帧/seek/网络切换、已安装应用 GUI、真实供应商模型调用或用户文档导入验收。没有构建/安装 deb 或 APK。Android cleartext/CSP/Wry/证书策略未改变。

四平台真实安装包升级 E2E 的豁免在 2026-10-18 到期，未擅自延期；该发布门禁不能由本地库测试替代。新 schema 不承诺旧客户端原地可写或无损降级。#492 的真机现象不能仅凭合成验证宣称已验收关闭。
