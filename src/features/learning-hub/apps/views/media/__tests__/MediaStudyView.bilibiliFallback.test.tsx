/**
 * #492 应用内播放失败的用户可见出口：
 * - <video> 失败不再自动切 iframe；给出「重试 / 手动改用 B 站播放器 / 在浏览器打开」；
 * - 两次失败仍停在失败说明，不自动来回切换；
 * - 手动切到外链播放器后，document load 只算「页面已打开」，不伪造 isReady / playing；
 * - Rust 媒体事件按 session 严格匹配，旧 session 的事件不会显示在新条目上。
 */
import React, { useImperativeHandle } from 'react';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.hoisted(() => vi.fn());
const openUrlMock = vi.hoisted(() => vi.fn());
const streamListeners = vi.hoisted(() => new Map<string, (event: { payload: unknown }) => void>());

type FakeStatus = { currentTime: number; duration: number; isPlaying: boolean; isReady: boolean; documentLoaded?: boolean };
type FakeVideoProps = {
  src: string;
  sessionId?: string;
  onError: () => void;
  onStatusChange?: (s: FakeStatus) => void;
  onLoadStateChange?: (s: Record<string, unknown>) => void;
  onRetry?: () => void;
  handleRef?: React.Ref<unknown>;
  quality?: { options: Array<{ value: number; label: string }>; value: number; onChange: (v: number) => void } | null;
};
const video = vi.hoisted(() => ({
  props: null as null | FakeVideoProps,
  seekTo: vi.fn(),
  play: vi.fn(),
  pause: vi.fn(),
  mounts: 0,
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invokeMock,
  convertFileSrc: (path: string, protocol: string) => `${protocol}://localhost/${encodeURIComponent(path)}`,
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: async (name: string, handler: (event: { payload: unknown }) => void) => {
    streamListeners.set(name, handler);
    return () => streamListeners.delete(name);
  },
}));
vi.mock('react-i18next', async (importOriginal) => ({
  ...(await importOriginal<typeof import('react-i18next')>()),
  useTranslation: () => ({ t: (key: string) => key, i18n: { language: 'zh-CN' } }),
}));
vi.mock('@/components/UnifiedNotification', () => ({ showGlobalNotification: vi.fn() }));
vi.mock('@/utils/urlOpener', () => ({ openUrl: openUrlMock }));
vi.mock('../VideoPlayer', () => ({
  VideoPlayer: function FakeVideo(props: FakeVideoProps) {
    video.props = props;
    useImperativeHandle(props.handleRef, () => ({
      getElement: () => null,
      seekTo: video.seekTo,
      play: video.play,
      pause: video.pause,
    }));
    React.useEffect(() => {
      video.mounts += 1;
      props.onStatusChange?.({ currentTime: 0, duration: 0, isPlaying: false, isReady: false });
      props.onLoadStateChange?.({ ...PREPARING, sessionId: props.sessionId });
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);
    return <div data-testid="video-player" data-src={props.src} data-session={props.sessionId} />;
  },
}));
vi.mock('../AudioPlayer', () => ({ AudioPlayer: () => <div data-testid="audio-player" /> }));
vi.mock('../useMediaTranscript', () => ({
  useMediaTranscript: () => ({
    transcript: null,
    loading: false,
    error: null,
    estimate: null,
    estimating: false,
    starting: false,
    cancelling: false,
    requestEstimate: vi.fn(),
    clearEstimate: vi.fn(),
    start: vi.fn(),
    cancel: vi.fn(),
    refresh: vi.fn(),
    importFromPath: vi.fn(),
    exportToPath: vi.fn(),
  }),
}));
vi.mock('../useMediaProgressSync', () => ({
  useMediaProgressSync: () => ({ onStatus: vi.fn(), resumedFromRef: { current: null } }),
}));
vi.mock('../useTranscriptTrack', () => ({
  useTranscriptTrack: () => ({ trackSrc: null, trackRef: { current: null }, captionsOn: false, toggleCaptions: vi.fn() }),
}));
vi.mock('@/features/media-handout', () => ({ HandoutGenerateButton: () => null }));
vi.mock('@/features/learning-hub/useReferenceToChat', () => ({ useReferenceToChat: () => ({ referenceToChat: vi.fn() }) }));
vi.mock('@/features/chat/context/vfsRefApi', () => ({ uploadAttachmentBlob: vi.fn() }));
vi.mock('@/features/media-studio/mediaStudioNavigation', () => ({ openMediaStudio: vi.fn() }));

import { MediaStudyView } from '../MediaStudyView';
import { BilibiliEmbedPlayer } from '../BilibiliEmbedPlayer';
import { resetBilibiliAccountForTests } from '../bilibiliAccount';
import type { MediaPlayerHandle } from '../mediaPlayerHandle';

const PREPARING = {
  phase: 'preparing',
  startedAt: 0,
  lastProgressAt: 0,
  lastPositionSec: 0,
  hasFirstFrame: false,
  pausedByUser: false,
  hidden: false,
  seeking: false,
  reason: null,
  retryable: true,
};

const LINK = {
  kind: 'bilibili', version: 1, bvid: 'BV1xx411c7mD', aid: 1, cid: 2, page: 1, pageCount: 1,
  title: '线代', part: '', owner: null, cover: null, durationMs: 60_000, url: 'https://www.bilibili.com/video/BV1xx411c7mD',
};
const OPTIONS = [{ qn: 80, label: '高清 1080P' }, { qn: 64, label: '高清 720P' }];
const ACCOUNT = { loggedIn: false, mid: null, uname: null, face: null, vip: false, verified: true, expired: false };

function qualityBackend() {
  invokeMock.mockImplementation(async (cmd: string, args: { qn: number }) => {
    if (cmd === 'media_bilibili_auth_status') return ACCOUNT;
    if (cmd !== 'media_bilibili_stream_quality') throw new Error(`unexpected ${cmd}`);
    return { requested: args.qn, current: args.qn, options: OPTIONS };
  });
}

async function flush() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

class FakeResizeObserver {
  constructor(private cb: ResizeObserverCallback) {}
  observe() {
    this.cb([{ contentRect: { width: 1200 } } as ResizeObserverEntry], this as unknown as ResizeObserver);
  }
  disconnect() {}
  unobserve() {}
}

function renderLink() {
  return render(
    <MediaStudyView kind="video" src="" bilibili={LINK as never} resourceId="file_link" fileName="线代.bilibili" onError={vi.fn()} />,
  );
}

function failStream(reason = 'network', attempts = 1) {
  act(() => {
    for (let i = 0; i < attempts; i += 1) {
      video.props?.onLoadStateChange?.({
        ...PREPARING,
        phase: 'failed',
        reason,
        retryable: true,
        sessionId: video.props?.sessionId,
      });
    }
  });
}

function currentSession(): string | undefined {
  return video.props?.src.match(/[?&]s=([0-9a-f]{16})/)?.[1];
}

beforeEach(() => {
  vi.stubGlobal('ResizeObserver', FakeResizeObserver);
  window.localStorage.clear();
  resetBilibiliAccountForTests();
  invokeMock.mockReset();
  openUrlMock.mockReset();
  streamListeners.clear();
  video.props = null;
  video.mounts = 0;
  vi.clearAllMocks();
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe('MediaStudyView · Bilibili stream failure', () => {
  it('shows a bounded failure with retry / manual embed / external open instead of auto-switching', async () => {
    qualityBackend();
    renderLink();
    await flush();

    expect(document.querySelector('[data-bilibili-stream-error]')).toBeNull();
    failStream();
    await flush();

    const banner = document.querySelector('[data-bilibili-stream-error]');
    expect(banner).not.toBeNull();
    expect(banner?.getAttribute('data-bilibili-stream-reason')).toBe('network');
    expect(banner?.textContent).toContain('learningHub:mediaBilibili.playback.loadFailedTitle');
    expect(document.querySelector('[data-bilibili-frame]')).toBeNull();

    // 手动改用 B 站播放器（用户主动，不是自动 fallback）
    fireEvent.click(document.querySelector('[data-bilibili-action="use-embed"]')!);
    expect(document.querySelector('[data-bilibili-frame]')).not.toBeNull();
    expect(document.querySelector('[data-bilibili-stream-error]')).toBeNull();

    // 回应用内播放：换新 session 重挂载，位置由播放器恢复
    fireEvent.click(screen.getByText('learningHub:mediaBilibili.playback.retry'));
    await flush();
    expect(document.querySelector('[data-bilibili-frame]')).toBeNull();
    expect(document.querySelector('[data-testid="video-player"]')).not.toBeNull();
  });

  it('does not loop between players when the stream fails twice', async () => {
    qualityBackend();
    renderLink();
    await flush();

    failStream('network');
    await flush();
    failStream('network');
    await flush();

    expect(document.querySelector('[data-bilibili-frame]')).toBeNull();
    expect(document.querySelector('[data-bilibili-stream-error]')).not.toBeNull();
    expect(video.mounts).toBe(1);
  });

  it('retry starts a fresh session and mounts a new stream player', async () => {
    qualityBackend();
    renderLink();
    await flush();
    const firstSession = currentSession();
    expect(firstSession).toBeTruthy();
    const mounts = video.mounts;

    failStream('network');
    await flush();
    fireEvent.click(document.querySelector('[data-bilibili-action="retry-stream"]')!);
    await flush();

    const nextSession = currentSession();
    expect(nextSession).toBeTruthy();
    expect(nextSession).not.toBe(firstSession);
    expect(video.mounts).toBe(mounts + 1);
    expect(document.querySelector('[data-bilibili-stream-error]')).toBeNull();
  });

  it('opens the video in the browser only when the user asks', async () => {
    qualityBackend();
    renderLink();
    await flush();
    failStream('network');
    await flush();
    expect(openUrlMock).not.toHaveBeenCalled();
    fireEvent.click(document.querySelector('[data-bilibili-action="open-external"]')!);
    expect(openUrlMock).toHaveBeenCalledTimes(1);
    expect(String(openUrlMock.mock.calls[0][0])).toContain('bilibili.com/video/BV1xx411c7mD');
  });

  it('binds Rust media events to the current session only', async () => {
    qualityBackend();
    renderLink();
    await flush();
    const session = currentSession()!;
    failStream();
    await flush();
    const handler = streamListeners.get('media-bilibili-stream');
    expect(handler).toBeTruthy();

    act(() => {
      handler!({
        payload: {
          sessionId: 'ffffffffffffffff',
          requestId: 1,
          stage: 'error',
          code: 'stale_timeout',
          host: 'https://upos-sz-mirrorcos.bilivideo.com/x?sign=SECRET',
          range: null,
          bytes: null,
          elapsedMs: 900,
        },
      });
    });
    expect(document.querySelector('[data-bilibili-stream-error]')?.getAttribute('data-bilibili-stream-reason')).toBe(
      'network',
    );

    act(() => {
      handler!({
        payload: {
          sessionId: session,
          requestId: 2,
          stage: 'error',
          code: 'cdn_timeout',
          host: 'https://upos-sz-mirrorcos.bilivideo.com/x?sign=SECRET',
          range: null,
          bytes: null,
          elapsedMs: 1200,
        },
      });
    });
    expect(document.querySelector('[data-bilibili-stream-error]')?.getAttribute('data-bilibili-stream-reason')).toBe(
      'cdn_timeout',
    );
    expect(document.body.textContent).not.toContain('SECRET');
  });
});

describe('BilibiliEmbedPlayer · document load is not media ready', () => {
  it('reports documentLoaded without faking isReady or playing', () => {
    const handleRef = { current: null as MediaPlayerHandle | null };
    const onStatusChange = vi.fn();
    const { container } = render(
      <BilibiliEmbedPlayer link={LINK as never} handleRef={handleRef} onStatusChange={onStatusChange} />,
    );
    const iframe = container.querySelector('iframe')!;
    fireEvent.load(iframe);

    const last = onStatusChange.mock.calls.at(-1)![0] as FakeStatus;
    expect(last.documentLoaded).toBe(true);
    expect(last.isReady).toBe(false);
    expect(last.isPlaying).toBe(false);
    expect(container.querySelector('[data-bilibili-document-loaded="1"]')).not.toBeNull();
    // 外链播放器没有可截帧的元素，pause 不能真正生效
    expect(handleRef.current?.getElement()).toBeNull();
    expect(() => handleRef.current?.pause()).not.toThrow();
  });
});
