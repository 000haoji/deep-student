/**
 * #491 题组上下文面板：共享材料（按 order_index 只展示一次）+ 同组必要题干 + 聚合进度。
 *
 * 安全边界：兄弟题只提供题干/选项/标签/顺序；本组件从不渲染兄弟答案。
 * 编辑边界：材料编辑与整组删除都走 expected_group_version CAS；删组前必须展示
 * 受影响材料与子题清单并要求确认；单删子题不在本组件内联动删组。
 */
import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Badge } from '@/components/ui/shad/Badge';
import { DsButton } from '@/components/ui/DsButton';
import { Textarea } from '@/components/ui/shad/Textarea';
import { LatexText } from '@/components/LatexText';
import { QuestionImageSlot } from '@/components/QuestionImageViews';
import { loadQuestionImageUrls } from '@/components/questionImageLoader';
import { cn } from '@/lib/utils';
import { showGlobalNotification } from '@/components/UnifiedNotification';
import {
  deleteQuestionGroup,
  fetchQuestionGroup,
  isGroupVersionConflictError,
  isMaterialInUseError,
  previewGroupDelete,
  updateGroupMaterial,
} from '@/api/qbankGroupApi';
import type { QuestionImage } from '@/api/questionBankApi';
import type {
  GroupDeletePreview,
  QuestionGroupContext,
  QuestionGroupMaterial,
} from '@/types/qbankGroup';
import { groupHasFigureMaterials, normalizeGroupProgress, orderedMaterials } from '@/utils/qbankGroupContext';

export interface QuestionGroupPanelProps {
  context: QuestionGroupContext;
  /** practice：只读上下文；edit：允许编辑材料/删除整组 */
  mode?: 'practice' | 'edit';
  /** 当前题 id：后端 context 可能包含本题，展示时排除，避免重复题干 */
  currentQuestionId?: string;
  /** 删除成功后通知宿主刷新列表/统计 */
  onGroupDeleted?: (groupId: string) => void;
  /** 材料更新后回传最新上下文（后端已递增组版本） */
  onMaterialUpdated?: (context: QuestionGroupContext) => void;
  className?: string;
}

const GROUP_TYPE_KEY = {
  reading: 'typeReading',
  shared_condition: 'typeSharedCondition',
  other: 'typeOther',
} as const;

function materialTypeKey(material: QuestionGroupMaterial): string {
  switch (material.content_type) {
    case 'figure':
      return 'materialFigure';
    case 'formula':
      return 'materialFormula';
    default:
      return '';
  }
}

/** 组材料配图：复用题目图片加载与内联放大交互（失败占位、点击展开）。 */
const MaterialImages: React.FC<{ images: QuestionImage[] }> = ({ images }) => {
  const [urls, setUrls] = useState<Record<string, string>>({});
  const imagesRef = useRef(images);
  imagesRef.current = images;
  const signature = images.map((image) => image.id).join(',');

  useEffect(() => {
    let cancelled = false;
    loadQuestionImageUrls(imagesRef.current)
      .then((next) => {
        if (!cancelled) setUrls(next);
      })
      .catch(() => {
        if (!cancelled) setUrls({});
      });
    return () => {
      cancelled = true;
    };
  }, [signature]);

  return (
    <div className="mt-1 grid grid-cols-2 gap-2" data-testid="question-group-material-images">
      {images.map((image) => (
        <div key={image.id} className="overflow-hidden rounded-md border border-border/40 bg-muted/10">
          <QuestionImageSlot
            url={urls[image.id]}
            alt={image.name}
            collapsedClassName="max-h-40"
            placeholderClassName="h-20"
            iconSize={16}
          />
        </div>
      ))}
    </div>
  );
};

