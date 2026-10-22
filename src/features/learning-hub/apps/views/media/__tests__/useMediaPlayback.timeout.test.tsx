import React from 'react';
import { act, cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { MEDIA_LOAD_TIMEOUT_MS, useMediaPlayback } from '../useMediaPlayback';

function Player({ src = 'test.mp4', active = true, onError }: { src?: string; active?: boolean; onError: (reason?: string) => void }) {
  const playback = useMediaPlayback<HTMLVideoElement>({ src, isActive: active, onError });
  return <video ref={playback.mediaRef} data-ready={String(playback.isReady)} />;
}

describe('media playback deadlines', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => undefined);
    vi.spyOn(HTMLMediaElement.prototype, 'load').mockImplementation(() => undefined);
  });
  afterEach(() => { cleanup(); vi.useRealTimers(); vi.restoreAllMocks(); });

  it('terminates a load that never emits metadata or error, once per source', () => {
    const onError = vi.fn();
    const { rerender } = render(<Player onError={onError} />);
    act(() => vi.advanceTimersByTime(MEDIA_LOAD_TIMEOUT_MS + 1_000));
    expect(onError).toHaveBeenCalledWith('timeout');
    act(() => vi.advanceTimersByTime(MEDIA_LOAD_TIMEOUT_MS));
    expect(onError).toHaveBeenCalledTimes(1);
    rerender(<Player src="retry.mp4" onError={onError} />);
    act(() => vi.advanceTimersByTime(MEDIA_LOAD_TIMEOUT_MS + 1_000));
    expect(onError).toHaveBeenCalledTimes(2);
  });

  it('keeps metadata and progress when hidden, and pauses the deadline', () => {
    const onError = vi.fn();
    const { container, rerender } = render(<Player onError={onError} />);
    const video = container.querySelector('video')!;
    Object.defineProperty(video, 'readyState', { configurable: true, value: 4 });
    fireEvent.loadedMetadata(video);
    expect(video.dataset.ready).toBe('true');
    rerender(<Player active={false} onError={onError} />);
    act(() => vi.advanceTimersByTime(MEDIA_LOAD_TIMEOUT_MS * 2));
    expect(video.dataset.ready).toBe('true');
    expect(onError).not.toHaveBeenCalled();
  });

  it('repeated waiting events cannot postpone a stalled-playback failure forever', () => {
    const onError = vi.fn();
    const { container } = render(<Player onError={onError} />);
    const video = container.querySelector('video')!;
    Object.defineProperty(video, 'readyState', { configurable: true, value: 2 });
    Object.defineProperty(video, 'paused', { configurable: true, value: false });
    for (let i = 0; i < 4; i += 1) {
      fireEvent.waiting(video);
      act(() => vi.advanceTimersByTime(10_000));
    }
    expect(onError).toHaveBeenCalledTimes(1);
    expect(onError).toHaveBeenCalledWith('timeout');
  });
});
