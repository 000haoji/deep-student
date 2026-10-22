/**
 * #492 清晰度查询失败的处理：
 * - quality 查询失败不再静默清空：可见的准备 / 失败状态 + 重试；
 * - streamUrl 独立生成，清晰度失败不阻断播放器挂载与播放尝试；
 * - 重试成功恢复菜单与状态。
 */
import React, { useImperativeHandle } from 'react';
import { act, cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.hoisted(() => vi.fn());
type FakeVideoProps = {
  src: string;
  sessionId?: string;
  onError: () => void;
  onStatusChange?: (s: { currentTime: number; duration: number; isPlaying: boolean; isReady: boolean }) => void;
  onLoadStateChange?: (s: Record<string, unknown>) => void;
  handleRef?: React.Ref<unknown>;
  quality?: { options: Array<{ value: number; label: string }>; value: number; onChange: (v: number) => void } | null;
};
const video = vi.hoisted(() => ({
  props: null as null | FakeVideoProps,
  mounts: 0,
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invokeMock,
  convertFileSrc: (path: string, protocol: string) => `${protocol}://localhost/${encodeURIComponent(path)}`,
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: async () => () => undefined }));
vi.mock('react-i18next', async (importOriginal) => ({
  ...(await importOriginal<typeof import('react-i18next')>()),
  useTranslation: () => ({ t: (key: string) => key, i18n: { language: 'zh-CN' } }),
}));
vi.mock('@/components/UnifiedNotification', () => ({ showGlobalNotification: vi.fn() }));
vi.mock('../VideoPlayer', () => ({
  VideoPlayer: function FakeVideo(props: FakeVideoProps) {
    video.props = props;
    useImperativeHandle(props.handleRef, () => ({
      getElement: () => null,
      seekTo: vi.fn(),
      play: vi.fn(),
      pause: vi.fn(),
    }));
    React.useEffect(() => {
      video.mounts += 1;
      props.onStatusChange?.({ currentTime: 0, duration: 0, isPlaying: false, isReady: false });
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);
    return <div data-testid="video-player" data-src={props.src} />;
  },
}));
vi.mock('../BilibiliEmbedPlayer', () => ({ BilibiliEmbedPlayer: () => <div data-testid="bilibili-embed" /> }));
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
import { resetBilibiliAccountForTests } from '../bilibiliAccount';

const LINK = {
  kind: 'bilibili', version: 1, bvid: 'BV1xx411c7mD', aid: 1, cid: 2, page: 1, pageCount: 1,
  title: '线代', part: '', owner: null, cover: null, durationMs: 60_000, url: 'https://www.bilibili.com/video/BV1xx411c7mD',
};
const OPTIONS = [{ qn: 80, label: '高清 1080P' }, { qn: 64, label: '高清 720P' }];
const ACCOUNT = { loggedIn: false, mid: null, uname: null, face: null, vip: false, verified: true, expired: false };

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

beforeEach(() => {
  vi.stubGlobal('ResizeObserver', FakeResizeObserver);
  window.localStorage.clear();
  resetBilibiliAccountForTests();
  invokeMock.mockReset();
  video.props = null;
  video.mounts = 0;
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function renderLink() {
  return render(
    <MediaStudyView kind="video" src="" bilibili={LINK as never} resourceId="file_link" fileName="线代.bilibili" onError={vi.fn()} />,
  );
}

describe('MediaStudyView · Bilibili quality failure', () => {
  it('keeps the stream mounted while surfacing the quality failure with a retry', async () => {
    let qualityCalls = 0;
    invokeMock.mockImplementation(async (cmd: string, args: { qn: number }) => {
      if (cmd === 'media_bilibili_auth_status') return ACCOUNT;
      qualityCalls += 1;
      if (qualityCalls === 1) throw new Error('backend offline');
      return { requested: args.qn, current: args.qn, options: OPTIONS };
    });

    renderLink();
    // 查询中：可见的 loading 状态
    expect(document.querySelector('[data-bilibili-quality-status="loading"]')).not.toBeNull();
    await flush();

    // 失败可见，但播放地址仍然独立生成、播放器照常挂载
    expect(document.querySelector('[data-bilibili-quality-error]')).not.toBeNull();
    expect(video.props?.src).toMatch(/^bilistream:\/\/localhost\/file_link\?qn=80&s=[0-9a-f]{16}$/);
    expect(video.props?.quality ?? null).toBeNull();

    // 重试：重新查询并恢复清晰度菜单；地址换新 session（播放器重挂载）
    const firstSrc = video.props?.src;
    fireEvent.click(document.querySelector('[data-bilibili-quality-retry]')!);
    await flush();

    expect(document.querySelector('[data-bilibili-quality-error]')).toBeNull();
    expect(invokeMock.mock.calls.filter(([cmd]) => cmd === 'media_bilibili_stream_quality')).toHaveLength(2);
    expect(video.props?.quality?.value).toBe(80);
    expect(video.props?.quality?.options).toEqual([
      { value: 80, label: '1080P' },
      { value: 64, label: '720P' },
    ]);
    expect(video.props?.src).not.toBe(firstSrc);
  });
});
