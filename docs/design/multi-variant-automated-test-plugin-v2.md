# 多变体全自动测试插件设计文档

> 版本: v2.0 | 日期: 2026-02-16
> 基准: 每次发送固定 3 个并行变体（Model A / Model B / Model C）
> 模板: attachmentPipelineTestPlugin.ts + chatInteractionTestPlugin.ts

---

## 1. 设计目标与覆盖范围

### 1.1 核心原则

- **3 变体为基准**：比 2 变体暴露更多并发问题
- **DOM 找不到 = 步骤失败**：绝不使用 store 操作作为 DOM 的「备用路径」，避免假阳性
- **长 prompt 保障时间窗口**：取消/打断测试必须使用 longPrompt 确保流式足够长
- **事件驱动等待**：骨架验证用 `variant_start` Tauri 事件确认时机，不依赖 `sessionStatus` 竞态
- **五维验证**：Store 状态 / 请求体 / 持久化 / DOM 渲染 / Icon 完整性

### 1.2 诚实的覆盖范围声明

**本测试覆盖**：
- `pendingParallelModelIds` → `TauriAdapter.buildSendOptions()` → 后端多变体 pipeline 的完整链路
- 多变体消息创建后的全部 UI 交互：取消 / 重试 / 切换 / 删除
- 后端持久化完整性、骨架消息、DOM 渲染正确性、Model Icon 完整性

**本测试不覆盖**（需人工测试或单元测试）：
- chip 面板选择模型 → `setPendingParallelModelIds` 的前端触发路径
  - 原因：chip 选中状态是 InputBarV2 内部 React useState，外部无法访问
  - 参考：chatInteractionTestPlugin step 7 使用相同的 monkey-patch 例外
- `useInputBarV2.sendMessage()` 中的 `multiModelSelectEnabled` Feature Flag 检查
- `downgradeInjectModesForNonMultimodal` 降级逻辑（由附件流水线测试覆盖）

---

## 2. 架构

沿用项目已有的双层架构（与 chatInteractionTestPlugin 完全一致）：

```
src/
├── chat-v2/debug/
│   └── multiVariantTestPlugin.ts        # 核心逻辑（纯函数，无 React/DOM 组件）
└── debug-panel/plugins/
    └── MultiVariantTestPlugin.tsx        # UI 组件（配置/进度/结果）
```

核心逻辑通过回调 `onLog` / `onStepComplete` 与 UI 层通信。

### 2.1 复用 chatInteractionTestPlugin 的基础设施

以下模块直接复用，不重新实现：

| 模块 | 来源 | 用途 |
|------|------|------|
| `createLogger` | chatInteractionTestPlugin | 每步独立日志 + CustomEvent 广播 |
| `createConsoleCapture` | chatInteractionTestPlugin | monkey-patch console 捕获管线日志 |
| `createChatV2LogCapture` | chatInteractionTestPlugin | 捕获 ChatV2 事件 |
| `createRequestBodyCapture` | chatInteractionTestPlugin | 监听 `chat_v2_llm_request_body` |
| `simulateTyping` | chatInteractionTestPlugin | React textarea nativeInputValueSetter |
| `clickSend` | chatInteractionTestPlugin | 点击 `[data-testid="btn-send"]` + disabled 等待 |
| `clickStop` | chatInteractionTestPlugin | 点击 `[data-testid="btn-stop"]` |
| `createAndSwitchSession` | chatInteractionTestPlugin | 新建会话 + 切换 + 等待 InputBarUI 就绪 |
| `waitFor` / `sleep` | chatInteractionTestPlugin | 轮询等待 |
| `waitForStreaming` / `waitForIdle` | chatInteractionTestPlugin | 流式状态等待 |
| `getLastMessageId` | chatInteractionTestPlugin | 获取最后一条指定角色消息 |
| `verifyPersistence` | chatInteractionTestPlugin | invoke load_session 校验 |
| `checkModelIcon` / `checkDomModelIcon` | chatInteractionTestPlugin | Model Icon 验证 |
| `sanitizeRequestBody` | chatInteractionTestPlugin | 请求体脱敏 dump |
| `withAutoConfirm` | chatInteractionTestPlugin | window.confirm 自动确认 |
| `startStepCaptures` / `stopStepCaptures` | chatInteractionTestPlugin | 每步启停捕获 |
| `finalizeChecks` / `makeStepResult` | chatInteractionTestPlugin | 统一验证汇总 |

