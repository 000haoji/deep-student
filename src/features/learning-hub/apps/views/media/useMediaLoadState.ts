/**
 * useMediaLoadState — 把 <video>/<audio> 的真实事件接进 mediaLoadState 纯模型。
 *
 * - `sessionKey` 代表条目 / 清晰度 / 会话：变化即新会话，旧定时器与旧监听全部作废；
 * - `sessionId` 可选：B 站链接由 useBilibiliQuality 生成，并写进 bilistream:// URL 的 s= 参数，
 *   同一个 id 用于匹配 Rust 媒体事件；
 * - 定时 tick 只做预算检查，暂停 / seek / 页面隐藏期间停表；
 * - trace 只记录阶段与脱敏原因，不读取 URL / 标题 / 用户信息。
 */
import { useCallback, useEffect, useRef, useState } from 'react';

import {
  DEFAULT_MEDIA_LOAD_BUDGETS,
  MEDIA_LOAD_TICK_MS,
  createMediaLoadState,
  createMediaSessionId,
  createMediaTraceSession,
  mediaErrorReason,
  reduceMediaLoadEvent,
  type MediaLoadBudgets,
  type MediaLoadEvent,
  type MediaLoadState,
  type MediaTraceSink,
  type MediaTraceSession,
} from './mediaLoadState';

export interface UseMediaLoadStateOptions {
  /** 媒体元素（由 useMediaPlayback 提供） */
  mediaRef: React.RefObject<HTMLMediaElement | null>;
  /** 条目 / 清晰度 / 会话标识：变化即重建会话 */
  sessionKey: string;
  /** 外部会话 id（B 站链接）；不传时内部生成 */
  sessionId?: string | null;
  /** 覆盖首帧 / 无进展预算（测试或真机调参） */
  budgets?: Partial<MediaLoadBudgets>;
  /** trace 输出（默认只在调试总开关打开时输出结构化单行） */
  traceSink?: MediaTraceSink;
  /** 时钟注入（默认 Date.now），便于测试 */
  now?: () => number;
}

export interface MediaLoadStateController {
  state: MediaLoadState;
  sessionId: string;
  /** 重启预算并让元素重新加载当前源 */
  retry: () => void;
  /** 明确取消（卸载 / 换条目前调用），此后不再进入 failed */
  cancel: () => void;
}

