/**
 * B 站链接条目的应用内清晰度：请求的 qn（全局偏好，默认最高档）→ 播放地址；
 * 后端回报可选档位与 B 站实际给的档位（请求的档位拿不到时 B 站自动降档）。
 *
 * #492：
 * - 播放地址与清晰度查询分开：streamUrl 独立生成，quality 失败不会阻断视频挂载；
 * - quality 查询不再静默失败：暴露 loading / ready / error 与 retry；
 * - 每次打开 / 重试 / 切清晰度生成 16 位十六进制 session，写进 URL 的 s= 参数，
 *   并只接受同 session 的 Rust 媒体事件（media-bilibili-stream），旧 session 的迟到事件丢弃。
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import {
  BILIBILI_DEFAULT_QN,
  bilibiliStreamApi,
  buildBilibiliStreamUrl,
  getBilibiliQualityPreference,
  setBilibiliQualityPreference,
  useBilibiliAccountEpoch,
  type BilibiliStreamQuality,
} from './bilibiliAccount';
import {
  MEDIA_BILIBILI_STREAM_EVENT,
  createMediaSessionId,
  normalizeBilibiliStreamEvent,
  sanitizeTraceReason,
  type MediaTraceSink,
  type NormalizedBilibiliStreamEvent,
} from './mediaLoadState';

export type BilibiliQualityStatus = 'idle' | 'loading' | 'ready' | 'error';

export interface BilibiliQualityState {
  /** 应用内播放地址（含 qn / 账号代次 / 播放会话）；fileId 为空时为 null */
  streamUrl: string | null;
  /** 本次播放会话（与 URL 的 s=、Rust 媒体事件一致）；fileId 为空时为 null */
  session: string | null;
  /** 请求的清晰度 */
  requested: number;
  /** 与当前请求对应的后端回报；查询中 / 失败时为 null */
  info: BilibiliStreamQuality | null;
  /** 清晰度查询状态（不影响 streamUrl 是否可用） */
  status: BilibiliQualityStatus;
  /** 脱敏后的失败原因码；没有失败时为 null */
  error: string | null;
  /** 最后一条当前会话的 Rust 媒体事件（已脱敏，按 session 严格过滤） */
  lastStreamEvent: NormalizedBilibiliStreamEvent | null;
  /** 用户选了一档：记为全局偏好并换地址 */
  select: (qn: number) => void;
  /** 重新查询清晰度并换一个新的播放会话 */
  retry: () => void;
}

function initialQn(): number {
  return getBilibiliQualityPreference() ?? BILIBILI_DEFAULT_QN;
}

/** 错误信息可能带路径 / 地址：只保留结构化 token，不把原始文本带到 UI 与日志 */
function qualityErrorToken(error: unknown): string {
  const raw =
    error instanceof Error ? error.message : typeof error === 'string' ? error : '';
  if (!raw) return 'quality-unavailable';
  const match = raw.match(/(?:^|[\s:])([A-Za-z0-9_.:-]{3,64})(?:[\s.]|$)/);
  return sanitizeTraceReason(match?.[1] ?? '') ?? 'quality-unavailable';
}

export interface UseBilibiliQualityOptions {
  /** 测试或诊断注入 trace 输出 */
  traceSink?: MediaTraceSink;
}

export function useBilibiliQuality(
  fileId: string | null,
  options: UseBilibiliQualityOptions = {},
): BilibiliQualityState {
  const { traceSink } = options;
  const epoch = useBilibiliAccountEpoch();
  const [requested, setRequested] = useState(initialQn);
  const [attempt, setAttempt] = useState(0);
  const [info, setInfo] = useState<{ key: string; value: BilibiliStreamQuality } | null>(null);
  const [failure, setFailure] = useState<{ key: string; token: string } | null>(null);
  const [lastStreamEvent, setLastStreamEvent] = useState<NormalizedBilibiliStreamEvent | null>(null);

  // 换条目：回到偏好（别的条目里选的档位也是全局偏好）
  useEffect(() => {
    setRequested(initialQn());
  }, [fileId]);

  const sessionKey = `${fileId ?? ''}|${requested}|${epoch}|${attempt}`;
  // 会话 id 用「渲染期派生」保持稳定：同一 key 不重算，StrictMode 双调用也不会换 id
  const [session, setSession] = useState<string | null>(() => (fileId ? createMediaSessionId() : null));
  const sessionKeyRef = useRef(sessionKey);
  if (sessionKeyRef.current !== sessionKey) {
    sessionKeyRef.current = sessionKey;
    setSession(fileId ? createMediaSessionId() : null);
    setLastStreamEvent(null);
  }

  // 地址与 quality 查询解耦：查询失败也照样能尝试播放
  const streamUrl = useMemo(() => {
    if (!fileId) return null;
    const base = buildBilibiliStreamUrl(fileId, requested, epoch);
    return session ? `${base}${base.includes('?') ? '&' : '?'}s=${session}` : base;
  }, [fileId, requested, epoch, session]);

  useEffect(() => {
    if (!fileId || !session || !sessionKey) return undefined;
    let cancelled = false;
    bilibiliStreamApi
      .quality(fileId, requested, epoch)
      .then((value) => {
        if (!cancelled) setInfo({ key: session, value });
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setInfo(null);
          setFailure({ key: session, token: qualityErrorToken(err) });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [fileId, requested, epoch, session, sessionKey]);

  // Rust 媒体事件：仅监听当前会话（完整 emit 由 Rust 定向到当前 webview）
  useEffect(() => {
    if (!session) return undefined;
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void listen(MEDIA_BILIBILI_STREAM_EVENT, (event) => {
      const normalized = normalizeBilibiliStreamEvent(event.payload, session);
      if (!normalized) return;
      traceSink?.({
        sessionId: normalized.sessionId,
        stage: normalized.stage,
        at: normalized.at,
        elapsedMs: normalized.elapsedMs,
        reason: normalized.reason,
        host: normalized.host,
        bytes: normalized.bytes,
      });
      setLastStreamEvent(normalized);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [session, traceSink]);

  const select = useCallback((qn: number) => {
    setBilibiliQualityPreference(qn);
    setRequested(qn);
  }, []);

  const retry = useCallback(() => {
    setAttempt((value) => value + 1);
  }, []);

  const status: BilibiliQualityStatus = !fileId
    ? 'idle'
    : info && info.key === session
      ? 'ready'
      : failure && failure.key === session
        ? 'error'
        : 'loading';

  return {
    streamUrl,
    session,
    requested,
    info: info && info.key === session ? info.value : null,
    status,
    error: failure && failure.key === session ? failure.token : null,
    lastStreamEvent,
    select,
    retry,
  };
}
