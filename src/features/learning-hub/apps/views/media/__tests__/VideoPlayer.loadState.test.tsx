/**
 * #492 VideoPlayer 有界加载 UI：
 * - 失败 / 停滞时停止转圈，显示结构化原因与重试按钮；
 * - 重试回到准备态并重新 load() 当前源（不自动切换任何播放器）；
 * - 首帧预算到点后不再无限 spinner。
 */
import React from 'react';
import { act, cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { VideoPlayer } from '../VideoPlayer';

function getVideo(container: HTMLElement): HTMLVideoElement {
  const el = container.querySelector('video');
  if (!el) throw new Error('video element missing');
  return el as HTMLVideoElement;
}

/** jsdom 的 video.textTracks 不是真正的事件目标；给 VideoPlayer 的字幕避让逻辑一个可用桩 */
class FakeTextTracks extends EventTarget {
  length = 0;
  [Symbol.iterator]() {
    return [][Symbol.iterator]();
  }
}

const originalTextTracks = Object.getOwnPropertyDescriptor(HTMLMediaElement.prototype, 'textTracks');

beforeEach(() => {
  vi.useFakeTimers();
  Object.defineProperty(HTMLMediaElement.prototype, 'textTracks', {
    configurable: true,
    get() {
      const el = this as HTMLMediaElement & { __fakeTextTracks?: FakeTextTracks };
      if (!el.__fakeTextTracks) el.__fakeTextTracks = new FakeTextTracks();
      return el.__fakeTextTracks;
    },
  });
});
afterEach(() => {
  vi.useRealTimers();
  cleanup();
  if (originalTextTracks) {
    Object.defineProperty(HTMLMediaElement.prototype, 'textTracks', originalTextTracks);
  } else {
    delete (HTMLMediaElement.prototype as unknown as Record<string, unknown>).textTracks;
  }
});

describe('VideoPlayer bounded load state', () => {
  it('shows a spinner while loading, then a retryable failure instead of spinning forever', () => {
    const { container } = render(<VideoPlayer src="bilistream://localhost/file_1?qn=80" fileName="a.mp4" onError={vi.fn()} />);
    const el = getVideo(container);

    expect(container.querySelector('[data-media-load-state="preparing"]')).not.toBeNull();
    expect(container.querySelector('.animate-spin')).not.toBeNull();
    expect(container.querySelector('[data-media-load-failed]')).toBeNull();

    // 首帧预算到点：失败可见且转圈停止
    act(() => {
      vi.advanceTimersByTime(21_000);
    });
    expect(container.querySelector('[data-media-load-failed]')).not.toBeNull();
    expect(container.querySelector('[data-media-load-reason="first-frame-timeout"]')).not.toBeNull();
    expect(container.querySelector('.animate-spin')).toBeNull();
    expect(container.querySelector('[data-media-load-retry]')).not.toBeNull();

    // 重试：回到准备态并重新请求当前源
    const load = vi.fn();
    Object.defineProperty(el, 'load', { configurable: true, value: load });
    fireEvent.click(container.querySelector('[data-media-load-retry]') as HTMLElement);
    expect(load).toHaveBeenCalledTimes(1);
    expect(container.querySelector('[data-media-load-state="preparing"]')).not.toBeNull();
    expect(container.querySelector('[data-media-load-failed]')).toBeNull();
  });

  it('maps a media error to a structured failure and reports it to the parent', () => {
    const onError = vi.fn();
    const onLoadStateChange = vi.fn();
    const { container } = render(
      <VideoPlayer src="filestream://x" fileName="a.mp4" onError={onError} onLoadStateChange={onLoadStateChange} />,
    );
    const el = getVideo(container);
    Object.defineProperty(el, 'error', { configurable: true, value: { code: 2 } });

    fireEvent(el, new Event('error'));

    expect(onError).toHaveBeenCalledTimes(1);
    expect(container.querySelector('[data-media-load-failed]')).not.toBeNull();
    expect(container.querySelector('[data-media-load-reason="network"]')).not.toBeNull();
    expect(container.querySelector('.animate-spin')).toBeNull();
    expect(onLoadStateChange).toHaveBeenLastCalledWith(
      expect.objectContaining({ phase: 'failed', reason: 'network' }),
    );
  });

  it('marks stalled playback after 15s without progress', () => {
    const { container } = render(<VideoPlayer src="filestream://x" fileName="a.mp4" onError={vi.fn()} />);
    const el = getVideo(container);
    let currentTime = 0;
    Object.defineProperty(el, 'currentTime', {
      configurable: true,
      get: () => currentTime,
      set: (value: number) => {
        currentTime = value;
      },
    });

    fireEvent(el, new Event('loadedmetadata'));
    currentTime = 4;
    fireEvent(el, new Event('playing'));

    act(() => {
      vi.advanceTimersByTime(14_000);
    });
    expect(container.querySelector('[data-media-load-stalled]')).toBeNull();

    act(() => {
      vi.advanceTimersByTime(1_500);
    });
    expect(container.querySelector('[data-media-load-stalled]')).not.toBeNull();
    expect(container.querySelector('[data-media-load-reason="no-progress"]')).not.toBeNull();
    expect(container.querySelector('.animate-spin')).toBeNull();
  });
});