export function useMediaLoadState({
  mediaRef,
  sessionKey,
  sessionId: externalSessionId = null,
  budgets,
  traceSink,
  now = Date.now,
}: UseMediaLoadStateOptions): MediaLoadStateController {
  const [internalSessionId, setInternalSessionId] = useState(() => createMediaSessionId());
  const sessionId = externalSessionId || internalSessionId;
  const [state, setState] = useState<MediaLoadState>(() => createMediaLoadState(sessionId, now()));

  const budgetsRef = useRef<MediaLoadBudgets>({
    firstFrameMs: budgets?.firstFrameMs ?? DEFAULT_MEDIA_LOAD_BUDGETS.firstFrameMs,
    noProgressMs: budgets?.noProgressMs ?? DEFAULT_MEDIA_LOAD_BUDGETS.noProgressMs,
  });
  budgetsRef.current = {
    firstFrameMs: budgets?.firstFrameMs ?? DEFAULT_MEDIA_LOAD_BUDGETS.firstFrameMs,
    noProgressMs: budgets?.noProgressMs ?? DEFAULT_MEDIA_LOAD_BUDGETS.noProgressMs,
  };

  const nowRef = useRef(now);
  nowRef.current = now;

  // 会话变化：从准备态重新开始（旧事件由 reducer 的 sessionId 检查隔离）
  useEffect(() => {
    const at = nowRef.current();
    setState((prev) => reduceMediaLoadEvent(prev, { type: 'start', at, sessionId }, budgetsRef.current));
  }, [sessionId]);

  // DOM 事件 + 可见性
  useEffect(() => {
    const el = mediaRef.current;
    const at = () => nowRef.current();
    const dispatch = (event: MediaLoadEvent) =>
      setState((prev) => reduceMediaLoadEvent(prev, event, budgetsRef.current));

    const bindings: Array<[string, () => void]> = el
      ? [
          ['loadedmetadata', () => dispatch({ type: 'loaded-metadata', at: at(), sessionId })],
          ['canplay', () => dispatch({ type: 'canplay', at: at(), sessionId })],
          ['playing', () => dispatch({ type: 'playing', at: at(), sessionId, positionSec: el.currentTime })],
          ['waiting', () => dispatch({ type: 'waiting', at: at(), sessionId })],
          ['pause', () => dispatch({ type: 'pause', at: at(), sessionId })],
          ['play', () => dispatch({ type: 'play', at: at(), sessionId })],
          ['seeking', () => dispatch({ type: 'seeking', at: at(), sessionId })],
          ['seeked', () => dispatch({ type: 'seeked', at: at(), sessionId, positionSec: el.currentTime })],
          [
            'progress',
            () => dispatch({ type: 'progress', at: at(), sessionId, positionSec: el.currentTime }),
          ],
          [
            'error',
            () =>
              dispatch({
                type: 'error',
                at: at(),
                sessionId,
                reason: mediaErrorReason(el.error?.code),
              }),
          ],
        ]
      : [];

    const listeners: Array<[string, EventListener]> = bindings.map(([type, handler]) => [type, handler]);
    for (const [type, handler] of listeners) el?.addEventListener(type, handler);

    const handleVisibility = () =>
      dispatch({
        type: document.visibilityState === 'hidden' ? 'hidden' : 'visible',
        at: at(),
        sessionId,
      });
    if (typeof document !== 'undefined') {
      document.addEventListener('visibilitychange', handleVisibility);
    }

    // 预算检查：暂停 / 隐藏 / seek 时 reducer 自己停表
    const timer = window.setInterval(
      () => dispatch({ type: 'tick', at: at(), sessionId }),
      MEDIA_LOAD_TICK_MS,
    );

    return () => {
      for (const [type, handler] of listeners) el?.removeEventListener(type, handler);
      if (typeof document !== 'undefined') {
        document.removeEventListener('visibilitychange', handleVisibility);
      }
      window.clearInterval(timer);
    };
  }, [mediaRef, sessionId]);

  // 脱敏 trace：阶段转换各记一条（附结构化原因）
  const traceSessionsRef = useRef<MediaTraceSession | null>(null);
  const prevPhaseRef = useRef<MediaLoadState['phase']>(state.phase);
  useEffect(() => {
    traceSessionsRef.current = createMediaTraceSession(sessionId, traceSink);
    prevPhaseRef.current = 'preparing';
  }, [sessionId, traceSink]);
  useEffect(() => {
    const trace = traceSessionsRef.current;
    const previous = prevPhaseRef.current;
    if (previous === state.phase) return;
    prevPhaseRef.current = state.phase;
    trace?.record(state.phase, { reason: state.reason });
  }, [state.phase, state.reason]);

  const retry = useCallback(() => {
    const at = nowRef.current();
    const nextSessionId = externalSessionId || createMediaSessionId();
    if (!externalSessionId) setInternalSessionId(nextSessionId);
    // 只重启预算与会话；媒体元素的重载由播放器层负责（避免重复 load）
    setState((prev) =>
      reduceMediaLoadEvent(prev, { type: 'start', at, sessionId: nextSessionId }, budgetsRef.current),
    );
  }, [externalSessionId]);

  const cancel = useCallback(() => {
    setState((prev) =>
      reduceMediaLoadEvent(prev, { type: 'cancel', at: nowRef.current(), sessionId }, budgetsRef.current),
    );
  }, [sessionId]);

  return { state, sessionId, retry, cancel };
}

export default useMediaLoadState;
