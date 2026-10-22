/**
 * #492 前端播放状态：纯模型
 * - 准备 / 加载 / 缓冲 / playing / stalled / failed / cancelled 的阶段推进；
 * - 首帧预算与「无进展」预算分开：首帧超时 → failed，播放中无进展 → stalled（有界，不无限转圈）；
 * - 暂停 / seek / 页面隐藏不误判失败；取消与新旧 session 隔离；
 * - 脱敏 trace：只记会话、阶段、结构化原因与 CDN 主机，不记签名 URL / Cookie / 标题 / 用户身份。
 */
import { describe, expect, it } from 'vitest';

import {
  DEFAULT_MEDIA_LOAD_BUDGETS,
  MEDIA_BILIBILI_STREAM_EVENT,
  MEDIA_TRACE_MAX_ENTRIES,
  createMediaLoadState,
  createMediaSessionId,
  createMediaTraceSession,
  isMediaLoadBlocked,
  isMediaLoadBusy,
  mediaErrorReason,
  mediaLoadFailureKey,
  normalizeBilibiliStreamEvent,
  reduceMediaLoadEvent,
  sanitizeTraceHost,
  sanitizeTraceReason,
  type MediaLoadEvent,
  type MediaLoadState,
} from '../mediaLoadState';

const SESSION = '0123456789abcdef';
const BUDGETS = { firstFrameMs: 100, noProgressMs: 50 };

function run(events: MediaLoadEvent[], initial?: MediaLoadState) {
  const start = createMediaLoadState(SESSION, 0);
  return events.reduce((state, event) => reduceMediaLoadEvent(state, event, BUDGETS), initial ?? start);
}

const start = (at = 0, sessionId = SESSION): MediaLoadEvent => ({ type: 'start', at, sessionId });
const tick = (at: number): MediaLoadEvent => ({ type: 'tick', at });
const playing = (at: number): MediaLoadEvent => ({ type: 'playing', at });

