# 圆桌协议 v2 (Roundtable Protocol)

> 基于文件读写与 JSON 状态机的多代理讨论系统。
> 任何代理只需阅读本指南，即可无缝加入讨论、或作为主持人全自动推进会议。

---

## 目录结构

```
.roundtable/
├── GUIDE.md                ← 本文件（协议规范）
└── {会议名}/                ← 每场会议一个独立目录
    ├── AGENDA.md            # 会议背景、议题与角色设定
    ├── turn.json            # 核心状态机（严格的 JSON 格式）
    ├── 001_{角色}.md         # 只读的发言流（按时间戳/序号递增）
    ├── 002_{角色}.md
    ├── ...
    └── MINUTES.md           # 最终会议纪要（由主持人生成）
```

---

## 核心状态机 (turn.json)

`turn.json` 是整个系统的唯一可变状态源，严格遵循以下结构：

```json
{
  "conference": "会议名称",
  "status": "open",               // open | concluding | closed
  "round": 1,                     // 当前轮次
  "max_rounds": 3,                // 最大轮次限制
  "speaker_order": ["architect", "reviewer", "security"], // 发言顺序
  "current_speaker_index": 0,     // 当前该谁发言 (指向 speaker_order 的索引)
  "current_speaker": "architect", // 当前发言角色名
  "prompt_for_speaker": "请阐述你对缓存架构的初始设计方案" // 给当前角色的行动指令/上下文提示
}
```

---

## 代理行动指南

任何被唤起的代理，请严格按以下步骤执行：

### 第一步：定位与验明正身
1. 读取 `.roundtable/{会议名}/turn.json`。
2. 检查 `status`：
   - 若为 `closed`：回复用户“会议已结束，请查阅 MINUTES.md”。终止操作。
   - 若为 `concluding`：如果你的角色是**主持人 (moderator)**，执行第四步（总结）。否则回复用户等待主持人总结。
3. 检查 `current_speaker` 是否为你当前被分配的角色：
   - 若**不是**：回复用户 `[会议阻塞] 当前轮到 {current_speaker} 发言，请调度对应代理。`（如果系统支持唤起子代理，请直接通过 Task/Agent 工具唤起下一个角色）。

### 第二步：上下文拉取（增量阅读）
为了避免上下文爆炸，采用以下阅读策略：
1. **必读**：`AGENDA.md`（掌握全局背景）
2. **必读**：`turn.json` 中的 `prompt_for_speaker`（了解别人对你的期望）
3. **选读**：读取 `.roundtable/{会议名}/` 下最新的 2-3 份发言文件。若需要更早的历史，再往前追溯。

### 第三步：生成发言并推进状态 (Atomic Commit)
1. **撰写发言**：创建文件 `{三位序号}_{你的角色}.md`（如 `003_reviewer.md`）。
   - 格式建议包含：`## 立场`、`## 分析论证`、`## 回应/提问`。
2. **计算下一个状态**：
   - `next_index` = `current_speaker_index + 1`
   - 若 `next_index` >= `speaker_order.length`，则本轮结束：`next_index = 0`，`round = round + 1`
3. **判断终局条件**：
   - 若 `round > max_rounds`，或者大家已达成共识，则将 `status` 设为 `concluding`，并将 `current_speaker` 设为 `moderator`（主持人）。
4. **更新状态机**：覆写 `turn.json`。
   - 更新 `current_speaker_index` 和 `current_speaker`。
   - 在 `prompt_for_speaker` 中简短写下你对下一个角色的期望或抛出的问题（例如："我已提出性能担忧，请 security 角色评估该方案的攻击面"）。

*(注意：步骤1和步骤4必须在你的同一次代码执行流中完成。)*

### 第四步：主持人总结 (Moderator 专属)
当 `turn.json` 状态为 `concluding`，且当前角色为 `moderator`（通常是发起讨论的初始代理或用户）时：
1. 完整梳理所有 `NNN_*.md` 文件。
2. 生成 `MINUTES.md`，包含：**讨论摘要、最终共识、悬而未决的分歧、决议行动项**。
3. 将 `turn.json` 的 `status` 改为 `closed`。

---

## 自动化接力扩展 (无人值守模式)

如果代理运行在支持工具调用（如 Cursor Task Tool / Sub-agent）的环境中，**你无需等待用户手动切换**。

在更新完 `turn.json` 后，如果 `status` 仍为 `open`，**当前代理应直接使用 Task/Agent 工具唤起下一个角色**：
- **Prompt**: `"请阅读 .roundtable/GUIDE.md 并以 {current_speaker} 的身份加入 {会议名} 会议。你的指示是: {prompt_for_speaker}"`
- 这样可以通过代理间的递归或链式调用，一键完成所有轮次的讨论，最终直接向用户交付 `MINUTES.md`。

---

## 发起一场新会议
请代理执行以下操作：
1. 创建 `.roundtable/{会议名}/` 目录。
2. 写入 `AGENDA.md`（定义背景和角色）。
3. 写入初始的 `turn.json`。
4. 作为第一个发言者或触发第一个发言者。