const QuestionGroupPanel: React.FC<QuestionGroupPanelProps> = ({
  context,
  mode = 'practice',
  currentQuestionId,
  onGroupDeleted,
  onMaterialUpdated,
  className,
}) => {
  const { t } = useTranslation(['exam_sheet']);

  const [showSiblings, setShowSiblings] = useState(true);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draft, setDraft] = useState('');
  const [saving, setSaving] = useState(false);
  const [deletePreview, setDeletePreview] = useState<GroupDeletePreview | null>(null);
  const [deleteBusy, setDeleteBusy] = useState(false);

  const materials = useMemo(() => orderedMaterials(context.materials), [context.materials]);
  const siblings = useMemo(
    () => context.siblings.filter((sibling) => sibling.id !== currentQuestionId),
    [context.siblings, currentQuestionId],
  );
  const progress = useMemo(() => normalizeGroupProgress(context.group_progress), [context.group_progress]);
  const childCount = Math.max(context.sibling_count ?? siblings.length, siblings.length);
  const hasFigures = groupHasFigureMaterials(context);

  const beginEdit = useCallback((material: QuestionGroupMaterial) => {
    setEditingId(material.id);
    setDraft(material.text ?? '');
  }, []);

  const cancelEdit = useCallback(() => {
    setEditingId(null);
    setDraft('');
  }, []);

  const saveMaterial = useCallback(
    async (material: QuestionGroupMaterial) => {
      if (saving) return;
      setSaving(true);
      try {
        const ok = await updateGroupMaterial(
          material.id,
          { text: draft },
          context.group.version,
        );
        if (ok) {
          showGlobalNotification('success', t('exam_sheet:questionBank.group.materialSaved'));
          setEditingId(null);
          const refreshed = await fetchQuestionGroup(context.group.id);
          if (refreshed) {
            onMaterialUpdated?.(refreshed);
          } else {
            onMaterialUpdated?.({
              ...context,
              group: { ...context.group, version: context.group.version + 1 },
              materials: context.materials.map((item) =>
                item.id === material.id ? { ...item, text: draft } : item,
              ),
            });
          }
        }
      } catch (error) {
        if (isGroupVersionConflictError(error)) {
          showGlobalNotification('error', t('exam_sheet:questionBank.group.versionConflict'));
        } else if (isMaterialInUseError(error)) {
          showGlobalNotification('error', t('exam_sheet:questionBank.group.materialInUse'));
        } else {
          showGlobalNotification('error', error instanceof Error ? error.message : String(error));
        }
      } finally {
        setSaving(false);
      }
    },
    [context, draft, onMaterialUpdated, saving, t],
  );

  const requestDeleteGroup = useCallback(async () => {
    if (deleteBusy) return;
    setDeleteBusy(true);
    try {
      const preview = await previewGroupDelete(context.group.id);
      if (!preview) {
        showGlobalNotification('error', t('exam_sheet:questionBank.group.groupDeleteUnavailable'));
        return;
      }
      setDeletePreview(preview);
    } catch (error) {
      showGlobalNotification('error', error instanceof Error ? error.message : String(error));
    } finally {
      setDeleteBusy(false);
    }
  }, [context.group.id, deleteBusy, t]);

  const confirmDeleteGroup = useCallback(async () => {
    if (!deletePreview || deleteBusy) return;
    setDeleteBusy(true);
    try {
      const ok = await deleteQuestionGroup(
        deletePreview.group_id,
        deletePreview.version,
        true,
      );
      if (ok) {
        showGlobalNotification('success', t('exam_sheet:questionBank.group.deleteGroupDone'));
        setDeletePreview(null);
        onGroupDeleted?.(deletePreview.group_id);
      }
    } catch (error) {
      if (isGroupVersionConflictError(error)) {
        showGlobalNotification('error', t('exam_sheet:questionBank.group.versionConflict'));
      } else {
        showGlobalNotification('error', error instanceof Error ? error.message : String(error));
      }
    } finally {
      setDeleteBusy(false);
    }
  }, [deleteBusy, deletePreview, onGroupDeleted, t]);

  return (
    <section
      className={cn(
        'rounded-lg border border-border/50 bg-muted/20 px-3 py-2.5 text-sm',
        className,
      )}
      aria-label={t('exam_sheet:questionBank.group.title')}
      data-testid="question-group-panel"
    >
      <header className="flex flex-wrap items-center gap-2">
        <Badge variant="secondary" className="text-xs h-5">
          {t(`exam_sheet:questionBank.group.${GROUP_TYPE_KEY[context.group.group_type]}`)}
        </Badge>
        {context.group.title && (
          <span className="font-medium text-foreground">{context.group.title}</span>
        )}
        <span className="text-xs text-muted-foreground" data-testid="question-group-card-count">
          {t('exam_sheet:questionBank.group.subQuestionCount', { count: childCount })}
        </span>
        {progress && (
          <span className="text-xs text-muted-foreground" data-testid="question-group-progress">
            {t('exam_sheet:questionBank.group.progress', {
              answered: progress.answered,
              total: progress.total,
            })}
            {' · '}
            {t('exam_sheet:questionBank.group.progressMastered', { count: progress.mastered })}
          </span>
        )}
        {mode === 'edit' && (
          <DsButton
            variant="ghost"
            size="sm"
            className="ml-auto text-destructive hover:text-destructive"
            onClick={requestDeleteGroup}
            disabled={deleteBusy}
          >
            {t('exam_sheet:questionBank.group.deleteGroup')}
          </DsButton>
        )}
      </header>

      {materials.length > 0 && (
        <ol className="mt-2 space-y-2" data-testid="question-group-materials">
          {materials.map((material) => {
            const typeLabel = materialTypeKey(material);
            const isEditing = editingId === material.id;
            return (
              <li key={material.id} className="rounded-md bg-background/60 px-2.5 py-2">
                <div className="flex items-start gap-2">
                  <span className="mt-0.5 text-xs font-medium text-muted-foreground">
                    {t('exam_sheet:questionBank.group.materialOrder', { order: material.order_index })}
                  </span>
                  {typeLabel && (
                    <Badge variant="secondary" className="h-5 text-[11px]">
                      {t(`exam_sheet:questionBank.group.${typeLabel}`)}
                    </Badge>
                  )}
                  {mode === 'edit' && material.content_type !== 'figure' && !isEditing && (
                    <DsButton
                      variant="ghost"
                      size="sm"
                      className="ml-auto"
                      onClick={() => beginEdit(material)}
                    >
                      {t('exam_sheet:questionBank.group.editMaterial')}
                    </DsButton>
                  )}
                </div>
                {isEditing ? (
                  <div className="mt-1.5 space-y-1.5">
                    <Textarea
                      value={draft}
                      onChange={(event) => setDraft(event.target.value)}
                      rows={4}
                      aria-label={t('exam_sheet:questionBank.group.editMaterial')}
                    />
                    <div className="flex gap-2">
                      <DsButton size="sm" onClick={() => void saveMaterial(material)} disabled={saving}>
                        {t('exam_sheet:questionBank.group.saveMaterial')}
                      </DsButton>
                      <DsButton variant="ghost" size="sm" onClick={cancelEdit} disabled={saving}>
                        {t('exam_sheet:questionBank.group.cancelMaterial')}
                      </DsButton>
                    </div>
                  </div>
                ) : (
                  <>
                    {material.text && (
                      <div className="mt-1 whitespace-pre-wrap leading-relaxed">
                        <LatexText content={material.text} />
                      </div>
                    )}
                    {(material.images?.length ?? 0) > 0 && (
                      <MaterialImages images={material.images!} />
                    )}
                  </>
                )}
              </li>
            );
          })}
        </ol>
      )}

      {siblings.length > 0 && (
        <div className="mt-2">
          <DsButton
            variant="ghost"
            size="sm"
            onClick={() => setShowSiblings((value) => !value)}
            aria-expanded={showSiblings}
          >
            {showSiblings
              ? t('exam_sheet:questionBank.group.hideSiblings')
              : t('exam_sheet:questionBank.group.showSiblings')}
          </DsButton>
          {showSiblings && (
            <div className="mt-1.5 space-y-1.5" data-testid="question-group-siblings">
              <p className="text-xs text-muted-foreground">
                {t('exam_sheet:questionBank.group.siblingsHint')}
              </p>
              {siblings.map((sibling) => (
                <div key={sibling.id} className="rounded-md bg-background/40 px-2.5 py-1.5">
                  <div className="flex items-center gap-2 text-xs text-muted-foreground">
                    <span className="font-medium text-foreground">
                      {sibling.sub_label || sibling.question_label || '—'}
                    </span>
                  </div>
                  {sibling.content && (
                    <div className="mt-0.5 whitespace-pre-wrap leading-relaxed">
                      <LatexText content={sibling.content} />
                    </div>
                  )}
                  {sibling.options && sibling.options.length > 0 && (
                    <ul className="mt-1 space-y-0.5 text-xs text-muted-foreground">
                      {sibling.options.map((option) => (
                        <li key={option.key}>
                          {option.key}. {option.content}
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {hasFigures && mode === 'edit' && (
        <p className="mt-2 text-xs text-muted-foreground">
          {t('exam_sheet:questionBank.group.materialFigure')} · {context.materials.length}
        </p>
      )}

      {deletePreview && (
        <div
          className="mt-2 rounded-md border border-destructive/40 bg-destructive/5 px-2.5 py-2"
          role="alertdialog"
          aria-label={t('exam_sheet:questionBank.group.deleteGroupTitle')}
          data-testid="question-group-delete-confirm"
        >
          <p className="font-medium text-destructive">
            {t('exam_sheet:questionBank.group.deleteGroupTitle')}
          </p>
          <p className="mt-1 text-xs text-muted-foreground">
            {t('exam_sheet:questionBank.group.deleteGroupDesc', { count: deletePreview.total })}
          </p>
          <ul className="mt-1 space-y-0.5 text-xs">
            {deletePreview.child_questions.map((child) => (
              <li key={child.id}>
                {child.sub_label || child.question_label || child.id}
                {child.content_preview ? ` · ${child.content_preview}` : ''}
              </li>
            ))}
          </ul>
          <div className="mt-2 flex gap-2">
            <DsButton
              size="sm"
              variant="destructive"
              onClick={() => void confirmDeleteGroup()}
              disabled={deleteBusy}
            >
              {t('exam_sheet:questionBank.group.deleteGroupConfirm')}
            </DsButton>
            <DsButton
              variant="ghost"
              size="sm"
              onClick={() => setDeletePreview(null)}
              disabled={deleteBusy}
            >
              {t('exam_sheet:questionBank.group.deleteGroupCancel')}
            </DsButton>
          </div>
        </div>
      )}
    </section>
  );
};

export default QuestionGroupPanel;