describe('media load phase machine', () => {
  it('walks prepare → loading → buffering → playing and marks the first frame', () => {
    let state = run([start(0)]);
    expect(state.phase).toBe('preparing');
    expect(isMediaLoadBusy(state)).toBe(true);

    state = reduceMediaLoadEvent(state, { type: 'loaded-metadata', at: 10 }, BUDGETS);
    expect(state.phase).toBe('loading');

    state = reduceMediaLoadEvent(state, { type: 'canplay', at: 20 }, BUDGETS);
    expect(state.phase).toBe('buffering');

    state = reduceMediaLoadEvent(state, { type: 'playing', at: 30 }, BUDGETS);
    expect(state.phase).toBe('playing');
    expect(state.hasFirstFrame).toBe(true);
    expect(isMediaLoadBusy(state)).toBe(false);
    expect(isMediaLoadBlocked(state)).toBe(false);
    expect(state.reason).toBeNull();
    expect(mediaLoadFailureKey(state)).toBeNull();
  });

  it('fails once the first-frame budget is exhausted instead of spinning forever', () => {
    let state = run([start(0), { type: 'loaded-metadata', at: 10 }]);
    state = reduceMediaLoadEvent(state, tick(99), BUDGETS);
    expect(state.phase).toBe('loading');

    state = reduceMediaLoadEvent(state, tick(100), BUDGETS);
    expect(state.phase).toBe('failed');
    expect(state.reason).toBe('first-frame-timeout');
    expect(state.retryable).toBe(true);
    expect(isMediaLoadBusy(state)).toBe(false);
    expect(isMediaLoadBlocked(state)).toBe(true);
    expect(mediaLoadFailureKey(state)).toBe('learningHub:mediaPreview.loadFailed.firstFrame');

    // 失败后不自行恢复，也不重复失败事件
    const again = reduceMediaLoadEvent(state, tick(10_000), BUDGETS);
    expect(again).toBe(state);
  });

  it('marks a stalled playback after the no-progress budget (not a hard failure)', () => {
    let state = run([start(0), playing(5)]);
    expect(state.hasFirstFrame).toBe(true);

    state = reduceMediaLoadEvent(state, tick(54), BUDGETS);
    expect(state.phase).toBe('playing');

    state = reduceMediaLoadEvent(state, tick(55), BUDGETS);
    expect(state.phase).toBe('stalled');
    expect(state.reason).toBe('no-progress');
    expect(isMediaLoadBusy(state)).toBe(false);
    expect(isMediaLoadBlocked(state)).toBe(true);
    expect(mediaLoadFailureKey(state)).toBe('learningHub:mediaPreview.loadFailed.noProgress');

    // 恢复进度 → 回到 playing
    state = reduceMediaLoadEvent(state, { type: 'progress', at: 56, positionSec: 12 }, BUDGETS);
    expect(state.phase).toBe('playing');
    expect(state.reason).toBeNull();
  });

  it('does not count time while the user paused', () => {
    let state = run([start(0), playing(5), { type: 'pause', at: 6 }]);
    expect(state.phase).toBe('paused');
    expect(state.pausedByUser).toBe(true);

    state = reduceMediaLoadEvent(state, tick(10_000), BUDGETS);
    expect(state.phase).toBe('paused');
    expect(mediaLoadFailureKey(state)).toBeNull();

    state = reduceMediaLoadEvent(state, { type: 'play', at: 10_000 }, BUDGETS);
    expect(state.phase).toBe('buffering');
    expect(state.pausedByUser).toBe(false);
    // 重新播放后预算重新起算
    state = reduceMediaLoadEvent(state, tick(10_040), BUDGETS);
    expect(state.phase).toBe('buffering');
  });

  it('does not fail while the tab is hidden, and restarts the budget after it becomes visible', () => {
    let state = run([start(0), playing(5), { type: 'hidden', at: 6 }]);
    expect(state.hidden).toBe(true);

    state = reduceMediaLoadEvent(state, tick(30_000), BUDGETS);
    expect(state.phase).toBe('playing');
    expect(mediaLoadFailureKey(state)).toBeNull();

    state = reduceMediaLoadEvent(state, { type: 'visible', at: 30_000 }, BUDGETS);
    expect(state.hidden).toBe(false);
    state = reduceMediaLoadEvent(state, tick(30_040), BUDGETS);
    expect(state.phase).toBe('playing');
    state = reduceMediaLoadEvent(state, tick(30_051), BUDGETS);
    expect(state.phase).toBe('stalled');
  });

  it('treats a user seek as expected silence, not a stall', () => {
    let state = run([start(0), playing(5), { type: 'seeking', at: 6 }]);
    expect(state.seeking).toBe(true);

    state = reduceMediaLoadEvent(state, tick(10_000), BUDGETS);
    expect(state.phase).toBe('playing');
    expect(mediaLoadFailureKey(state)).toBeNull();

    state = reduceMediaLoadEvent(state, { type: 'seeked', at: 10_000 }, BUDGETS);
    expect(state.seeking).toBe(false);
    state = reduceMediaLoadEvent(state, tick(10_040), BUDGETS);
    expect(state.phase).toBe('playing');
    state = reduceMediaLoadEvent(state, tick(10_051), BUDGETS);
    expect(state.phase).toBe('stalled');
  });

  it('turns 15s of waiting after the first frame into stalled, and waiting before it into buffering', () => {
    let state = run([start(0), playing(5)]);
    state = reduceMediaLoadEvent(state, { type: 'waiting', at: 10 }, BUDGETS);
    expect(state.phase).toBe('buffering');
    state = reduceMediaLoadEvent(state, tick(59), BUDGETS);
    expect(state.phase).toBe('buffering');
    state = reduceMediaLoadEvent(state, tick(60), BUDGETS);
    expect(state.phase).toBe('stalled');
  });

  it('cancels explicitly, and a cancelled session never fails afterwards', () => {
    let state = run([start(0), { type: 'cancel', at: 3 }]);
    expect(state.phase).toBe('cancelled');
    expect(state.retryable).toBe(false);
    expect(isMediaLoadBusy(state)).toBe(false);

    state = reduceMediaLoadEvent(state, tick(60_000), BUDGETS);
    expect(state.phase).toBe('cancelled');
  });

  it('repeated waiting events do not extend the no-progress budget', () => {
    let state = run([start(0), { type: 'playing', at: 0, positionSec: 0 }]);
    state = reduceMediaLoadEvent(state, { type: 'waiting', at: 1 }, BUDGETS);
    for (let at = 2; at <= 45; at += 1) {
      state = reduceMediaLoadEvent(state, { type: 'waiting', at }, BUDGETS);
    }
    expect(state.phase).toBe('buffering');
    state = reduceMediaLoadEvent(state, tick(51), BUDGETS);
    expect(state.phase).toBe('stalled');
  });

  it('repeated playing events without position progress do not refresh the budget', () => {
    let state = run([start(0), { type: 'playing', at: 0, positionSec: 0 }]);
    for (const at of [10, 20, 30, 40]) {
      state = reduceMediaLoadEvent(state, { type: 'playing', at }, BUDGETS);
    }
    expect(state.phase).toBe('playing');
    state = reduceMediaLoadEvent(state, tick(51), BUDGETS);
    expect(state.phase).toBe('stalled');
  });

  it('play / wait cycles before the first frame do not extend the first-frame budget', () => {
    let state = run([start(0)]);
    state = reduceMediaLoadEvent(state, { type: 'play', at: 10 }, BUDGETS);
    state = reduceMediaLoadEvent(state, { type: 'waiting', at: 20 }, BUDGETS);
    state = reduceMediaLoadEvent(state, { type: 'play', at: 30 }, BUDGETS);
    state = reduceMediaLoadEvent(state, { type: 'waiting', at: 40 }, BUDGETS);
    state = reduceMediaLoadEvent(state, tick(99), BUDGETS);
    expect(state.phase).not.toBe('failed');
    state = reduceMediaLoadEvent(state, tick(100), BUDGETS);
    expect(state.phase).toBe('failed');
    expect(state.reason).toBe('first-frame-timeout');
  });

  it('a backward seek resets the progress baseline instead of looking stalled', () => {
    let state = run([start(0), { type: 'playing', at: 0, positionSec: 0 }]);
    state = reduceMediaLoadEvent(state, { type: 'progress', at: 5, positionSec: 500 }, BUDGETS);
    expect(state.lastPositionSec).toBe(500);
    state = reduceMediaLoadEvent(state, { type: 'seeking', at: 6 }, BUDGETS);
    state = reduceMediaLoadEvent(state, { type: 'seeked', at: 6, positionSec: 100 }, BUDGETS);
    expect(state.lastPositionSec).toBe(100);
    state = reduceMediaLoadEvent(state, { type: 'progress', at: 7, positionSec: 100.2 }, BUDGETS);
    state = reduceMediaLoadEvent(state, tick(56), BUDGETS);
    expect(state.phase).toBe('playing');
    state = reduceMediaLoadEvent(state, tick(57), BUDGETS);
    expect(state.phase).toBe('stalled');
  });

  it('ignores events that belong to an older session', () => {    const state = run([start(0)]);
    const stale = reduceMediaLoadEvent(
      state,
      { type: 'playing', at: 1, sessionId: 'ffffffffffffffff' },
      BUDGETS,
    );
    expect(stale).toBe(state);

    const failed = reduceMediaLoadEvent(
      state,
      { type: 'error', at: 2, sessionId: 'ffffffffffffffff', reason: 'network' },
      BUDGETS,
    );
    expect(failed).toBe(state);
  });
});

