/**
 * B 站链接条目的播放器：player.bilibili.com 外链播放器（iframe）。
 *
 * 外链播放器不提供跳转 / 进度接口，所以对外句柄的 seekTo 是「带 t= 重新加载」，
 * play 只给刚请求的跳转加上自动播放，pause 无法生效；getElement 恒为 null（不能截帧）。
 * 不活跃（保活的隐藏标签）时卸载 iframe：无法暂停，只能卸载才不会在后台出声。
 *
 * #492：`iframe.onLoad` 只说明文档载入，不是媒体就绪。这里只上报 `documentLoaded`，
 * isReady / isPlaying 恒为 false，界面明确提示「是否起播以 B 站页面为准」。
 */
import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Television } from '@phosphor-icons/react';
import type { MediaPlayerHandle, MediaPlayerStatus } from './mediaPlayerHandle';
import { buildBilibiliEmbedUrl, type BilibiliLinkDescriptor } from './bilibiliLinkApi';

/** iframe sandbox：脚本与自身存储（跨源，碰不到应用）、全屏；不给弹窗与顶层导航 */
export const BILIBILI_EMBED_SANDBOX = 'allow-scripts allow-same-origin allow-presentation';

interface FrameState {
  seconds: number;
  autoplay: boolean;
  /** 每次跳转 +1，作为 iframe key 强制重载（同一秒再跳也会重新起播） */
  nonce: number;
}

export interface BilibiliEmbedPlayerProps {
  link: BilibiliLinkDescriptor;
  isActive?: boolean;
  handleRef: React.MutableRefObject<MediaPlayerHandle | null>;
  onStatusChange?: (status: MediaPlayerStatus) => void;
}

export const BilibiliEmbedPlayer: React.FC<BilibiliEmbedPlayerProps> = ({
  link,
  isActive = true,
  handleRef,
  onStatusChange,
}) => {
  const { t } = useTranslation(['learningHub']);
  const [frame, setFrame] = useState<FrameState>({ seconds: 0, autoplay: false, nonce: 0 });
  /** iframe 文档已载入——不等于媒体 ready，界面不得据此宣称可播放 */
  const [documentLoaded, setDocumentLoaded] = useState(false);
  const seekPendingRef = useRef(false);
  const durationSec = link.durationMs > 0 ? link.durationMs / 1000 : 0;

  const src = useMemo(
    () => buildBilibiliEmbedUrl(link, { startSeconds: frame.seconds, autoplay: frame.autoplay }),
    [link, frame.seconds, frame.autoplay],
  );

  useEffect(() => {
    handleRef.current = {
      getElement: () => null,
      seekTo: (seconds: number) => {
        const target = Math.max(0, durationSec > 0 ? Math.min(seconds, durationSec) : seconds);
        seekPendingRef.current = true;
        setDocumentLoaded(false);
        setFrame((prev) => ({ seconds: target, autoplay: prev.autoplay, nonce: prev.nonce + 1 }));
      },
      play: () => {
        if (!seekPendingRef.current) return;
        setFrame((prev) => (prev.autoplay ? prev : { ...prev, autoplay: true }));
      },
      pause: () => {},
    };
    return () => {
      handleRef.current = null;
    };
  }, [handleRef, durationSec]);

  useEffect(() => {
    onStatusChange?.({
      currentTime: frame.seconds,
      duration: durationSec,
      isPlaying: false,
      // 外链播放器没有可信的媒体状态：不伪造 isReady
      isReady: false,
      documentLoaded,
    });
  }, [frame.seconds, durationSec, documentLoaded, onStatusChange]);

  const handleLoad = useCallback(() => {
    seekPendingRef.current = false;
    setDocumentLoaded(true);
  }, []);

  const title = link.pageCount > 1 && link.part ? `${link.title} · P${link.page} ${link.part}` : link.title;

  return (
    <div className="relative flex h-full w-full items-center justify-center bg-black" data-bilibili-player="">
      {isActive ? (
        <iframe
          key={frame.nonce}
          src={src}
          title={t('learningHub:mediaBilibili.playerTitle', { title })}
          className="h-full w-full border-0"
          sandbox={BILIBILI_EMBED_SANDBOX}
          allow="autoplay; fullscreen; picture-in-picture; encrypted-media"
          allowFullScreen
          onLoad={handleLoad}
          data-bilibili-frame=""
        />
      ) : (
        <div className="flex flex-col items-center gap-2 text-xs text-white/60">
          <Television size={28} weight="duotone" aria-hidden="true" />
          <span className="max-w-xs truncate px-4">{title}</span>
        </div>
      )}
      {isActive && documentLoaded && (
        <p
          data-bilibili-document-loaded="1"
          className="pointer-events-none absolute inset-x-0 bottom-0 z-10 bg-black/60 px-3 py-1 text-[11px] text-white/80"
          role="status"
        >
          {t('learningHub:mediaBilibili.playback.embedLoaded')}
        </p>
      )}
    </div>
  );
};

export default BilibiliEmbedPlayer;