### 2.2 新增模块

| 模块 | 用途 |
|------|------|
| `sendMultiVariant` | 封装 monkey-patch + store.setState + simulateTyping + clickSend 的完整发送流程 |
| `findVariantButton` | 在 `[data-variant-index=N]` 卡片内按 i18n title 定位取消/重试/删除按钮 |
| `clickVariantButton` | scrollIntoView + findVariantButton + click（失败 = 抛错，不降级） |
| `clickIndicatorDot` | 点击第 N 个指示器圆点 |
| `clickNavArrow` | 点击左/右导航箭头 |
| `createVariantEventCapture` | 监听 `chat_v2_event_{sessionId}` 解析 variant_start/variant_end |
| `waitAllVariantsDone` | sessionStatus=idle AND streamingVariantIds.size=0 |
| `waitVariantStatus` | 等待特定变体到达目标状态 |
| `takeDomSnapshot` | 扫描卡片/指示器/箭头/Icon 的完整 DOM 状态 |
| `takeStoreSnapshot` | 拍摄 Store 中变体相关状态 |

---

## 3. 配置

```typescript
interface MultiVariantTestConfig {
  modelA: string;         // 模型 A（必须不同供应商以验证 Icon）
  modelB: string;         // 模型 B
  modelC: string;         // 模型 C
  prompt: string;         // 短 prompt（默认: "你好，请用一句话自我介绍。"）
  longPrompt: string;     // 长 prompt，确保流式 ≥5s（默认: "请写一篇 800 字关于人工智能发展历史的文章，从 1950 年图灵测试讲起，包含每个十年的关键里程碑、代表性人物和技术突破，最后展望未来。"）
  cancelDelayMs: number;  // 取消等待时间（默认 3000ms）
  fastCancelDelayMs: number; // 快速取消（默认 800ms）
  roundTimeoutMs: number; // 单轮超时（默认 120000ms）
  intervalMs: number;     // 步骤间隔（默认 3000ms）
  skipSteps: StepName[];
}
```

**longPrompt 的重要性**：所有取消/打断测试必须使用 longPrompt。如果使用短 prompt，LLM 可能在 cancel 到达前已完成响应，导致 cancel 测试永真（假阴性）。longPrompt 至少要让每个变体流式持续 5 秒以上。

---

## 4. 测试步骤总览（5 组，18 个步骤）

### Group A — 发送与取消（3 步，独立会话）

基础验证：多变体能发送、能取消。

| # | 步骤 | 操作 | 核心验证 |
|---|------|------|---------|
| 1 | `mv_send_3` | 3 模型发送(prompt) → 等完成 | 3 变体 success，3 请求体，DOM 3 卡片 |
| 2 | `mv_cancel_middle` | 3 模型发送(longPrompt) → 取消 B → 等 A/C 完成 | B=cancelled, A/C=success, 无僵尸 |
| 3 | `mv_cancel_all` | 3 模型发送(longPrompt) → 依次取消 A/B/C | 3 cancelled, idle, 10s 内回到 idle |

### Group B — 重试与恢复（3 步，独立会话）

基础验证：取消后能重试。

| # | 步骤 | 操作 | 核心验证 |
|---|------|------|---------|
| 4 | `mv_retry_one` | 发送 → 取消 B → 等完成 → DOM 重试 B | B: cancelled→success, A/C 不变 |
| 5 | `mv_retry_all` | 发送 → 取消全部 → store.retryAllVariants [例外] | 3 → success, ≥3 新请求体 |
| 6 | `mv_fast_cancel_retry` | 发送 → 800ms 取消 A → 等 cancelled → DOM 重试 A → 等完成 | 无僵尸流，A 最终 success |

### Group C — 切换与删除（4 步，共享会话）

验证：变体间切换和逐个删除。

