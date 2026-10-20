import React, { useState, useCallback, useRef, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { DsButton } from '@/components/ui/DsButton';
import { showGlobalNotification } from '@/components/UnifiedNotification';
import {
  Crop,
  CaretLeft,
  CaretRight,
  CircleNotch,
  ImageIcon,
  Trash,
  ArrowLeft,
} from '@phosphor-icons/react';
import { registerBackHandler, BACK_PRIORITY } from '@/app/navigation/androidBackCoordinator';
import { useMobileSubviewChrome } from '@/components/layout';
import { CustomScrollArea } from './custom-scroll-area';

// ============================================================================
// Types
// ============================================================================

interface SourceImageInfo {
  blobHash: string;
  dataUrl: string;
  pageIndex: number;
}

interface CropRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface ImageCropDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  examId: string;
  questionId: string;
  /** Persisted document-import provenance; absent for legacy/manual questions. */
  sourceRef?: string | null;
  onImageAdded?: () => void;
  /**
   * @deprecated 组件现恒以内联裁剪工具渲染（模块规范禁模态），该 prop 仅为兼容保留、不再参与逻辑。
   */
  inline?: boolean;
  /** ★ 标签页：宿主标签页是否活跃（保活 display:none 实例不得接管统一顶栏） */
  isActive?: boolean;
}

function sourceLocation(sourceRef: string | null | undefined): { pageIndex?: number; rect: CropRect | null } {
  try {
    const source = JSON.parse(sourceRef || 'null');
    if (source?.schema_version !== 1 || source.kind !== 'document_import') return { rect: null };
    const validIndex = (index: unknown): index is number => typeof index === 'number' && Number.isInteger(index) && index >= 0;
    const globalIndex = Array.isArray(source.preview_page_indices) ? source.preview_page_indices.find(validIndex) : undefined;
    const offset = validIndex(source.page_offset) ? source.page_offset : 0;
    const localIndex = Array.isArray(source.page_indices) ? source.page_indices.find(validIndex) : undefined;
    const spans = Array.isArray(source.page_spans) ? source.page_spans : [];
    const span = globalIndex !== undefined
      ? spans.find((span: any) => validIndex(span?.page_index) && span.page_index + offset === globalIndex)
      : spans.find((span: any) => validIndex(span?.page_index));
    const pageIndex = globalIndex ?? ((span?.page_index ?? localIndex) !== undefined ? (span?.page_index ?? localIndex) + offset : undefined);
    const box = span?.bbox;
    // Only normalized xyxy is safe to apply without knowing the raster size.
    const rect = Array.isArray(box) && box.length === 4
      && box.every((value: unknown) => typeof value === 'number' && Number.isFinite(value) && value >= 0 && value <= 1)
      && box[2] > box[0] && box[3] > box[1]
      ? { x: box[0], y: box[1], width: box[2] - box[0], height: box[3] - box[1] } : null;
    return { pageIndex, rect };
  } catch { return { rect: null }; }
}

function imageObjectUrl(dataUrl: string): string {
  const match = /^data:(image\/[a-zA-Z0-9.+-]+);base64,(.+)$/.exec(dataUrl);
  if (!match) throw new Error('Invalid source image');
  const bytes = Uint8Array.from(atob(match[2]), character => character.charCodeAt(0));
  return URL.createObjectURL(new Blob([bytes], { type: match[1] }));
}

// ============================================================================
// ImageCropDialog
// ============================================================================

