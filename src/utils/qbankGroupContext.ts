/**
 * #491 题组上下文纯函数层：解析、字段裁剪、进度换算、导出兼容判定。
 *
 * 这里的输出直接进入 UI 与 AI 工具上下文，"不泄露兄弟答案" 是硬约束：
 * 无论后端是否误带 answer / explanation / structured_data / user_answer / is_correct，
 * 解析层都会丢弃它们（editor/export 用途也只放行 status）。
 */
import type { QuestionImage } from '@/api/questionBankApi';
import type {
  GroupMaterialContentType,
  NativeBundleV2Header,
  QuestionGroup,
  QuestionGroupContext,
  QuestionGroupMaterial,
  QuestionGroupProgress,
  QuestionGroupPurpose,
  QuestionGroupType,
  QuestionSibling,
} from '@/types/qbankGroup';

type Raw = Record<string, unknown>;

const GROUP_TYPES: readonly QuestionGroupType[] = ['reading', 'shared_condition', 'other'];
const MATERIAL_TYPES: readonly GroupMaterialContentType[] = ['text', 'formula', 'figure', 'mixed'];

function isRecord(value: unknown): value is Raw {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function pickString(...values: unknown[]): string | null {
  for (const value of values) {
    if (typeof value === 'string') return value;
  }
  return null;
}

function pickNumber(...values: unknown[]): number | null {
  for (const value of values) {
    if (typeof value === 'number' && Number.isFinite(value)) return value;
    if (typeof value === 'string' && value.trim() !== '') {
      const parsed = Number(value);
      if (Number.isFinite(parsed)) return parsed;
    }
  }
  return null;
}

function pickArray(...values: unknown[]): unknown[] {
  for (const value of values) {
    if (Array.isArray(value)) return value;
  }
  return [];
}

function normalizeGroupType(value: unknown): QuestionGroupType {
  return typeof value === 'string' && (GROUP_TYPES as readonly string[]).includes(value)
    ? (value as QuestionGroupType)
    : 'other';
}

function normalizeMaterialType(value: unknown): GroupMaterialContentType {
  return typeof value === 'string' && (MATERIAL_TYPES as readonly string[]).includes(value)
    ? (value as GroupMaterialContentType)
    : 'text';
}

function normalizeImages(value: unknown): QuestionImage[] | null {
  if (!Array.isArray(value)) return null;
  return value.filter(isRecord) as unknown as QuestionImage[];
}

function normalizeGroup(raw: Raw): QuestionGroup | null {
  const id = pickString(raw.id);
  const examId = pickString(raw.exam_id, raw.examId);
  if (!id || !examId) return null;
  const deletedAt = pickString(raw.deleted_at, raw.deletedAt);
  if (deletedAt) return null;
  return {
    id,
    exam_id: examId,
    group_type: normalizeGroupType(raw.group_type ?? raw.groupType),
    title: pickString(raw.title),
    source_ref: pickString(raw.source_ref, raw.sourceRef),
    version: pickNumber(raw.version, raw.group_version, raw.groupVersion) ?? 0,
    created_at: pickString(raw.created_at, raw.createdAt),
    updated_at: pickString(raw.updated_at, raw.updatedAt),
    deleted_at: null,
  };
}

function normalizeMaterial(raw: Raw, fallbackGroupId: string): QuestionGroupMaterial | null {
  const id = pickString(raw.id);
  if (!id) return null;
  const deletedAt = pickString(raw.deleted_at, raw.deletedAt);
  if (deletedAt) return null;
  return {
    id,
    group_id: pickString(raw.group_id, raw.groupId) ?? fallbackGroupId,
    order_index:
      pickNumber(raw.order_index, raw.orderIndex, raw.material_order, raw.materialOrder) ??
      Number.MAX_SAFE_INTEGER,
    content_type: normalizeMaterialType(raw.content_type ?? raw.contentType),
    text: pickString(raw.text, raw.content),
    images: normalizeImages(raw.images),
    title: pickString(raw.title),
    exam_id: pickString(raw.exam_id, raw.examId),
    source_ref: pickString(raw.source_ref, raw.sourceRef, raw.source_id, raw.sourceId),
    page_index: pickNumber(raw.page_index, raw.pageIndex, raw.page_no, raw.pageNo),
    bbox: raw.bbox ?? null,
    created_at: pickString(raw.created_at, raw.createdAt),
    updated_at: pickString(raw.updated_at, raw.updatedAt),
    deleted_at: null,
  };
}

function normalizeSibling(raw: Raw, purpose: QuestionGroupPurpose): QuestionSibling | null {
  const id = pickString(raw.id);
  if (!id) return null;
  const sibling: QuestionSibling = { id };
  const subLabel = pickString(raw.sub_label, raw.subLabel);
  if (subLabel !== null) sibling.sub_label = subLabel;
  const questionLabel = pickString(raw.question_label, raw.questionLabel);
  if (questionLabel !== null) sibling.question_label = questionLabel;
  const groupOrder = pickNumber(raw.group_order, raw.groupOrder);
  if (groupOrder !== null) sibling.group_order = groupOrder;
  const questionType = pickString(raw.question_type, raw.questionType);
  if (questionType !== null) sibling.question_type = questionType;
  // status 是作答/学习状态：仅编辑与导出用途可见，练习和 AI 工具不携带。
  if (purpose === 'editor' || purpose === 'export') {
    const status = pickString(raw.status);
    if (status !== null) sibling.status = status;
  }
  const content = pickString(raw.content, raw.ocr_text, raw.ocrText);
  if (content !== null) sibling.content = content;
  if (Array.isArray(raw.options)) {
    sibling.options = raw.options.filter(isRecord).map((option) => ({
      key: pickString(option.key) ?? '',
      content: pickString(option.content) ?? '',
    }));
  }
  if (Array.isArray(raw.tags)) {
    sibling.tags = raw.tags.filter((tag): tag is string => typeof tag === 'string');
  }
  return sibling;
}

/**
 * 把后端 `group_context`（snake_case，兼容 camelCase）归一成 UI 可直接消费的上下文。
 * 无组、组已软删除或结构非法时返回 null。
 */
export function parseQuestionGroupContext(
  raw: unknown,
  purpose: QuestionGroupPurpose = 'practice',
): QuestionGroupContext | null {
  if (!isRecord(raw)) return null;
  const groupRaw = isRecord(raw.group) ? raw.group : null;
  if (!groupRaw) return null;
  const group = normalizeGroup(groupRaw);
  if (!group) return null;

  const materials = pickArray(raw.materials)
    .filter(isRecord)
    .map((material) => normalizeMaterial(material, group.id))
    .filter((material): material is QuestionGroupMaterial => material !== null)
    .sort((a, b) => a.order_index - b.order_index);

  const seen = new Set<string>();
  const siblings: QuestionSibling[] = [];
  for (const entry of pickArray(raw.siblings)) {
    if (!isRecord(entry)) continue;
    const sibling = normalizeSibling(entry, purpose);
    if (!sibling || seen.has(sibling.id)) continue;
    seen.add(sibling.id);
    siblings.push(sibling);
  }

  const rawCount = pickNumber(raw.sibling_count, raw.siblingCount);
  return {
    group,
    materials,
    siblings,
    sibling_count: rawCount ?? siblings.length,
    group_progress: normalizeGroupProgress(raw.group_progress ?? raw.groupProgress),
  };
}

/**
 * 后端进度口径为 {total, mastered, review, in_progress, new}；
 * `answered` 只是按题去重的派生值（total - new），不覆盖原始计数。
 */
export function normalizeGroupProgress(
  raw: unknown,
): (QuestionGroupProgress & { answered: number }) | null {
  if (!isRecord(raw)) return null;
  const total = pickNumber(raw.total);
  if (total === null || total < 0) return null;
  const mastered = pickNumber(raw.mastered) ?? 0;
  const review = pickNumber(raw.review) ?? 0;
  const inProgress = pickNumber(raw.in_progress, raw.inProgress) ?? 0;
  const newCount = pickNumber(raw.new) ?? 0;
  const explicitAnswered = pickNumber(raw.answered);
  const answered = explicitAnswered ?? Math.max(0, total - newCount);
  return {
    total,
    mastered,
    review,
    in_progress: inProgress,
    new: newCount,
    answered,
  };
}

/** 单独解析一份材料响应（创建/更新材料命令的返回值）。 */
export function parseGroupMaterial(raw: unknown, groupId?: string): QuestionGroupMaterial | null {
  if (!isRecord(raw)) return null;
  return normalizeMaterial(raw, groupId ?? pickString(raw.group_id, raw.groupId) ?? '');
}

/** 按 group id 建索引；同一组出现多份上下文时保留版本更高的一份。 */
export function groupContextsToMap(
  contexts: ReadonlyArray<QuestionGroupContext | null | undefined>,
): Map<string, QuestionGroupContext> {
  const map = new Map<string, QuestionGroupContext>();
  for (const context of contexts) {
    if (!context) continue;
    const existing = map.get(context.group.id);
    if (!existing || context.group.version >= existing.group.version) {
      map.set(context.group.id, context);
    }
  }
  return map;
}

export type ExportFormatForCompat = 'json' | 'txt' | 'csv';

/**
 * 只有后端 bundle 服务产出的原生 JSON v2 可无损往返；
 * TXT/CSV/Markdown 是扁平化投影，共享材料会展开到每题且无法回导关系。
 */
export function describeExportCompatibility(
  format: ExportFormatForCompat,
  hasGroupRelations: boolean,
): { roundTrip: boolean; notice: 'none' | 'group_expand' } {
  if (format === 'json') return { roundTrip: true, notice: 'none' };
  return { roundTrip: false, notice: hasGroupRelations ? 'group_expand' : 'none' };
}

/** 校验原生 bundle 头，防止把旧的自定义 JSON 当成可往返主格式。 */
export function isNativeBundleV2(payload: unknown): payload is NativeBundleV2Header {
  if (!isRecord(payload)) return false;
  if (payload.schema !== 'deep-student.qbank.bundle') return false;
  if (payload.format_version !== 2) return false;
  return (
    isRecord(payload.exam) &&
    Array.isArray(payload.sources) &&
    Array.isArray(payload.groups) &&
    Array.isArray(payload.materials) &&
    Array.isArray(payload.children) &&
    Array.isArray(payload.standalone_questions) &&
    Array.isArray(payload.order) &&
    Array.isArray(payload.tombstones) &&
    Array.isArray(payload.assets)
  );
}

/** 组内材料的展示顺序（1..M 连续；乱序时按 order_index 升序稳定输出）。 */
export function orderedMaterials(materials: ReadonlyArray<QuestionGroupMaterial>): QuestionGroupMaterial[] {
  return [...materials].sort((a, b) => a.order_index - b.order_index);
}

/** 组是否含有可视化（图片/图形）材料，用于决定是否提示资产闭包。 */
export function groupHasFigureMaterials(context: QuestionGroupContext | null | undefined): boolean {
  if (!context) return false;
  return context.materials.some(
    (material) =>
      material.content_type === 'figure' ||
      material.content_type === 'mixed' ||
      (material.images?.length ?? 0) > 0,
  );
}
