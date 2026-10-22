# PR #490：空题目集首导与 CI 回归修复

## 问题与修复

新建题目集使用 `status=empty`，上传组件保留该题目集 ID 发起首次导入。导入服务将带 ID 的请求统一作为 AppendDocument 校验，但新增白名单遗漏 empty，导致实际安装包在解析或模型请求前返回 `Validation: 会话状态 empty 不允许追加文档`。

修复仅允许未删除、没有未完成 checkpoint 的 empty 会话追加首个文档。Resume 白名单不变，删除、未完成断点、lease、CAS 与来源归属保护不变；前端不丢弃 session_id，不另建题目集。

回归覆盖：

- empty 无断点允许 AppendDocument；
- empty 有断点仍拒绝追加；
- 已删除 empty 拒绝追加；
- empty 有无断点均不允许 Resume；
- 真实 repo 创建 empty 题目集后，经真实 JSON 导入入口复用原 ID，确认一个题目集、两道合成题，不请求模型。

## GitHub CI 的另外三条失败

原 head `81f12066` 的三个 Rust 分片失败均为代码问题，不按上游门禁忽略：

1. 新增 `question_group_materials_fts` 未登记同步分类，真实 schema 覆盖与 checksum 口径两条集成测试失败。已登记为 `(virtual)` / `DerivedRebuild`，FTS 及其影子表不作为权威同步行或 checksum 表。
2. FSRS 部分迁移恢复先执行了依赖缺失列的索引。Refinery 只在 `Runner::run` 内排序，`get_migrations()` 本身保留未保证有序的构造列表；兼容恢复已显式按版本升序。新增强制版本降序 runner 的真实 mistakes 迁移回归，验证补列、索引重建及 history 记账；不修改任何已发布 SQL 或锁文件。

## 本轮验证原则

以原 GitHub 失败与实际应用错误日志作为缺陷证据，不重复执行修复前本地复现。修复后执行完整本地 CI 适用测试，覆盖 Rust lib/bins 及 84 个适用集成目标，而非仅 lib 和少数选定目标。

本地使用已有 Cargo 和合成隔离 fixture；没有 nextest，不宣称 Rust 工具链与 GitHub 完全同构。CI 明确排除的外部执行器 E2E 与独立迁移 fixture gate 不冒称通过。w69 性能用例单独运行，原阈值不变。用户数据库、原始文档、设备及模型供应商均不作为测试输入。

本地完整验证通过后允许提交并构建，不等待新 head GitHub 检查结束。Android 使用完整默认功能与本地开发签名，不等同于 mobile-slim cargo check、真机验收或 release 发布门禁。

## 实际本地验证结果

- 完整 Rust lib/bins 与 84 个 CI 适用集成目标：7,889 passed / 0 failed / 75 ignored；其中 lib 6,893 passed / 9 ignored。75 ignored 为既有联网/外部环境等用例，不声称已执行。feature-gated 零用例目标亦不作为测试覆盖计数。
- 新首导真实路径、empty 安全矩阵、强制逆序迁移恢复、原 FSRS 失败用例、同步 schema 11 项均通过。
- `sync_scenarios_tests` 72/72。首次完整运行暴露 mock 云实例标识跨测试复用，墓碑发布序号在默认状态库持久化；已给 mock 每实例唯一标识，并把所有 Rust runner 的 XDG data/config/cache/state 环境指向各自临时目录，完整复跑通过。不读取、清理或删除既有用户状态。
- w69 性能用例独占执行 19.95s，通过原 `<30s` 门槛。
- 完整前端 12,345 passed / 4 skipped / 5 todo，1,551 文件通过。
- tsc/tsgo、ESLint（0 error，warning保留）、licenses、Clippy all-targets correctness、rustfmt、diff whitespace 均通过。
- 脚本契约 120/120、迁移脚本 14/14、迁移静态 131 文件、生产迁移 195 项、已有合成 fixture 真实升级 gate 均通过。

本机没有 Docker、cargo-nextest 或 Windows runner；Cloud Provider Docker 契约、Windows 专用测试与真实安装升级 E2E 未在本机执行，不冒称已验收。未下载新工具或修改过期发布豁免。

新安装包元数据由本轮交付清单记录；本文不预先声明尚未完成的构建成功。