| # | 步骤 | 操作 | 核心验证 |
|---|------|------|---------|
| 7 | `mv_switch_setup` | 3 模型发送(prompt) → 等完成 | 前置：创建 3 个 success 变体 |
| 8 | `mv_switch_nav` | 点击右箭头 → 验证 → 再右箭头 → 验证 → 左箭头 | activeVariantId 跟随, DOM dot 位置 |
| 9 | `mv_delete_one` | DOM 删除非 active 变体 | variants 3→2, active 不变, blocks 清理 |
| 10 | `mv_delete_to_single` | DOM 删除再一个 → 只剩 1 | isMultiVariant=false, 指示器消失 |

### Group D — 中间状态打断（5 步，每步独立会话）

验证：各种「不等完成就操作」的边缘场景。

| # | 步骤 | 操作 | 边缘焦点 |
|---|------|------|---------|
| 11 | `mv_cancel_first` | 发送 → 取消 A → 等 B/C 完成 | 取消 index=0 不影响其余 |
| 12 | `mv_cancel_last` | 发送 → 取消 C → 等 A/B 完成 | 取消 index=2（边界） |
| 13 | `mv_cancel_two` | 发送 → 取消 A → 取消 B → 等 C 完成 | 连续取消 2 个 |
| 14 | `mv_cancel_then_delete` | 发送 → 取消 B → 等 cancelled → DOM 删除 B → 等完成 | 取消后立即删除 |
| 15 | `mv_switch_during_stream` | 发送 → 流式中点击指示器切换到 C → 等完成 | streaming 中切换不中断流式 |

### Group E — 持久化与 DOM（3 步，每步独立会话）

| # | 步骤 | 操作 | 焦点 |
|---|------|------|------|
| 16 | `mv_persist_complete` | 发送 → 完成 → load_session | DB variants/blocks/active_variant_id |
| 17 | `mv_skeleton_check` | 发送 → 等 variant_start 事件 → 立即 load_session | 骨架消息 variants 非空 |
| 18 | `mv_icon_and_dom` | 3 供应商模型发送 → 完成 → DOM 全检 | 卡片/指示器/箭头/Icon |

---

## 5. 每步详细设计

### 5.1 通用模式：多变体发送

每次需要发送多变体消息时，调用 `sendMultiVariant` 封装函数：

```typescript
/**
 * 封装多变体发送的完整流程。
 *
 * ★ 已记录的例外：
 *   chip 面板选中状态是 InputBarV2 内部 React useState，外部无法访问。
 *   handleSendMessage 会调用 setPendingParallelModelIds(null) 覆盖我们的值。
 *   解决方案：临时拦截 setPendingParallelModelIds 的 null 写入。
 *   与 chatInteractionTestPlugin step 7 使用完全相同的技术。
 */
async function sendMultiVariant(
  store: StoreApi<ChatStore>,
  modelIds: string[],
  prompt: string,
  log: LogFn,
): Promise<void> {
  // 1. monkey-patch（与 chatInteractionTestPlugin step 7 完全一致）
  const origSetPending = store.getState().setPendingParallelModelIds;
  (store as any).setState({
    setPendingParallelModelIds: (ids: string[] | null) => {
      if (ids === null) {
        log('info', 'model', 'setPendingParallelModelIds(null) 已拦截');
        return;
      }
      origSetPending(ids);
    },
  });
  (store as any).setState({ pendingParallelModelIds: modelIds });
  log('info', 'model', `设置并行模型: ${modelIds.join(', ')} (monkey-patch 激活)`);

  // 2. 输入文字（真实路径）
  if (!simulateTyping(prompt)) throw new Error('无法输入文字');
  await sleep(500);

  // 3. 点击发送（真实路径）
  if (!await clickSend(log)) {
    await sleep(1000);
    if (!await clickSend(log)) throw new Error('发送按钮不可用');
  }

  // 4. 等待流式开始
  if (!await waitForStreaming(store, 15000)) throw new Error('多变体流式未开始');

  // 5. 恢复 monkey-patch（adapter 已读取 pendingParallelModelIds）
  (store as any).setState({ setPendingParallelModelIds: origSetPending });
  log('success', 'send', '多变体发送完成，monkey-patch 已恢复');
}
```

### 5.2 通用模式：变体卡片内按钮操作

