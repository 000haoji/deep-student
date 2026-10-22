/**
 * 题目/材料图片共用视图：内联放大预览 + 加载/失败占位。
 * 从 QuestionBankEditor 抽出，供题组材料（#491）复用同一套交互与错误占位。
 */
import React from 'react';
import { useTranslation } from 'react-i18next';
import { CircleNotch, WarningCircle } from '@phosphor-icons/react';
import { cn } from '@/lib/utils';
import { QUESTION_IMAGE_LOAD_ERROR } from '@/components/questionImageLoader';

/** 内联放大预览图片：点击在题目区内展开/收起，不使用遮罩层灯箱 */
const InlineZoomImage: React.FC<{
  src: string;
  alt: string;
  collapsedClassName?: string;
}> = React.memo(({ src, alt, collapsedClassName = 'max-h-64' }) => {
  const [expanded, setExpanded] = React.useState(false);
  const { t } = useTranslation('practice');
  return (
    <button
      type="button"
      onClick={() => setExpanded(v => !v)}
      className={cn(
        'block w-full focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary/40',
        expanded ? 'cursor-zoom-out' : 'cursor-zoom-in'
      )}
      aria-expanded={expanded}
      aria-label={expanded ? t('editor.imageCollapse') : t('editor.imageExpand')}
      title={expanded ? t('editor.imageCollapse') : t('editor.imageExpand')}
    >
      <img
        src={src}
        alt={alt}
        loading="lazy"
        className={cn(
          'w-full object-contain transition-[max-height] duration-300 ease-out',
          expanded ? 'max-h-[70vh]' : collapsedClassName
        )}
/>
    </button>
  );
});
InlineZoomImage.displayName = 'InlineZoomImage';

/** 题目图片占位：已加载显示图片，失败显示错误占位，其余显示加载动画 */
const QuestionImageSlot: React.FC<{
  url: string | undefined;
  alt: string;
  collapsedClassName: string;
  placeholderClassName: string;
  iconSize: number;
}> = ({ url, alt, collapsedClassName, placeholderClassName, iconSize }) => {
  const { t } = useTranslation('exam_sheet');
  if (url && url !== QUESTION_IMAGE_LOAD_ERROR) {
    return <InlineZoomImage src={url} alt={alt} collapsedClassName={collapsedClassName} />;
  }
  if (url === QUESTION_IMAGE_LOAD_ERROR) {
    return (
      <div
        role="img"
        aria-label={t('image.load_failed')}
        className={cn('w-full flex flex-col items-center justify-center gap-1 text-xs text-muted-foreground', placeholderClassName)}
      >
        <WarningCircle size={iconSize} />
        <span>{t('image.load_failed')}</span>
      </div>
    );
  }
  return (
    <div className={cn('w-full flex items-center justify-center text-muted-foreground', placeholderClassName)}>
      <CircleNotch size={iconSize} className="animate-spin" />
    </div>
  );
};

export { InlineZoomImage, QuestionImageSlot };
