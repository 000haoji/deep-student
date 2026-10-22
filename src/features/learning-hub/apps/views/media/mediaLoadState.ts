/**
 * mediaLoadState — 应用内播放的有界加载状态（#492）
 *
 * 目的：把「永久转圈」换成可解释、可重试的有界状态。
 * - 阶段：准备 preparing → 加载 loading → 缓冲 buffering → 播放 playing；
 *   播放中长时间没有进展 → stalled；失败 failed；主动取消 / 卸载 → cancelled。
 * - 首帧预算（默认 20s，待真机验证）耗尽 → failed(first-frame-timeout)；
 *   播放中无进展预算（默认 15s，待真机验证）耗尽 → stalled(no-progress)。
 * - 用户暂停 / 主动 seek / 页面隐藏期间不计预算，避免误判失败。
 * - 换条目 / 换清晰度 / 取消会新建 session；旧 session 的延迟事件一律忽略。
 *
 * 本模块是纯函数层，DOM 事件接线见 useMediaLoadState.ts。
 * trace 只记录会话、阶段、结构化原因码与 CDN 主机；不记录签名 URL、Cookie、
 * 视频标题或用户身份，录制前统一脱敏。
 */
import { debugMasterSwitch } from '@/debug-panel/debugMasterSwitch';

/** 首帧等待预算（待真机验证的常量，不是已达成的指标） */
export const MEDIA_FIRST_FRAME_BUDGET_MS = 20_000;
/** 播放中「无进展」预算（待真机验证的常量） */
export const MEDIA_NO_PROGRESS_BUDGET_MS = 15_000;
/** 预算检查间隔 */
export const MEDIA_LOAD_TICK_MS = 500;

export type MediaLoadPhase =
  | 'preparing'
  | 'loading'
  | 'buffering'
  | 'playing'
  | 'paused'
  | 'stalled'
  | 'failed'
  | 'cancelled';

export type MediaLoadFailureReason =
  | 'first-frame-timeout'
  | 'no-progress'
  | 'network'
  | 'decode'
  | 'unsupported'
  | 'aborted'
  | 'cancelled'
  | 'unknown';

export interface MediaLoadBudgets {
  firstFrameMs: number;
  noProgressMs: number;
}

export const DEFAULT_MEDIA_LOAD_BUDGETS: MediaLoadBudgets = {
  firstFrameMs: MEDIA_FIRST_FRAME_BUDGET_MS,
  noProgressMs: MEDIA_NO_PROGRESS_BUDGET_MS,
};

export interface MediaLoadState {
  sessionId: string;
  phase: MediaLoadPhase;
  /** 本次会话开始时间（首帧预算起点） */
  startedAt: number;
  /** 最近一次真实进展（playing / progress / seek / 恢复可见 / 重新播放） */
  lastProgressAt: number;
  lastPositionSec: number;
  hasFirstFrame: boolean;
  pausedByUser: boolean;
  hidden: boolean;
  seeking: boolean;
  reason: MediaLoadFailureReason | null;
  retryable: boolean;
}

export type MediaLoadEvent =
  | { type: 'start'; at: number; sessionId: string }
  | { type: 'loaded-metadata'; at: number; sessionId?: string }
  | { type: 'canplay'; at: number; sessionId?: string }
  | { type: 'playing'; at: number; sessionId?: string; positionSec?: number }
  | { type: 'waiting'; at: number; sessionId?: string }
  | { type: 'progress'; at: number; sessionId?: string; positionSec: number }
  | { type: 'pause'; at: number; sessionId?: string }
  | { type: 'play'; at: number; sessionId?: string }
  | { type: 'seeking'; at: number; sessionId?: string }
  | { type: 'seeked'; at: number; sessionId?: string; positionSec?: number }
  | { type: 'hidden'; at: number; sessionId?: string }
  | { type: 'visible'; at: number; sessionId?: string }
  | { type: 'tick'; at: number; sessionId?: string }
  | { type: 'error'; at: number; sessionId?: string; reason?: MediaLoadFailureReason }
  | { type: 'cancel'; at: number; sessionId?: string };

export function createMediaLoadState(sessionId: string, at: number): MediaLoadState {
  return {
    sessionId,
    phase: 'preparing',
    startedAt: at,
    lastProgressAt: at,
    lastPositionSec: 0,
    hasFirstFrame: false,
    pausedByUser: false,
    hidden: false,
    seeking: false,
    reason: null,
    retryable: false,
  };
}