```typescript
/**
 * 在变体卡片内定位并点击按钮。
 *
 * ★ 关键原则：找不到 = 步骤失败，绝不降级到 store 操作。
 * 使用 i18n title 定位（与 chatInteractionTestPlugin 的 clickButtonByTitle 一致）。
 */
async function clickVariantButton(
  variantIndex: number,
  action: 'cancel' | 'retry' | 'delete',
  log: LogFn,
): Promise<void> {
  // 1. 滚动到卡片
  const card = document.querySelector(`[data-variant-index="${variantIndex}"]`);
  if (!card) throw new Error(`变体卡片[${variantIndex}]未找到 — DOM 未渲染`);
  card.scrollIntoView({ behavior: 'instant', inline: 'center' });
  await sleep(300);

  // 2. 按 i18n title 在卡片内找按钮
  const titleMap = {
    cancel: getI18nTitle('chatV2:variant.cancel'),
    retry: getI18nTitle('chatV2:variant.retry'),
    delete: getI18nTitle('chatV2:variant.delete'),
  };
  const title = titleMap[action];

  const btn = card.querySelector<HTMLButtonElement>(
    `button[title="${title}"], button[aria-label="${title}"]`
  );

  if (!btn) throw new Error(`变体[${variantIndex}] ${action} 按钮未找到 (title="${title}") — 可能按钮未渲染`);
  if (btn.disabled) throw new Error(`变体[${variantIndex}] ${action} 按钮已禁用`);

  btn.click();
  log('success', 'dom', `变体[${variantIndex}] ${action} 已点击`);
}
```

### 5.3 通用模式：导航按钮点击

```typescript
function clickNavArrow(direction: 'prev' | 'next'): boolean {
  const label = direction === 'prev' ? 'Previous variant' : 'Next variant';
  const btn = document.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`);
  if (!btn || btn.disabled) return false;
  btn.click();
  return true;
}

function clickIndicatorDot(index: number): boolean {
  const dots = document.querySelectorAll('.variant-indicator-dot, .variant-indicator-dot-active');
  if (index >= dots.length) return false;
  (dots[index] as HTMLElement).click();
  return true;
}
```

### 5.4 通用模式：骨架验证（事件驱动）

```typescript
/**
 * ★ 修复竞态：用 variant_start Tauri 事件确认流式真正开始，
 *   而非依赖 sessionStatus（前端状态可能延迟）。
 *   骨架消息在 variant_start 之前写入 DB，因此收到第一个
 *   variant_start 后立即 load_session 一定能读到骨架。
 */
async function verifySkeletonDuringStream(
  sessionId: string,
  variantEventCapture: VariantEventCapture,
  log: LogFn,
): Promise<VerificationCheck[]> {
  const checks: VerificationCheck[] = [];

  // 等待第一个 variant_start 事件（确认后端已写入骨架）
  const gotVariantStart = await waitFor(
    () => variantEventCapture.events.some(e => e.type === 'variant_start'),
    15000, 200
  );
  checks.push({
    name: '收到 variant_start 事件',
    passed: gotVariantStart,
    detail: gotVariantStart
      ? `✓ 收到 ${variantEventCapture.events.length} 个事件`
      : '❌ 15s 内未收到 variant_start',
  });

  if (!gotVariantStart) return checks;

  // 立即读取 DB（此时骨架一定已写入）
  const { invoke } = await import('@tauri-apps/api/core');
  const data = await invoke<{ messages?: Array<{ id: string; role: string; variants?: unknown[] }> }>(
    'chat_v2_load_session', { sessionId }
  );
  const messages = data?.messages || [];
  const assistant = messages.find(m => m.role === 'assistant');

  checks.push({
    name: '骨架助手消息存在',
    passed: !!assistant,
    detail: assistant ? `✓ id=${assistant.id}` : '❌ DB 中无助手消息',
  });

  if (assistant) {
    const variants = assistant.variants as Array<{ id: string; modelId?: string }> | undefined;
    checks.push({
      name: '骨架包含 variants',
      passed: !!variants && variants.length >= 2,
      detail: variants
        ? `✓ ${variants.length} 个变体: ${variants.map(v => v.modelId || v.id).join(', ')}`
        : '❌ variants 为空或不存在',
    });
  }

  return checks;
}
```

---

## 6. Group A — 发送与取消

### 步骤 1: `mv_send_3`

```
前置: 新建会话
操作:
  1. sendMultiVariant(store, [A,B,C], prompt)   [含 monkey-patch 例外]
  2. waitAllVariantsDone(store, roundTimeoutMs)

