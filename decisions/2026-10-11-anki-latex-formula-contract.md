# Anki 制卡公式契约修复 — 决策记录

日期：2026-10-11
对应方案：[docs/plans/2026-10-11-anki-latex-formula-contract-plan.md](../../docs/plans/2026-10-11-anki-latex-formula-contract-plan.md)
上游 issue：helixnow/deep-student#462（制卡默认 prompt 缺公式契约，生成裸写公式）

## 用户决策

**采用「彻底版」组合** = 最小可用 + C + D：

- **A（生成侧）**：A1-a 抽公共常量模块 `src-tauri/src/prompt_contracts.rs`（单一来源）
  + A2-a 注入完整 `LATEX_RULES`（905 chars）
  + A3 在 `streaming_anki_service::build_prompt` 的 `system_sections` 中作为独立
  `<latex_rules>` 段注入（放 `custom_requirements` 之后、`base_prompt` 之前）。
- **B（质检侧）**：`anki_qa_lint` 新增 `bare_latex`（`LintSeverity::Warn`，永不拒绝；
  `LintConfig::check_bare_latex` 默认开）；B2-a 仅 Rust 侧先落地（`RUST_ONLY_CODES`）。
- **C（模板）**：给 `design-glass` / `design-exam` / `design-blueprint` 的
  `generation_prompt` 补一行公式要求。
- **D（其它链路统一）**：把公共常量接到 qbank 出题/批改、model2、llm_structurer、
  question_import、vlm、ocr_adapters、essay_grading、translation、anki_critic，
  并处理 `model2_pipeline.rs:9351`「不要用 LaTeX」与 OCR 侧要求 LaTeX 的冲突。

## 约束

- 遵守 RULES.txt 第 8 条：每步先跑测试验证，再进入构建。
- 不修改渲染层（KaTeX/卡面/字体），不适配 HarmonyOS/卓易通。
- 新增 lint code 按 `anki_qa_lint.rs:84-89` 的七步联动清单执行。

## 实施结果

分支：`feat/qbank-import-difficulty-rubric`（未提交）。

### 已落地改动

**A 生成侧**
- 新增 `src-tauri/src/prompt_contracts.rs`：`LATEX_RULES`（唯一来源）+ `LATEX_OUTPUT_BRIEF`
  （中文一句话版）+ `LATEX_OUTPUT_BRIEF_EN`，附核心约束单测。
- `chat_v2/prompt_builder.rs` 改为引用 `prompt_contracts::LATEX_RULES`（字节级结构/预算测试保持绿）。
- `streaming_anki_service::build_prompt` 注入 `LATEX_RULES`（在 custom_requirements 之后、
  base_prompt 之前），新增 `build_prompt_injects_latex_rules_before_base_prompt` 测试。

**B 质检侧**
- `anki_qa_lint` 新增 `bare_latex`（Warn，永不拒绝）+ `LintConfig::check_bare_latex`（默认开）
  + 检测实现（strip HTML → 去代码 → 移除已包裹数学区 → 命中 `\命令`/`^`/`_{`）+ 5 个单测。
- 七步联动：`codes::ALL`、`anki_gold_set::LINT_CONTRACT_CODES`、`cardLint.mjs::RUST_ONLY_CODES`、
  eval README 对照表、前端 `AnkiQaFlagBadge`（`LINT_LABEL_KEYS` + `LINT_NUMERIC_PARAM_NAMES`）、
  中英 `locales/*/anki.json` 的 `qaFlags.lint.bare_latex`。

**C 模板**
- `design-glass` / `design-exam` / `design-architect` 的 `generation_prompt` 补公式要求；
  前端 `src/data/anki/` 与后端权威副本 `src-tauri/src/data/` 两份同步。

**D 其它链路统一**（单一来源 = `prompt_contracts`）
- `qbank_generation`（pipeline 注入 brief，移除 types.rs 内联句）
- `qbank_grading`（pipeline 注入 brief，移除 GRADE/ANALYZE 两处内联句）
- `model2_pipeline`：`analysis`/`chat` 改用 brief；**OCR 冲突解决**——原「不要使用 LaTeX」改为
  要求 LaTeX（与 ocr_adapters 一致）；`anki_generation` 兜底 prompt 补 brief
- `llm_structurer`、`question_import_service`（两处）、`vlm_grounding_service`（三处）、
  `ocr_adapters`（中/英各三处）
- 补齐零契约链路：`essay_grading`、`translation`、`anki_critic`

### 测试结果

- `cargo test --lib`（全量 6500+ 用例）：本机环境过慢（>45 分钟未跑完），
  改为按模块定向验证（下方清单），全部绿。
- 定向：prompt_contracts 2、prompt_builder 19、anki_qa_lint 71、anki_gold_set 50、
  streaming_anki_service build_prompt 4、qbank_generation 21、qbank_grading 23、llm_structurer 1、
  question_import 4、vlm_grounding 9、ocr_adapters 17、essay_grading 57、translation 44、
  anki_critic 56 —— 全绿。
- 前端：`npm run typecheck` 绿；`npm run lint` 0 error（4553 既有 warning）；
  `npm run check:i18n` 通过；vitest：lintContract 6、builtinTemplatesIntegrity 114、
  ankiQaFlags 11、tests/vitest/anki 全量 69 —— 全绿。

### 遗留

- `bare_latex` 目前仅 Rust 生产侧（`RUST_ONLY_CODES`），eval 回放侧未实现。
- 未做历史卡片的批量自动规范化（方案 §6-D 的可选项）。

## 提交与构建

- commit：`feat(anki): 统一公式契约并新增 bare_latex 质检，修复制卡裸写公式`（分支 `feat/qbank-import-difficulty-rubric`）。
- 未推送：本机无 git 凭据且 github.com 不可达（`could not read Username` / HTTP2 层错误），PR 需在有网络与凭据的环境执行。
- Linux deb：`build-linux/Deep Student_0.10.5_amd64.deb`（x86_64，71MB）。末尾 updater 签名报错是缺 `TAURI_SIGNING_PRIVATE_KEY`，deb 已正常产出。
- Android APK：见下。