/** 最近一次进展是否真的推进了播放位置（timeupdate 在 stalled 时也可能重复同一秒） */
const POSITION_EPSILON = 1e-3;

export function reduceMediaLoadEvent(
  state: MediaLoadState,
  event: MediaLoadEvent,
  budgets: MediaLoadBudgets = DEFAULT_MEDIA_LOAD_BUDGETS,
): MediaLoadState {
  if (event.type === 'start') {
    return createMediaLoadState(event.sessionId, event.at);
  }
  // 旧 session 的延迟事件不得污染新会话
  if (event.sessionId && event.sessionId !== state.sessionId) return state;
  // failed / cancelled 是终止态，只有 start（重试 / 换会话）能离开
  if (state.phase === 'failed' || state.phase === 'cancelled') return state;

  switch (event.type) {
    case 'loaded-metadata':
      return state.phase === 'preparing' ? { ...state, phase: 'loading' } : state;
    case 'canplay':
      return state.hasFirstFrame || state.pausedByUser ? state : { ...state, phase: 'buffering' };
    case 'waiting':
      if (!state.hasFirstFrame) return state;
      // 只有首次进入等待才算「等到此刻为止有进展」；重复 waiting 不得续命预算
      return state.phase === 'buffering'
        ? state
        : { ...state, phase: 'buffering', lastProgressAt: event.at };
    case 'playing': {
      const position = typeof event.positionSec === 'number' ? event.positionSec : null;
      const changed =
        position === null ? false : Math.abs(position - state.lastPositionSec) > POSITION_EPSILON;
      // 重复 playing 事件（尤其没有位置推进）不能无条件刷新首帧 / 进展预算
      const progressed = !state.hasFirstFrame || changed;
      return {
        ...state,
        phase: 'playing',
        hasFirstFrame: true,
        pausedByUser: false,
        lastPositionSec: changed && position !== null ? position : state.lastPositionSec,
        lastProgressAt: progressed ? event.at : state.lastProgressAt,
        reason: null,
        retryable: false,
      };
    }
    case 'progress': {
      // 位置变化（前进或后退 / seek）都算真实进展，避免回退后低位置一直被视为「无进展」
      const changed = Math.abs(event.positionSec - state.lastPositionSec) > POSITION_EPSILON;
      const next: MediaLoadState = {
        ...state,
        lastPositionSec: changed ? event.positionSec : state.lastPositionSec,
        lastProgressAt: changed ? event.at : state.lastProgressAt,
      };
      return changed && next.phase === 'stalled'
        ? { ...next, phase: 'playing', reason: null, retryable: false }
        : next;
    }
    case 'pause':
      return {
        ...state,
        pausedByUser: true,
        phase:
          state.phase === 'playing' || state.phase === 'buffering' || state.phase === 'stalled'
            ? 'paused'
            : state.phase,
        reason: null,
        retryable: false,
      };
    case 'play':
      return {
        ...state,
        pausedByUser: false,
        phase: state.hasFirstFrame ? 'buffering' : 'loading',
        lastProgressAt: event.at,
        reason: null,
        retryable: false,
      };
    case 'seeking':
      return { ...state, seeking: true, lastProgressAt: event.at };
    case 'seeked':
      // seek 后重置进展基线：回退 seek 时新位置低于旧位置，不能因此被当成「无进展」
      return {
        ...state,
        seeking: false,
        lastProgressAt: event.at,
        lastPositionSec:
          typeof event.positionSec === 'number' ? event.positionSec : 0,
      };
    case 'hidden':
      return { ...state, hidden: true };
    case 'visible':
      return {
        ...state,
        hidden: false,
        lastProgressAt: event.at,
        startedAt: state.hasFirstFrame ? state.startedAt : event.at,
      };
    case 'cancel':
      return { ...state, phase: 'cancelled', retryable: false };
    case 'error': {
      const reason = event.reason ?? 'unknown';
      // 被 pause / 切源打断的 play() abort 不是真正的失败
      if (reason === 'aborted') return state;
      return { ...state, phase: 'failed', reason, retryable: reason !== 'cancelled' };
    }
    case 'tick': {
      if (state.pausedByUser || state.hidden || state.seeking) {
        // 停表：暂停 / 隐藏 / seek 的时长不计入预算
        return {
          ...state,
          lastProgressAt: event.at,
          startedAt: state.hasFirstFrame ? state.startedAt : event.at,
        };
      }
      if (state.phase === 'stalled') return state;
      if (!state.hasFirstFrame) {
        if (event.at - state.startedAt >= budgets.firstFrameMs) {
          return { ...state, phase: 'failed', reason: 'first-frame-timeout', retryable: true };
        }
        return state;
      }
      if (event.at - state.lastProgressAt >= budgets.noProgressMs) {
        return { ...state, phase: 'stalled', reason: 'no-progress', retryable: true };
      }
      return state;
    }
    default:
      return state;
  }
}