验证:
  [Store] variants.length===3, 每个 status==='success', blockIds 非空
  [Store] sessionStatus==='idle', streamingVariantIds.size===0
  [请求体] ≥3 个, 模型集合包含 A/B/C
  [持久化] load_session → variants.length===3
  [DOM] [data-variant-index]===3, indicator dots===3, active dot===1
  [Icon] 每个变体 detectProviderBrand !== 'generic'
```

### 步骤 2: `mv_cancel_middle`

```
前置: 新建会话
操作:
  1. sendMultiVariant(store, [A,B,C], longPrompt)  ★ 必须用 longPrompt
  2. sleep(cancelDelayMs)                           等待流式进行
  3. clickVariantButton(1, 'cancel')                ★ DOM 点击，找不到=失败
  4. waitVariantStatus(B, ['cancelled'], 10000)     ★ 10s 内必须 cancelled
  5. waitAllVariantsDone(store, roundTimeoutMs)

验证:
  [Store] A=success, B=cancelled, C=success
  [Store] idle, 无僵尸
  [时间约束] cancel 后 10s 内 B 变为 cancelled（不接受「已经 success 所以永真」）
  [DOM] B 卡片有重试按钮, 无取消按钮
```

**关于时间约束**：如果 B 在 cancelDelayMs 前已经完成（模型太快），步骤不会失败——但验证会标记 `B.status === 'success'`，说明 longPrompt 不够长，需要用户调整配置。这不是假阳性，而是诚实的测试结果报告。

### 步骤 3: `mv_cancel_all`

```
前置: 新建会话
操作:
  1. sendMultiVariant(store, [A,B,C], longPrompt)
  2. sleep(cancelDelayMs)
  3. clickVariantButton(0, 'cancel')     取消 A
  4. sleep(500)
  5. clickVariantButton(1, 'cancel')     取消 B
  6. sleep(500)
  7. clickVariantButton(2, 'cancel')     取消 C
  8. waitForIdle(store, 15000)           ★ 15s 内必须 idle

验证:
  [Store] 3 个变体中至少 2 个 cancelled（快速模型可能已完成）
  [Store] idle, streamingVariantIds.size===0
  [时间约束] 最后一个 cancel 后 15s 内 idle
```

---

## 7. Group B — 重试与恢复

### 步骤 4: `mv_retry_one`

```
前置: 新建会话
操作:
  1. sendMultiVariant(store, [A,B,C], longPrompt)
  2. sleep(cancelDelayMs)
  3. clickVariantButton(1, 'cancel')
  4. waitAllVariantsDone(store, roundTimeoutMs)       等 A/C 完成
  5. snapshotBefore = takeStoreSnapshot(store)
  6. clickVariantButton(1, 'retry')                   ★ DOM 重试
  7. waitAllVariantsDone(store, roundTimeoutMs)

验证:
  [Store] B.status==='success', B.blockIds 与 snapshotBefore 不同
  [Store] A/C 的 blockIds 不变（不受影响）
  [请求体] 重试后新增 1 个请求体
  [Icon] B 重试后 Icon 不为 generic
```

### 步骤 5: `mv_retry_all`

```
前置: 新建会话
操作:
  1. sendMultiVariant(store, [A,B,C], longPrompt)
  2. sleep(cancelDelayMs)
  3. 依次 clickVariantButton(0/1/2, 'cancel')
  4. waitForIdle(store, 15000)
  5. store.getState().retryAllVariants(messageId)  [例外: UI 无「全部重试」按钮]
  6. waitAllVariantsDone(store, roundTimeoutMs * 2)

验证:
  [Store] 3 个变体最终 status 包含 success
  [请求体] 重试后 ≥3 个新请求体