describe('media errors', () => {
  it('maps MediaError codes to stable structured reasons', () => {
    expect(mediaErrorReason(1)).toBe('aborted');
    expect(mediaErrorReason(2)).toBe('network');
    expect(mediaErrorReason(3)).toBe('decode');
    expect(mediaErrorReason(4)).toBe('unsupported');
    expect(mediaErrorReason(undefined)).toBe('unknown');
  });

  it('reports a media failure with the localized reason key, but ignores aborted errors', () => {
    let state = run([start(0), playing(1)]);
    state = reduceMediaLoadEvent(state, { type: 'error', at: 2, reason: 'aborted' }, BUDGETS);
    expect(state.phase).toBe('playing');

    state = reduceMediaLoadEvent(state, { type: 'error', at: 3, reason: 'network' }, BUDGETS);
    expect(state.phase).toBe('failed');
    expect(state.retryable).toBe(true);
    expect(mediaLoadFailureKey(state)).toBe('learningHub:mediaPreview.loadFailed.network');
  });

  it('exposes sane default budgets for real-device validation', () => {
    expect(DEFAULT_MEDIA_LOAD_BUDGETS.firstFrameMs).toBe(20_000);
    expect(DEFAULT_MEDIA_LOAD_BUDGETS.noProgressMs).toBe(15_000);
  });
});

describe('session ids', () => {
  it('creates 16 lowercase hex chars and differs between sessions', () => {
    const a = createMediaSessionId();
    const b = createMediaSessionId();
    expect(a).toMatch(/^[0-9a-f]{16}$/);
    expect(b).toMatch(/^[0-9a-f]{16}$/);
    expect(a).not.toBe(b);
  });
});