/** 是否应该显示「正在加载」转圈 */
export function isMediaLoadBusy(state: MediaLoadState): boolean {
  return state.phase === 'preparing' || state.phase === 'loading' || state.phase === 'buffering';
}

/** 是否有界停止（停滞 / 失败）：转圈必须停，改给重试入口 */
export function isMediaLoadBlocked(state: MediaLoadState): boolean {
  return state.phase === 'stalled' || state.phase === 'failed';
}

const FAILURE_KEYS: Record<MediaLoadFailureReason, string | null> = {
  'first-frame-timeout': 'learningHub:mediaPreview.loadFailed.firstFrame',
  'no-progress': 'learningHub:mediaPreview.loadFailed.noProgress',
  network: 'learningHub:mediaPreview.loadFailed.network',
  decode: 'learningHub:mediaPreview.loadFailed.decode',
  unsupported: 'learningHub:mediaPreview.loadFailed.unsupported',
  unknown: 'learningHub:mediaPreview.loadFailed.unknown',
  aborted: null,
  cancelled: null,
};

/** 失败 / 停滞对应的本地化说明 key；没有可解释原因时返回 null */
export function mediaLoadFailureKey(state: MediaLoadState): string | null {
  if (!isMediaLoadBlocked(state) || !state.reason) return null;
  return FAILURE_KEYS[state.reason];
}

/** HTMLElement MediaError.code → 稳定结构化原因 */
export function mediaErrorReason(code: number | undefined): MediaLoadFailureReason {
  switch (code) {
    case 1:
      return 'aborted';
    case 2:
      return 'network';
    case 3:
      return 'decode';
    case 4:
      return 'unsupported';
    default:
      return 'unknown';
  }
}

/** 16 位十六进制会话 id（用于 bilistream:// URL 的 s= 参数与 Rust 媒体事件关联） */
export function createMediaSessionId(): string {
  const cryptoApi = (globalThis as { crypto?: { getRandomValues?: (array: Uint8Array) => Uint8Array } }).crypto;
  if (cryptoApi?.getRandomValues) {
    const bytes = new Uint8Array(8);
    cryptoApi.getRandomValues(bytes);
    return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
  }
  let out = '';
  for (let i = 0; i < 16; i += 1) out += Math.floor(Math.random() * 16).toString(16);
  return out;
}

// ---------------------------------------------------------------- 脱敏 trace

export interface MediaTraceEntry {
  sessionId: string;
  stage: string;
  at: number;
  elapsedMs: number;
  reason: string | null;
  host: string | null;
  bytes: number | null;
}

export interface MediaTraceExtra {
  at?: number;
  reason?: string | null;
  host?: string | null;
  bytes?: number | null;
}

export type MediaTraceSink = (entry: MediaTraceEntry) => void;

/** 单会话最多保留的 trace 条数（环形覆盖，长视频不随 timeupdate 无限增长） */
export const MEDIA_TRACE_MAX_ENTRIES = 200;

const defaultTraceSink: MediaTraceSink = (entry) => {
  // 默认只在调试总开关打开时输出一行；不逐帧打印，也不进 release 日志
  try {
    if (!debugMasterSwitch.isEnabled()) return;
  } catch {
    return;
  }
  if (typeof console !== 'undefined' && typeof console.warn === 'function') {
    console.warn('[media-trace]', entry);
  }
};

const TRACE_STAGES = new Set([
  'preparing',
  'loading',
  'buffering',
  'playing',
  'paused',
  'stalled',
  'failed',
  'cancelled',
  'progress',
  'prepare',
  'resolve',
  'connect',
  'headers',
  'first-byte',
  'complete',
  'error',
  'cancel',
]);

/** 只允许短结构化原因码；URL、Cookie、自由文本一律丢弃 */
export function sanitizeTraceReason(value: string | null | undefined): string | null {
  if (typeof value !== 'string') return null;
  const token = value.trim();
  if (!/^[A-Za-z0-9_.:-]{1,64}$/.test(token)) return null;
  return token;
}