例外说明: retryAllVariants 在 ParallelVariantView 中通过 onRetryAllVariants
  prop 传递，但没有独立的 DOM 按钮暴露给用户。它在菜单内部可能有入口，
  但菜单的 DOM 结构（AppMenu/Radix Popover）难以可靠自动化点击。
  此处使用 store 调用是唯一可接受的例外。
```

### 步骤 6: `mv_fast_cancel_retry`

```
前置: 新建会话
操作:
  1. sendMultiVariant(store, [A,B,C], longPrompt)
  2. sleep(fastCancelDelayMs)                        ★ 800ms, 比标准短
  3. clickVariantButton(0, 'cancel')
  4. waitVariantStatus(A, ['cancelled'], 10000)
  5. clickVariantButton(0, 'retry')                  ★ 快速重试
  6. waitAllVariantsDone(store, roundTimeoutMs)

验证:
  [Store] A 最终 success, B/C 也 success
  [Store] streamingVariantIds.size===0（无僵尸双重流）
  [请求体] A 的重试产生新请求体
```

---

## 8. Group C — 切换与删除（共享会话）

### 步骤 7: `mv_switch_setup` (前置)

```
操作: sendMultiVariant(store, [A,B,C], prompt) → waitAllVariantsDone
验证: 3 success（后续步骤 8-10 的前置条件）
```

### 步骤 8: `mv_switch_nav`

```
操作:
  1. 记录 activeVariantId (应为第 1 个)
  2. clickNavArrow('next')                ★ DOM 右箭头
  3. 等 200ms → 验证 activeVariantId 变为第 2 个
  4. clickNavArrow('next')
  5. 等 200ms → 验证 activeVariantId 变为第 3 个
  6. 验证 Next 箭头 disabled
  7. clickNavArrow('prev')
  8. 等 200ms → 验证 activeVariantId 变为第 2 个

验证:
  [Store] activeVariantId 每步正确
  [DOM] active dot 位置跟随变化
  [DOM] 第 1 个时 Prev disabled, 最后 1 个时 Next disabled
  [持久化] 最后 activeVariantId 与 load_session 一致
```

### 步骤 9: `mv_delete_one`

```
前置: 步骤 8 完成, active=第 2 个变体
操作:
  1. snapshotBefore = takeStoreSnapshot(store)
  2. clickVariantButton(0, 'delete')       ★ 删除第 1 个(非 active)
  3. 等待 variants.length 变为 2

验证:
  [Store] variants.length===2, 不包含被删除的变体
  [Store] activeVariantId 不变（删除的不是 active）
  [Store] 被删除变体的 blocks 已清理
  [DOM] [data-variant-index]===2, dots===2
```

### 步骤 10: `mv_delete_to_single`

```
前置: 步骤 9 完成, 剩 2 个变体
操作:
  1. 找到非 active 变体的 index
  2. clickVariantButton(index, 'delete')
  3. 等待 variants.length 变为 1

验证:
  [Store] variants.length===1, isMultiVariantMessage===false
  [DOM] indicator dots 消失, 左右箭头消失
  [DOM] 卡片容器消失, 降级为普通单变体消息布局
```

---

## 9. Group D — 中间状态打断

**所有步骤独立会话，使用 longPrompt。**

### 步骤 11: `mv_cancel_first`

```
操作: 发送 → cancelDelayMs → clickVariantButton(0,'cancel') → 等完成
验证: A=cancelled, B/C=success, idle
特别关注: A 是第一个变体(index=0), 如果 A 是 active, cancel 后 activeVariantId 应自动切换
```

### 步骤 12: `mv_cancel_last`

```
操作: 发送 → cancelDelayMs → clickVariantButton(2,'cancel') → 等完成
验证: A/B=success, C=cancelled, idle
特别关注: C 是最后一个(index=2), 边界位置
```

### 步骤 13: `mv_cancel_two`

```
操作: 发送 → cancelDelayMs → cancel A → 500ms → cancel B → 等 C 完成
验证: A/B=cancelled, C=success, idle, streamingVariantIds.size===0
特别关注: 连续取消 2 个, 中间无 idle 间隔
```

### 步骤 14: `mv_cancel_then_delete`

```
操作: 发送 → cancelDelayMs → cancel B → waitVariantStatus(B,'cancelled') → delete B → 等完成
验证: variants.length===2 (B 被删除), A/C=success, idle
特别关注: 取消后立即删除的边界时序
```

### 步骤 15: `mv_switch_during_stream`

```
操作: 发送 → 1s → clickIndicatorDot(2) → 等完成
验证: activeVariantId===C, 3 个变体全部完成(success), 切换不中断流式
特别关注: streaming 状态下切换是纯前端乐观更新, 不应触发 cancel
```

---

## 10. Group E — 持久化与 DOM

### 步骤 16: `mv_persist_complete`

```
操作: 发送 → 完成 → sleep(1000) → invoke('chat_v2_load_session')
验证:
  - 助手消息 variants.length===3
  - 每个 variant 有 block_ids（非空）
  - active_variant_id 指向有效变体
  - 每个 variant 有 model_id 和 status
  - 用户消息也存在