describe('sanitized trace', () => {
  it('keeps only the CDN host, never the signed URL', () => {
    expect(sanitizeTraceHost('https://upos-sz-mirrorcos.bilivideo.com:443/x?sign=SECRET')).toBe(
      'upos-sz-mirrorcos.bilivideo.com',
    );
    expect(sanitizeTraceHost('HTTP://user:pass@CDN.Example.com/path')).toBe('cdn.example.com');
    expect(sanitizeTraceHost('upos.example.com')).toBe('upos.example.com');
    expect(sanitizeTraceHost('')).toBeNull();
    expect(sanitizeTraceHost('not a host!')).toBeNull();
  });

  it('keeps only short structured reason codes', () => {
    expect(sanitizeTraceReason('cdn_timeout')).toBe('cdn_timeout');
    expect(sanitizeTraceReason('HTTP_412')).toBe('HTTP_412');
    expect(sanitizeTraceReason('https://x.example.com/?sign=SECRET')).toBeNull();
    expect(sanitizeTraceReason('bad reason with spaces')).toBeNull();
    expect(sanitizeTraceReason(null)).toBeNull();
  });

  it('records only whitelisted fields and sanitizes host / reason', () => {
    const entries: unknown[] = [];
    const trace = createMediaTraceSession(SESSION, (entry) => entries.push(entry));
    trace.record('connect', {
      host: 'https://upos-sz-mirrorcos.bilivideo.com/x?sign=SECRET',
      reason: 'https://evil.example.com/?cookie=1',
      bytes: 1024,
      title: '线代 第一讲',
      user: 'alice',
    } as never);

    expect(entries).toHaveLength(1);
    const entry = entries[0] as Record<string, unknown>;
    expect(entry.sessionId).toBe(SESSION);
    expect(entry.stage).toBe('connect');
    expect(entry.host).toBe('upos-sz-mirrorcos.bilivideo.com');
    expect(entry.reason).toBeNull();
    expect(entry.bytes).toBe(1024);
    expect(Object.keys(entry).sort()).toEqual(['at', 'bytes', 'elapsedMs', 'host', 'reason', 'sessionId', 'stage']);
    const json = JSON.stringify(entry);
    expect(json).not.toContain('sign');
    expect(json).not.toContain('SECRET');
    expect(json).not.toContain('线代');
    expect(json).not.toContain('alice');
  });

  it('caps trace entries so long sessions do not grow without bound', () => {
    const trace = createMediaTraceSession(SESSION, () => undefined);
    for (let i = 0; i < MEDIA_TRACE_MAX_ENTRIES + 50; i += 1) trace.record('progress');
    expect(trace.entries()).toHaveLength(MEDIA_TRACE_MAX_ENTRIES);
  });

  it('normalizes the Rust media event strictly by session', () => {
    expect(MEDIA_BILIBILI_STREAM_EVENT).toBe('media-bilibili-stream');
    const payload = {
      sessionId: SESSION,
      requestId: 7,
      stage: 'first-byte',
      code: 'cdn_timeout',
      host: 'https://upos-sz-mirrorcos.bilivideo.com:443/x?sign=SECRET',
      range: 'bytes=0-262143',
      bytes: 262144,
      elapsedMs: 1234,
    };

    const entry = normalizeBilibiliStreamEvent(payload, SESSION, 5_000);
    expect(entry).not.toBeNull();
    expect(entry).toMatchObject({
      sessionId: SESSION,
      stage: 'first-byte',
      reason: 'cdn_timeout',
      host: 'upos-sz-mirrorcos.bilivideo.com',
      bytes: 262144,
      at: 5_000,
    });
    expect(JSON.stringify(entry)).not.toContain('SECRET');

    // 旧 session / 缺失 session / 未知阶段
    expect(normalizeBilibiliStreamEvent({ ...payload, sessionId: 'ffffffffffffffff' }, SESSION)).toBeNull();
    expect(normalizeBilibiliStreamEvent({ ...payload, sessionId: undefined }, SESSION)).toBeNull();
    expect(normalizeBilibiliStreamEvent(payload, null)).toBeNull();
    expect(normalizeBilibiliStreamEvent({ ...payload, stage: 'weird-stage' }, SESSION)?.stage).toBe('unknown');
    expect(normalizeBilibiliStreamEvent({ ...payload, range: 'bytes=secret' }, SESSION)?.range).toBeNull();
    expect(normalizeBilibiliStreamEvent('nope', SESSION)).toBeNull();
  });
});