/** 只保留 CDN 主机名（小写、无协议 / 端口 / 路径 / 凭据 / 查询串） */
export function sanitizeTraceHost(value: string | null | undefined): string | null {
  if (typeof value !== 'string') return null;
  let raw = value.trim();
  if (!raw) return null;
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(raw)) {
    try {
      raw = new URL(raw).hostname;
    } catch {
      return null;
    }
  } else {
    const authority = raw.split('/')[0];
    raw = authority.includes('@') ? authority.slice(authority.lastIndexOf('@') + 1) : authority;
    raw = raw.replace(/:\d+$/, '');
  }
  const host = raw.toLowerCase();
  if (!host || host.length > 253) return null;
  if (!/^[a-z0-9](?:[a-z0-9.-]*[a-z0-9])?$/.test(host)) return null;
  return host;
}

export interface MediaTraceSession {
  sessionId: string;
  record: (stage: string, extra?: MediaTraceExtra) => MediaTraceEntry;
  entries: () => MediaTraceEntry[];
}

/**
 * 单次播放会话的脱敏 trace：只接受阶段、原因码、CDN 主机与字节数，
 * 其他字段（标题、URL、Cookie、用户身份）根本不会被读取。
 */
export function createMediaTraceSession(
  sessionId: string,
  sink: MediaTraceSink = defaultTraceSink,
): MediaTraceSession {
  const startedAt = Date.now();
  const recorded: MediaTraceEntry[] = [];
  return {
    sessionId,
    record(stage, extra = {}) {
      const at = typeof extra.at === 'number' ? extra.at : Date.now();
      const entry: MediaTraceEntry = {
        sessionId,
        stage: TRACE_STAGES.has(stage) ? stage : 'unknown',
        at,
        elapsedMs: Math.max(0, at - startedAt),
        reason: sanitizeTraceReason(extra.reason),
        host: sanitizeTraceHost(extra.host),
        bytes: typeof extra.bytes === 'number' && Number.isFinite(extra.bytes) ? extra.bytes : null,
      };
      recorded.push(entry);
      if (recorded.length > MEDIA_TRACE_MAX_ENTRIES) {
        recorded.splice(0, recorded.length - MEDIA_TRACE_MAX_ENTRIES);
      }
      sink(entry);
      return entry;
    },
    entries() {
      return [...recorded];
    },
  };
}

// ---------------------------------------------------------------- Rust 媒体事件

/** Rust 侧 `emit_to` 当前 webview 的事件名（payload 见 NormalizedBilibiliStreamEvent） */
export const MEDIA_BILIBILI_STREAM_EVENT = 'media-bilibili-stream';

const RUST_STAGES = new Set(['prepare', 'resolve', 'connect', 'headers', 'first-byte', 'complete', 'error', 'cancel']);

export interface NormalizedBilibiliStreamEvent {
  sessionId: string;
  stage: string;
  at: number;
  elapsedMs: number;
  /** 结构化原因码（Rust 的 code），脱敏后仅保留 token */
  reason: string | null;
  host: string | null;
  bytes: number | null;
  range: string | null;
}

/**
 * 严格按 session 过滤 Rust 媒体事件：payload.sessionId 必须与当前播放会话一致，
 * 否则返回 null（旧条目 / 旧清晰度的迟到事件不得显示到新会话上）。
 * 注意 complete / first-byte 只是字节层事实，不等于首帧或 playing。
 */
export function normalizeBilibiliStreamEvent(
  raw: unknown,
  sessionId: string | null,
  at = Date.now(),
): NormalizedBilibiliStreamEvent | null {
  if (!sessionId || !raw || typeof raw !== 'object') return null;
  const payload = raw as Record<string, unknown>;
  if (typeof payload.sessionId !== 'string' || payload.sessionId !== sessionId) return null;

  const rawStage = typeof payload.stage === 'string' ? payload.stage : '';
  const range =
    typeof payload.range === 'string' && /^bytes=\d*-\d*$/.test(payload.range) ? payload.range : null;

  return {
    sessionId,
    stage: RUST_STAGES.has(rawStage) ? rawStage : 'unknown',
    at,
    elapsedMs:
      typeof payload.elapsedMs === 'number' && Number.isFinite(payload.elapsedMs) ? payload.elapsedMs : 0,
    reason: sanitizeTraceReason(typeof payload.code === 'string' ? payload.code : null),
    host: sanitizeTraceHost(typeof payload.host === 'string' ? payload.host : null),
    bytes: typeof payload.bytes === 'number' && Number.isFinite(payload.bytes) ? payload.bytes : null,
    range,
  };
}
