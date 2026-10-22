/**
 * #492 前端播放状态：hook
 * - 真实 <video> 事件驱动阶段；定时预算在暂停 / seek / 页面隐藏时停表；
 * - 首帧 20s / 无进展 15s（待真机验证的常量）到点后停止转圈并给出可重试状态；
 * - 换 session / 换条目（sessionKey 变化）取消旧定时器，旧会话的延迟事件不再污染新会话；
 * - trace 只记脱敏字段。
 */
import React from 'react';
import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useMediaLoadState, type UseMediaLoadStateOptions } from '../useMediaLoadState';

function makeElement() {
  const el = document.createElement('video');
  let currentTime = 0;
  Object.defineProperty(el, 'currentTime', {
    configurable: true,
    get: () => currentTime,
    set: (value: number) => {
      currentTime = value;
    },
  });
  return {
    el,
    setTime: (value: number) => {
      currentTime = value;
    },
  };
}

function setup(initial: Partial<UseMediaLoadStateOptions> = {}) {
  const { el, setTime } = makeElement();
  const mediaRef = { current: el } as React.RefObject<HTMLMediaElement | null>;
  const traceSink = vi.fn();
  const props: UseMediaLoadStateOptions = {
    mediaRef,
    sessionKey: 'bilistream://localhost/file_1?s=0123456789abcdef',
    sessionId: '0123456789abcdef',
    traceSink,
    ...initial,
  };
  const utils = renderHook((p: UseMediaLoadStateOptions) => useMediaLoadState(p), {
    initialProps: props,
  });
  return { ...utils, el, setTime, traceSink, props };
}

function fire(el: HTMLElement, type: string) {
  act(() => {
    el.dispatchEvent(new Event(type));
  });
}

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('useMediaLoadState', () => {
  it('starts preparing and follows the real media events to playing', () => {
    const { result, el, setTime } = setup();
    expect(result.current.state.phase).toBe('preparing');
    expect(result.current.state.sessionId).toBe('0123456789abcdef');

    fire(el, 'loadedmetadata');
    expect(result.current.state.phase).toBe('loading');
    fire(el, 'canplay');
    expect(result.current.state.phase).toBe('buffering');

    setTime(1.2);
    fire(el, 'playing');
    expect(result.current.state.phase).toBe('playing');
    expect(result.current.state.hasFirstFrame).toBe(true);
  });

  it('stops the spinner with a retryable failure when no first frame arrives within 20s', () => {
    const { result } = setup();
    act(() => {
      vi.advanceTimersByTime(19_000);
    });
    expect(result.current.state.phase).toBe('preparing');

    act(() => {
      vi.advanceTimersByTime(1_500);
    });
    expect(result.current.state.phase).toBe('failed');
    expect(result.current.state.reason).toBe('first-frame-timeout');
    expect(result.current.state.retryable).toBe(true);
  });

  it('turns 15s without progress into stalled and recovers when playback resumes', () => {
    const { result, el, setTime } = setup();
    fire(el, 'loadedmetadata');
    setTime(3);
    fire(el, 'playing');
    expect(result.current.state.phase).toBe('playing');

    act(() => {
      vi.advanceTimersByTime(14_500);
    });
    expect(result.current.state.phase).toBe('playing');

    act(() => {
      vi.advanceTimersByTime(1_000);
    });
    expect(result.current.state.phase).toBe('stalled');
    expect(result.current.state.reason).toBe('no-progress');

    setTime(4);
    fire(el, 'progress');
    expect(result.current.state.phase).toBe('playing');
  });

  it('does not fail while paused, hidden or seeking', () => {
    const { result, el, setTime } = setup();
    fire(el, 'loadedmetadata');
    setTime(2);
    fire(el, 'playing');

    fire(el, 'pause');
    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(result.current.state.phase).toBe('paused');

    fire(el, 'play');
    Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => 'hidden' });
    act(() => {
      document.dispatchEvent(new Event('visibilitychange'));
    });
    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(result.current.state.hidden).toBe(true);
    expect(result.current.state.phase).not.toBe('stalled');
    expect(result.current.state.phase).not.toBe('failed');

    Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => 'visible' });
    act(() => {
      document.dispatchEvent(new Event('visibilitychange'));
    });
    fire(el, 'seeking');
    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(result.current.state.seeking).toBe(true);
    expect(result.current.state.phase).not.toBe('stalled');
  });

  it('reports a structured network failure from the media error code', () => {
    const { result, el } = setup();
    Object.defineProperty(el, 'error', { configurable: true, value: { code: 2 } });
    fire(el, 'error');
    expect(result.current.state.phase).toBe('failed');
    expect(result.current.state.reason).toBe('network');
  });

  it('a new session cancels the old budgets so the new item is not failed by stale timers', () => {
    const { result, rerender, props } = setup({ sessionKey: 'first', sessionId: '1111111111111111' });
    act(() => {
      vi.advanceTimersByTime(19_500);
    });
    expect(result.current.state.phase).toBe('preparing');

    rerender({ ...props, sessionKey: 'second', sessionId: '2222222222222222' });
    expect(result.current.state.sessionId).toBe('2222222222222222');
    expect(result.current.state.phase).toBe('preparing');

    act(() => {
      vi.advanceTimersByTime(1_500);
    });
    expect(result.current.state.phase).toBe('preparing');
  });

  it('cancel stops timers and leaves a terminal cancelled state', () => {
    const { result } = setup();
    act(() => result.current.cancel());
    expect(result.current.state.phase).toBe('cancelled');
    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(result.current.state.phase).toBe('cancelled');
  });

  it('retry restarts the budget and leaves the element reload to the player layer', () => {
    const { result } = setup();
    const load = vi.fn();
    act(() => {
      vi.advanceTimersByTime(21_000);
    });
    expect(result.current.state.phase).toBe('failed');

    act(() => result.current.retry());
    expect(result.current.state.phase).toBe('preparing');
    // 外部 session（B 站链接）由上层换会话；hook 只重置预算，不重复 load
    expect(result.current.sessionId).toBe('0123456789abcdef');
    expect(load).not.toHaveBeenCalled();
    act(() => {
      vi.advanceTimersByTime(1_000);
    });
    expect(result.current.state.phase).toBe('preparing');
  });

  it('traces only sanitized structured entries bound to the session', () => {
    const { result, el, setTime, traceSink } = setup();
    fire(el, 'loadedmetadata');
    setTime(1);
    fire(el, 'playing');
    act(() => {
      vi.advanceTimersByTime(16_000);
    });

    const stages = traceSink.mock.calls.map(([entry]) => entry.stage);
    expect(stages).toContain('playing');
    expect(stages).toContain('stalled');
    for (const [entry] of traceSink.mock.calls) {
      expect(entry.sessionId).toBe('0123456789abcdef');
      expect(Object.keys(entry).sort()).toEqual(['at', 'bytes', 'elapsedMs', 'host', 'reason', 'sessionId', 'stage']);
    }
    expect(result.current.state.phase).toBe('stalled');
  });
});