```

### 步骤 17: `mv_skeleton_check`

```
操作: 发送 → verifySkeletonDuringStream (等 variant_start 事件 → load_session)
验证: (详见 5.4 通用模式)
  - 骨架助手消息存在
  - 骨架 variants 非空, length≥2
  - 每个 variant 有 id 和 model_id
```

### 步骤 18: `mv_icon_and_dom`

```
操作: 发送(3 不同供应商模型) → 完成 → takeDomSnapshot()
验证:
  [DOM]
  - [data-variant-index] === 3
  - .variant-indicator-dot + .variant-indicator-dot-active === 3
  - 恰好 1 个 active dot
  - button[aria-label="Previous variant"] 存在
  - button[aria-label="Next variant"] 存在
  - 每个卡片内 ProviderIcon <img> src 不含 'generic'/'logo.svg'

  [Icon]
  - detectProviderBrand(A) !== 'generic'
  - detectProviderBrand(B) !== 'generic'
  - detectProviderBrand(C) !== 'generic'
  - message._meta.modelId 指向 activeVariant.modelId
```

---

## 11. 捕获体系

### 11.1 三层并行捕获（与现有插件完全一致）

| 层 | 机制 | 捕获前缀 |
|---|------|---------|
| 控制台拦截 | monkey-patch console.log/warn/error/debug | `[VariantActions]`, `[ChatStore] switchVariant/deleteVariant/retryVariant/cancelVariant`, `[ChatV2::VariantHandler]`, `[ChatV2::pipeline]`, `[TauriAdapter]`, `[EventBridge]` |
| Tauri 事件 | `listen('chat_v2_llm_request_body')` | 按 `chat_v2_event_{sessionId}` 前缀过滤 |
| ChatV2 日志 | `window.addEventListener(CHATV2_LOG_EVENT)` | 按 captureStartTime 过滤异步残留 |

### 11.2 新增：variant 生命周期事件捕获

```typescript
// 监听 chat_v2_event_{sessionId} Tauri 事件
// 解析 payload 中的 type==='variant_start' / 'variant_end'
interface VariantLifecycleEvent {
  type: 'variant_start' | 'variant_end';
  variantId: string;
  modelId: string;
  status?: VariantStatus;  // variant_end 时
  timestamp: string;
}
```

### 11.3 Store 快照

每步操作前后拍摄，用于 diff 验证：

```typescript
interface StoreSnapshot {
  timestamp: string;
  sessionStatus: string;
  streamingVariantIds: string[];
  lastAssistantMessage: {
    id: string;
    variants: Array<{ id: string; modelId: string; status: string; blockIds: string[] }>;
    activeVariantId: string | undefined;
  } | null;
}
```

---

## 12. 模拟策略总表

### 完全真实 DOM 路径 ✅

| 操作 | DOM 目标 | 定位方式 | 来源 |
|------|---------|---------|------|
| 输入文字 | `textarea[data-testid="input-bar-v2-textarea"]` | data-testid | chatInteractionTestPlugin |
| 发送 | `[data-testid="btn-send"]` | data-testid | chatInteractionTestPlugin |
| 停止 | `[data-testid="btn-stop"]` | data-testid | chatInteractionTestPlugin |
| 变体取消 | 卡片内 `button[title="取消"]` | i18n title + `[data-variant-index]` | **新增** |
| 变体重试 | 卡片内 `button[title="重试"]` | i18n title + `[data-variant-index]` | **新增** |
| 变体删除 | 卡片内 `button[title="删除"]` | i18n title + `[data-variant-index]` | **新增** |
| 导航左箭头 | `button[aria-label="Previous variant"]` | aria-label | **新增** |
| 导航右箭头 | `button[aria-label="Next variant"]` | aria-label | **新增** |
| 指示器点击 | `.variant-indicator-dot` 第 N 个 | class + index | **新增** |

### 已记录例外 ⚠️

| 操作 | 方式 | 原因 | 先例 |
|------|------|------|------|
| 设置并行模型 | store.setState + monkey-patch | chip 面板选中状态是 React useState | chatInteractionTestPlugin step 7 |
| 重试所有变体 | store.getState().retryAllVariants() | 无独立 DOM 按钮，在 AppMenu 内部 | 无先例，本插件首创 |

### 绝对禁止 🚫

| 操作 | 禁止方式 | 原因 |
|------|---------|------|
| 取消变体 | ~~store.getState().cancelVariant()~~ | DOM 按钮存在，使用 store 会掩盖 UI bug |
| 重试变体 | ~~store.getState().retryVariant()~~ | DOM 按钮存在，使用 store 会掩盖 UI bug |
| 删除变体 | ~~store.getState().deleteVariant()~~ | DOM 按钮存在，使用 store 会掩盖 UI bug |
| 切换变体 | ~~store.getState().switchVariant()~~ | 指示器/箭头按钮存在，使用 store 会掩盖 UI bug |

---

## 13. 中间状态完整枚举

| 状态组合 | 场景 | 覆盖步骤 |
|---------|------|---------|
| 3 streaming | 发送后立即 | 2,3,4,5,6,11-15 |
| 1 cancelled + 2 streaming | 取消 1 个后 | 2,4,6,11,12 |
| 2 cancelled + 1 streaming | 连续取消 2 个 | 13 |
| 3 cancelled | 全部取消 | 3,5 |
| 1 cancelled + 2 success | 取消后等完成 | 2,4,11,12 |
| 2 cancelled + 1 success | 取消 2 个等完成 | 13 |
| cancelled → retry (pending) | 快速重试 | 6 |
| cancelled → delete | 取消后删除 | 14 |
| streaming 中 switch | 切换不中断 | 15 |
| 3 success | 正常完成 | 1,7,16,18 |
| 3 success → delete 1 | 删除 | 9 |
| 2 success → delete 1 (降级) | 删到 1 个 | 10 |
| streaming 中 load_session | 骨架 | 17 |

### 未覆盖（人工验证清单）

| 场景 | 原因 |
|------|------|
| 网络断开时多变体 | 无法模拟网络断开 |
| 后端 panic | 无法触发 panic |
| 模型响应 >5min | 超时限制 |

---

## 14. 全量运行器

```typescript
// 与 chatInteractionTestPlugin 完全相同的模式
async function runAllMultiVariantTests(
  config: MultiVariantTestConfig,
  onStepComplete: (result: StepResult, idx: number, total: number) => void,
  onLog: (entry: LogEntry) => void,
): Promise<StepResult[]> {
  // Group A: 3 步, 每步独立会话
  // Group B: 3 步, 每步独立会话
  // Group C: 4 步, 共享会话（步骤 7 创建, 8-10 复用）
  // Group D: 5 步, 每步独立会话
  // Group E: 3 步, 每步独立会话
}
```

关键差异：**Group C 步骤 7-10 共享会话**（因为切换和删除操作需要在已有多变体消息上执行），其余所有步骤独立会话（避免级联失败）。

---

## 15. 数据清理

```typescript
const SESSION_PREFIX = '[MultiVariantTest]';
// 完全复用 chatInteractionTestPlugin 的清理模式
```

---

## 16. 注册

```typescript
// DebugPanelHost.tsx
{
  id: 'multi-variant-test',
  labelDefault: '多变体自动化测试',
  descriptionDefault: '3 变体并行的 18 步全自动边缘测试：发送/取消/重试/切换/删除/持久化/DOM/Icon',
  Component: MultiVariantTestPlugin,
  groupId: 'chat-timeline',
}
```