export function ImageCropDialog({
  open,
  onOpenChange,
  examId,
  questionId,
  sourceRef,
  onImageAdded,
  isActive,
}: ImageCropDialogProps) {
  const { t } = useTranslation('common');

  const [sourceImages, setSourceImages] = useState<SourceImageInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [currentPage, setCurrentPage] = useState(0);
  const [cropping, setCropping] = useState(false);
  const [loadedImage, setLoadedImage] = useState<{ blobHash: string; url: string } | null>(null);
  const [pageLoading, setPageLoading] = useState(false);
  const [loadError, setLoadError] = useState(false);
  const [catalogOwner, setCatalogOwner] = useState<string | null>(null);
  const owner = JSON.stringify([examId, questionId, sourceRef]);

  // Crop selection state
  const [cropRect, setCropRect] = useState<CropRect | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [dragStart, setDragStart] = useState<{ x: number; y: number } | null>(null);
  const imageContainerRef = useRef<HTMLDivElement>(null);
  const imageRef = useRef<HTMLImageElement>(null);
  // 内联工具根节点：Android 返回键 handler 的可见性守卫用（保活隐藏实例不吞返回键）
  const rootRef = useRef<HTMLDivElement>(null);

  // The catalog contains hashes/indices only. Closing also drops all retained data.
  useEffect(() => {
    setSourceImages([]);
    setCatalogOwner(null);
    setCropRect(null);
    setLoadedImage(null);
    setLoadError(false);
    if (!open || !examId) return;
    let cancelled = false;
    setLoading(true);
    void invoke<SourceImageInfo[]>('qbank_get_source_images', { examId, metadataOnly: true })
      .then(images => {
        if (cancelled) return;
        const location = sourceLocation(sourceRef);
        const index = images.findIndex(image => image.pageIndex === location.pageIndex);
        // Never retain legacy all-page data URLs in the catalog state.
        setSourceImages(images.map(image => ({ ...image, dataUrl: '' })));
        setCurrentPage(index >= 0 ? index : 0);
        setCropRect(index >= 0 ? location.rect : null);
        setCatalogOwner(owner);
      })
      .catch(error => {
        if (!cancelled) {
          console.error('[ImageCropDialog] Failed to load source image catalog:', error);
          setLoadError(true);
        }
      })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [open, examId, questionId, sourceRef, owner]);

  const selectedImage = catalogOwner === owner ? sourceImages[currentPage] : undefined;
  const selectedPageIndex = selectedImage?.pageIndex;
  const selectedBlobHash = selectedImage?.blobHash;
  useEffect(() => {
    if (!open || selectedPageIndex === undefined || !selectedBlobHash) return;
    let cancelled = false;
    let objectUrl: string | undefined;
    setLoadedImage(null);
    setPageLoading(true);
    setLoadError(false);
    void invoke<SourceImageInfo[]>('qbank_get_source_images', { examId, pageIndex: selectedPageIndex })
      .then(images => {
        if (cancelled) return;
        const image = images.find(image => image.pageIndex === selectedPageIndex && image.blobHash === selectedBlobHash);
        if (!image?.dataUrl) throw new Error('Source image is unavailable');
        objectUrl = imageObjectUrl(image.dataUrl);
        setLoadedImage({ blobHash: image.blobHash, url: objectUrl });
      })
      .catch(error => {
        if (!cancelled) {
          console.error('[ImageCropDialog] Failed to load source page:', error);
          setLoadError(true);
        }
      })
      .finally(() => { if (!cancelled) setPageLoading(false); });
    return () => {
      cancelled = true;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [open, examId, selectedPageIndex, selectedBlobHash, owner]);

  // Get relative coordinates within the displayed image
  const getRelativeCoords = useCallback((clientX: number, clientY: number) => {
    const img = imageRef.current;
    if (!img) return null;
    const rect = img.getBoundingClientRect();
    const x = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
    const y = Math.max(0, Math.min(1, (clientY - rect.top) / rect.height));
    return { x, y };
  }, []);

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    const coords = getRelativeCoords(e.clientX, e.clientY);
    if (!coords) return;
    e.preventDefault();
    setIsDragging(true);
    setDragStart(coords);
    setCropRect(null);
  }, [getRelativeCoords]);

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!isDragging || !dragStart) return;
    const coords = getRelativeCoords(e.clientX, e.clientY);
    if (!coords) return;

    const x = Math.min(dragStart.x, coords.x);
    const y = Math.min(dragStart.y, coords.y);
    const width = Math.abs(coords.x - dragStart.x);
    const height = Math.abs(coords.y - dragStart.y);

    setCropRect({ x, y, width, height });
  }, [isDragging, dragStart, getRelativeCoords]);

  const handleMouseUp = useCallback(() => {
    setIsDragging(false);
    // If the crop rect is too small, clear it (functional update avoids stale closure)
    setCropRect(prev => {
      if (prev && (prev.width < 0.01 || prev.height < 0.01)) return null;
      return prev;
    });
  }, []);

  // Touch support
  const handleTouchStart = useCallback((e: React.TouchEvent) => {
    if (e.touches.length !== 1) return;
    const touch = e.touches[0];
    const coords = getRelativeCoords(touch.clientX, touch.clientY);
    if (!coords) return;
    e.preventDefault();
    setIsDragging(true);
    setDragStart(coords);
    setCropRect(null);
  }, [getRelativeCoords]);

  const handleTouchMove = useCallback((e: React.TouchEvent) => {
    if (!isDragging || !dragStart || e.touches.length !== 1) return;
    const touch = e.touches[0];
    const coords = getRelativeCoords(touch.clientX, touch.clientY);
    if (!coords) return;

    const x = Math.min(dragStart.x, coords.x);
    const y = Math.min(dragStart.y, coords.y);
    const width = Math.abs(coords.x - dragStart.x);
    const height = Math.abs(coords.y - dragStart.y);

    setCropRect({ x, y, width, height });
  }, [isDragging, dragStart, getRelativeCoords]);

  const handleTouchEnd = useCallback(() => {
    setIsDragging(false);
    setCropRect(prev => {
      if (prev && (prev.width < 0.01 || prev.height < 0.01)) return null;
      return prev;
    });
  }, []);

  // Submit crop
  const handleCrop = useCallback(async () => {
    if (!cropRect || !selectedImage || pageLoading || loadedImage?.blobHash !== selectedImage.blobHash) return;

    setCropping(true);
    try {
      await invoke('qbank_crop_source_image', {
        request: {
          question_id: questionId,
          blob_hash: selectedImage.blobHash,
          crop_x: cropRect.x,
          crop_y: cropRect.y,
          crop_width: cropRect.width,
          crop_height: cropRect.height,
        },
      });

      showGlobalNotification('success', t('question_bank.crop_success'));

      setCropRect(null);
      onImageAdded?.();
    } catch (e: any) {
      console.error('[ImageCropDialog] Crop failed:', e);
      showGlobalNotification(
        'error',
        t('question_bank.crop_failed', { error: e?.message || String(e) }),
      );
    } finally {
      setCropping(false);
    }
  }, [cropRect, selectedImage, pageLoading, loadedImage, questionId, t, onImageAdded]);

  const currentImage = selectedImage && loadedImage?.blobHash === selectedImage.blobHash
    ? { ...selectedImage, dataUrl: loadedImage.url } : undefined;
  const hasMultiplePages = sourceImages.length > 1;

  // 内联裁剪工具：Android 返回键 = 取消关闭
  useEffect(() => {
    if (!open) return;
    return registerBackHandler(() => {
      // 可见性守卫（同 EnhancedPdfViewer）：保活但不可见的实例（display:none
      // 标签页）不得吞掉活跃视图的返回键、不得触发关闭回调。
      // visibility:hidden 不清布局盒（getClientRects 仍有返回值），需单独查 computed 值。
      const el = rootRef.current;
      if (!el || !el.isConnected) return false;
      if (el.getClientRects().length === 0) return false;
      if (window.getComputedStyle(el).visibility === 'hidden') return false;
      onOpenChange(false);
      return true;
    }, BACK_PRIORITY.overlay);
  }, [open, onOpenChange]);

  // 📱 小屏 learning-hub 承载：标题/返回上移 App 级统一顶栏，页内不再自绘
  // 第二条顶栏；「裁剪并添加」确认钮改渲染到下方提示条（见 hosted 分支）。
  // 返回箭头与系统返回键同语义（取消关闭）。无宿主（桌面分栏等）保持自绘顶栏。
  const subviewChromeHosted = useMobileSubviewChrome(
    {
      title: t('question_bank.source_images'),
      onBack: () => onOpenChange(false),
    },
    [t, onOpenChange],
    open && isActive !== false,
  );

  // 裁剪主体（Dialog 与 inline 工具共用）
  const cropBody = (
    <>
        {loading ? (
          <div className="flex items-center justify-center py-20">
            <CircleNotch size={24} className="animate-spin text-muted-foreground" />
          </div>
        ) : sourceImages.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-20 text-muted-foreground">
            <ImageIcon size={40} className="mb-2 opacity-40" />
            <p className="text-sm">
              {t('question_bank.no_source_images')}
            </p>
          </div>
        ) : (
          <div className="space-y-3">
            {/* Page navigation */}
            {hasMultiplePages && (
              <div className="flex items-center justify-center gap-3">
                <DsButton
                  variant="ghost"
                  size="sm"
                  iconOnly
                  disabled={currentPage === 0}
                  onClick={() => { setCurrentPage(p => p - 1); setCropRect(null); }}
                  aria-label={t('prev')}
                  className="[@media(pointer:coarse)]:!min-h-11 [@media(pointer:coarse)]:!min-w-11"
                >
                  <CaretLeft size={16} />
                </DsButton>
                <span className="text-sm text-muted-foreground tabular-nums">
                  {currentPage + 1} / {sourceImages.length}
                </span>
                <DsButton
                  variant="ghost"
                  size="sm"
                  iconOnly
                  disabled={currentPage === sourceImages.length - 1}
                  onClick={() => { setCurrentPage(p => p + 1); setCropRect(null); }}
                  aria-label={t('next')}
                  className="[@media(pointer:coarse)]:!min-h-11 [@media(pointer:coarse)]:!min-w-11"
                >
                  <CaretRight size={16} />
                </DsButton>
              </div>
            )}

            {pageLoading && <div role="status" className="flex justify-center py-10"><CircleNotch className="animate-spin" size={24} /></div>}
            {loadError && <p role="alert" className="py-4 text-center text-sm text-destructive">{t('question_bank.no_source_images')}</p>}
            {/* Image with crop overlay */}
            {currentImage && !pageLoading && (
              <div
                ref={imageContainerRef}
                className="relative select-none cursor-crosshair border border-border/50 rounded-lg overflow-hidden bg-muted/30"
                // 触屏拖选时禁用浏览器默认滚动/缩放手势（passive 监听下 preventDefault 不可靠）
                style={{ touchAction: 'none' }}
                onMouseDown={handleMouseDown}
                onMouseMove={handleMouseMove}
                onMouseUp={handleMouseUp}
                onMouseLeave={handleMouseUp}
                onTouchStart={handleTouchStart}
                onTouchMove={handleTouchMove}
                onTouchEnd={handleTouchEnd}
                onTouchCancel={handleTouchEnd}
              >
                <img
                  ref={imageRef}
                  src={currentImage.dataUrl}
                  alt={`Page ${currentImage.pageIndex + 1}`}
                  className="w-full h-auto pointer-events-none"
                  draggable={false}
/>

                {/* Crop overlay */}
                {cropRect && (
                  <>
                    {/* Dimmed areas */}
                    <div
                      className="absolute inset-0 bg-black/40 pointer-events-none"
                      style={{
                        clipPath: `polygon(
                          0% 0%, 100% 0%, 100% 100%, 0% 100%, 0% 0%,
                          ${cropRect.x * 100}% ${cropRect.y * 100}%,
                          ${cropRect.x * 100}% ${(cropRect.y + cropRect.height) * 100}%,
                          ${(cropRect.x + cropRect.width) * 100}% ${(cropRect.y + cropRect.height) * 100}%,
                          ${(cropRect.x + cropRect.width) * 100}% ${cropRect.y * 100}%,
                          ${cropRect.x * 100}% ${cropRect.y * 100}%
                        )`,
                      }}
/>
                    {/* Selection border */}
                    <div
                      className="absolute border-2 border-primary pointer-events-none"
                      style={{
                        left: `${cropRect.x * 100}%`,
                        top: `${cropRect.y * 100}%`,
                        width: `${cropRect.width * 100}%`,
                        height: `${cropRect.height * 100}%`,
                      }}
/>
                    {/* Size label */}
                    {imageRef.current && (
                      <div
                        className="absolute text-[10px] bg-primary text-primary-foreground px-1.5 py-0.5 rounded pointer-events-none"
                        style={{
                          left: `${cropRect.x * 100}%`,
                          top: `${cropRect.y * 100}%`,
                          transform: 'translateY(-100%)',
                        }}
                      >
                        {Math.round(cropRect.width * (imageRef.current.naturalWidth || 0))}
                        ×
                        {Math.round(cropRect.height * (imageRef.current.naturalHeight || 0))}
                      </div>
                    )}
                  </>
                )}
              </div>
            )}
          </div>
        )}
    </>
  );

  // ==================== 全屏内联裁剪工具（全端统一，禁模态） ====================
  if (!open) return null;
  return (
    <div
      ref={rootRef}
      className="absolute inset-0 z-30 flex flex-col bg-background"
      role="dialog"
      aria-label={t('question_bank.source_images')}
    >
        {/* 顶栏：取消 + 标题 + 确认（小屏由 App 级统一顶栏接管返回与标题，
            此处不再自绘；确认钮改渲染到下方提示条） */}
        {!subviewChromeHosted && (
          <div className="flex h-12 flex-shrink-0 items-center gap-1.5 border-b border-border/60 px-2">
            <DsButton
              variant="ghost"
              size="icon"
              iconOnly
              onClick={() => onOpenChange(false)}
              aria-label={t('cancel')}
              className="!h-11 !w-11 text-muted-foreground"
            >
              <ArrowLeft size={20} />
            </DsButton>
            <div className="flex min-w-0 flex-1 items-center gap-2">
              <ImageIcon size={16} className="flex-shrink-0 text-muted-foreground" />
              <span className="truncate text-sm font-medium text-foreground">
                {t('question_bank.source_images')}
              </span>
            </div>
            <DsButton
              variant="primary"
              size="sm"
              disabled={!cropRect || cropping || pageLoading || !currentImage}
              onClick={handleCrop}
              className="!h-9 flex-shrink-0 px-3 [@media(pointer:coarse)]:!h-11"
            >
              {cropping ? (
                <CircleNotch size={14} className="mr-1 animate-spin" />
              ) : (
                <Crop size={14} className="mr-1" />
              )}
              {t('question_bank.crop_and_add')}
            </DsButton>
          </div>
        )}

        {/* 提示条：拖选提示 / 已选中 + 清除（+ 顶栏被接管时的确认钮） */}
        <div className="flex min-h-[36px] flex-shrink-0 items-center gap-2 border-b border-border/40 px-4 py-1.5">
          <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">
            {cropRect ? (
              <span className="font-medium text-primary">
                {t('question_bank.crop_selected')}
              </span>
            ) : sourceImages.length > 0 ? (
              t('question_bank.drag_to_crop')
            ) : (
              t('question_bank.crop_hint')
            )}
          </span>
          {cropRect && (
            <DsButton
              variant="ghost"
              size="sm"
              onClick={() => setCropRect(null)}
              className="!h-8 flex-shrink-0 px-2 text-xs [@media(pointer:coarse)]:!h-11"
            >
              <Trash size={14} className="mr-1" />
              {t('question_bank.clear_selection')}
            </DsButton>
          )}
          {subviewChromeHosted && (
            <DsButton
              variant="primary"
              size="sm"
              disabled={!cropRect || cropping || pageLoading || !currentImage}
              onClick={handleCrop}
              className="!h-9 flex-shrink-0 px-3 [@media(pointer:coarse)]:!h-11"
            >
              {cropping ? (
                <CircleNotch size={14} className="mr-1 animate-spin" />
              ) : (
                <Crop size={14} className="mr-1" />
              )}
              {t('question_bank.crop_and_add')}
            </DsButton>
          )}
        </div>

        {/* 图片区：全高滚动（safe-area 兼容） */}
        <CustomScrollArea
          className="min-h-0 flex-1"
          viewportClassName="px-3 pt-3"
          viewportProps={{
            style: {
              paddingBottom:
                'calc(var(--mobile-safe-area-bottom, env(safe-area-inset-bottom, 0px)) + 12px)',
            },
          }}
        >
          {cropBody}
        </CustomScrollArea>
      </div>
  );
}
