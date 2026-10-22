/**
 * #491 题组命令适配层。
 *
 * 单一出口，UI/编辑器不直接拼 invoke 名称。后端命令尚在落盘时，
 * "命令不存在" 会被识别为可用性缺口并返回 null（UI 退回无组显示），
 * 其余错误（参数/权限/版本冲突/材料占用）照常抛出，不能被静默吞掉。
 */
import { invoke } from '@tauri-apps/api/core';
import { parseGroupMaterial, parseQuestionGroupContext } from '@/utils/qbankGroupContext';
import type {
  GroupDeletePreview,
  GroupMaterialUpdateParams,
  GroupUpdateParams,
  QuestionGroup,
  QuestionGroupContext,
  QuestionGroupMaterial,
  QuestionGroupPurpose,
} from '@/types/qbankGroup';

export const QBANK_GROUP_COMMANDS = {
  getContext: 'qbank_get_question_group_context',
  getGroup: 'qbank_get_question_group',
  listGroups: 'qbank_list_question_groups',
  createGroup: 'qbank_create_question_group',
  updateGroup: 'qbank_update_question_group',
  previewGroupDelete: 'qbank_preview_group_delete',
  deleteGroup: 'qbank_delete_question_group',
  createMaterial: 'qbank_create_group_material',
  updateMaterial: 'qbank_update_group_material',
  deleteMaterial: 'qbank_delete_group_material',
  publishGroup: 'qbank_publish_question_group',
  searchMaterials: 'qbank_search_group_materials',
  exportBundle: 'qbank_export_bundle_json',
  exportBundleCsv: 'qbank_export_bundle_csv',
  importBundle: 'qbank_import_bundle_json',
} as const;

function errorMessage(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  if (error && typeof error === 'object') {
    const record = error as Record<string, unknown>;
    if (typeof record.message === 'string') return record.message;
    try {
      return JSON.stringify(record);
    } catch {
      return String(error);
    }
  }
  return String(error ?? '');
}

/** Tauri 未注册命令的报错形态；只匹配明确的“命令不存在”，不吞参数错误。 */
export function isCommandUnavailableError(error: unknown): boolean {
  const message = errorMessage(error);
  return (
    /command\s+[\w_]+\s+not\s+found/i.test(message) ||
    /unknown\s+command/i.test(message) ||
    /not\s+allowed\s+by\s+the\s+application\s+configuration/i.test(message) ||
    /command\s+[\w_]+\s+does\s+not\s+exist/i.test(message)
  );
}

/**
 * 组/材料 CAS 冲突。后端以 expected_group_version 对比 group.version，
 * 冲突时返回稳定错误码；旧后端可能只回一句中文说明，按关键词一并识别。
 */
export function isGroupVersionConflictError(error: unknown): boolean {
  const message = errorMessage(error);
  return /GROUP_VERSION_CONFLICT|QBANK_CONFLICT|VERSION_CONFLICT/.test(message)
    || /expected_group_version|expected_version/.test(message)
    || /版本冲突/.test(message);
}

/** 材料仍被同组其它子题引用，禁止单独删除。 */
export function isMaterialInUseError(error: unknown): boolean {
  const message = errorMessage(error);
  return /MATERIAL_IN_USE|GROUP_MATERIAL_IN_USE/.test(message) || /材料.*引用/.test(message);
}

async function invokeOptional<T>(command: string, args?: Record<string, unknown>): Promise<T | null> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    if (isCommandUnavailableError(error)) return null;
    throw error;
  }
}

/**
 * 后端 `QuestionGroup` 是平铺结构（组字段 + materials + siblings），
 * 单题 `group_context` 才带 `group` 包装；这里统一成前端上下文。
 */
function parseGroupPayload(raw: unknown, purpose: QuestionGroupPurpose): QuestionGroupContext | null {
  if (!raw || typeof raw !== 'object') return null;
  const record = raw as Record<string, unknown>;
  if (typeof record.id === 'string' && !('group' in record)) {
    return parseQuestionGroupContext(
      {
        group: record,
        materials: Array.isArray(record.materials) ? record.materials : [],
        siblings: Array.isArray(record.siblings) ? record.siblings : [],
      },
      purpose,
    );
  }
  return parseQuestionGroupContext(raw, purpose);
}

export interface FetchGroupContextParams {
  examId: string;
  questionId?: string;
  cardId?: string;
  purpose?: QuestionGroupPurpose;
}

/**
 * 单题入口统一取 context。`question_id`/`card_id` 至少给一个；
 * 返回 null 表示该题无组或后端尚未提供该命令。
 */
export async function fetchQuestionGroupContext(
  params: FetchGroupContextParams,
): Promise<QuestionGroupContext | null> {
  const purpose = params.purpose ?? 'practice';
  const raw = await invokeOptional<unknown>(QBANK_GROUP_COMMANDS.getContext, {
    request: {
      exam_id: params.examId,
      question_id: params.questionId ?? null,
      card_id: params.cardId ?? null,
      purpose,
    },
  });
  return parseQuestionGroupContext(raw, purpose);
}

export interface QuestionGroupBundle {
  group: QuestionGroup;
  materials: QuestionGroupMaterial[];
  siblings: QuestionGroupContext['siblings'];
}

/** 按组 id 取完整 bundle（编辑场景）。 */
export async function fetchQuestionGroup(groupId: string): Promise<QuestionGroupContext | null> {
  const raw = await invokeOptional<unknown>(QBANK_GROUP_COMMANDS.getGroup, { groupId });
  return parseGroupPayload(raw, 'editor');
}

export async function listQuestionGroups(
  examId: string,
  page = 1,
  pageSize = 20,
): Promise<{ groups: QuestionGroupContext[]; total: number } | null> {
  const raw = await invokeOptional<unknown>(QBANK_GROUP_COMMANDS.listGroups, {
    examId,
    page,
    pageSize,
  });
  if (!raw || typeof raw !== 'object') return null;
  const record = raw as Record<string, unknown>;
  const rows = Array.isArray(record.groups) ? record.groups : [];
  const groups = rows
    .map((row) => parseGroupPayload(row, 'editor'))
    .filter((ctx): ctx is QuestionGroupContext => ctx !== null);
  const total = typeof record.total === 'number' ? record.total : groups.length;
  return { groups, total };
}

export async function createQuestionGroup(params: {
  examId: string;
  groupType: QuestionGroup['group_type'];
  title?: string | null;
  sourceRef?: string | null;
}): Promise<QuestionGroupContext | null> {
  const raw = await invokeOptional<unknown>(QBANK_GROUP_COMMANDS.createGroup, {
    params: {
      exam_id: params.examId,
      group_type: params.groupType,
      title: params.title ?? null,
      source_type: 'manual',
      source_ref: params.sourceRef ?? null,
    },
  });
  return parseGroupPayload(raw, 'editor');
}

export async function updateQuestionGroup(
  groupId: string,
  params: GroupUpdateParams,
  expectedVersion: number,
): Promise<QuestionGroupContext | null> {
  const raw = await invokeOptional<unknown>(QBANK_GROUP_COMMANDS.updateGroup, {
    request: {
      group_id: groupId,
      update: params,
      expected_version: expectedVersion,
    },
  });
  return parseGroupPayload(raw, 'editor');
}

/** 删除前预览受影响材料与子题；UI 必须展示这个清单再确认。 */
export async function previewGroupDelete(groupId: string): Promise<GroupDeletePreview | null> {
  const raw = await invokeOptional<unknown>(QBANK_GROUP_COMMANDS.previewGroupDelete, { groupId });
  if (!raw || typeof raw !== 'object') return null;
  const record = raw as Record<string, unknown>;
  const materialIds = Array.isArray(record.material_ids)
    ? record.material_ids.filter((id): id is string => typeof id === 'string')
    : [];
  const children = Array.isArray(record.child_questions)
    ? record.child_questions.filter((row): row is Record<string, unknown> => !!row && typeof row === 'object')
    : [];
  return {
    group_id: typeof record.group_id === 'string' ? record.group_id : groupId,
    version: typeof record.version === 'number' ? record.version : 0,
    material_ids: materialIds,
    child_questions: children.map((row) => ({
      id: typeof row.id === 'string' ? row.id : '',
      sub_label: typeof row.sub_label === 'string' ? row.sub_label : null,
      question_label: typeof row.question_label === 'string' ? row.question_label : null,
      content_preview:
        typeof row.content_preview === 'string'
          ? row.content_preview
          : typeof row.content === 'string'
            ? row.content.slice(0, 80)
            : null,
    })).filter((row) => row.id !== ''),
    total:
      typeof record.total_children === 'number'
        ? record.total_children
        : typeof record.total === 'number'
          ? record.total
          : children.length,
  };
}

/**
 * 整组删除：必须先拿到 preview 的版本并显式 confirmed=true，
 * 后端在同一事务软删除组/材料/子题。
 */
export async function deleteQuestionGroup(
  groupId: string,
  expectedVersion: number,
  confirmed: boolean,
): Promise<boolean> {
  const result = await invokeOptional<unknown>(QBANK_GROUP_COMMANDS.deleteGroup, {
    request: {
      group_id: groupId,
      expected_version: expectedVersion,
      confirmed,
    },
  });
  return result !== null;
}

export async function createGroupMaterial(params: {
  groupId: string;
  orderIndex: number;
  contentType: QuestionGroupMaterial['content_type'];
  text?: string | null;
  images?: QuestionGroupMaterial['images'];
  pageIndex?: number | null;
  bbox?: unknown;
}): Promise<QuestionGroupMaterial | null> {
  const raw = await invokeOptional<unknown>(QBANK_GROUP_COMMANDS.createMaterial, {
    params: {
      group_id: params.groupId,
      order_index: params.orderIndex,
      content_type: params.contentType,
      title: null,
      text: params.text ?? null,
      images: params.images ?? null,
      source_ref: null,
      page_index: params.pageIndex ?? null,
      bbox: params.bbox ?? null,
    },
  });
  return parseGroupMaterial(raw, params.groupId);
}

/** 材料变更统一 CAS 到 group.version（材料本身无版本）。 */
export async function updateGroupMaterial(
  materialId: string,
  params: GroupMaterialUpdateParams,
  expectedGroupVersion: number,
): Promise<boolean> {
  const result = await invokeOptional<unknown>(QBANK_GROUP_COMMANDS.updateMaterial, {
    request: {
      material_id: materialId,
      update: params,
      expected_version: expectedGroupVersion,
    },
  });
  return result !== null;
}

/** 单删材料：CAS 到组版本；最后一份材料且组内仍有子题时后端会拒绝（MATERIAL_IN_USE）。 */
export async function deleteGroupMaterial(
  materialId: string,
  expectedGroupVersion: number,
): Promise<boolean> {
  const result = await invokeOptional<unknown>(QBANK_GROUP_COMMANDS.deleteMaterial, {
    request: {
      material_id: materialId,
      expected_version: expectedGroupVersion,
    },
  });
  return result !== null;
}

export async function publishQuestionGroup(bundle: unknown): Promise<boolean> {
  const result = await invokeOptional<unknown>(QBANK_GROUP_COMMANDS.publishGroup, { bundle });
  return result !== null;
}

export interface GroupMaterialSearchHit {
  questionId: string;
  materialId: string;
  groupId: string;
  subLabel?: string | null;
  questionLabel?: string | null;
  groupOrder?: number | null;
  /** FTS 命中片段（材料正文），用于搜索页展示。 */
  snippet: string;
}

/** 搜索组材料：返回命中材料所属子题与片段（组材料更新后可重新索引）。 */
export async function searchGroupMaterials(
  keyword: string,
  options: { examId?: string; limit?: number } = {},
): Promise<GroupMaterialSearchHit[]> {
  const raw = await invokeOptional<unknown>(QBANK_GROUP_COMMANDS.searchMaterials, {
    keyword,
    examId: options.examId ?? null,
    limit: options.limit ?? 20,
  });
  if (!Array.isArray(raw)) return [];
  return raw
    .filter((row): row is Record<string, unknown> => !!row && typeof row === 'object')
    .map((row) => ({
      questionId: typeof row.question_id === 'string' ? row.question_id : '',
      materialId: typeof row.material_id === 'string' ? row.material_id : '',
      groupId: typeof row.group_id === 'string' ? row.group_id : '',
      subLabel: typeof row.sub_label === 'string' ? row.sub_label : null,
      questionLabel: typeof row.question_label === 'string' ? row.question_label : null,
      groupOrder: typeof row.group_order === 'number' ? row.group_order : null,
      snippet: typeof row.snippet === 'string' ? row.snippet : '',
    }))
    .filter((row) => row.materialId !== '' && row.questionId !== '');
}


/**
 * 原生 JSON v2 导出由后端 bundle 服务生成（含真实 asset 字节与 tombstones）。
 * Request: { exam_id, file_path?, include_assets, include_study_state }
 * Result:  { schema, format_version, file_path?, file_size, sha256, round_trip_safe,
 *            groups, materials, questions, standalone_questions, assets, tombstones,
 *            warnings: [{code,message,ref_id}], json? }
 * 前端只透传请求、校验返回并下载 `json`（未给 file_path 时），不自行拼装导出包。
 */
export interface NativeBundleWarning {
  code: string;
  message: string;
  refId?: string | null;
}

export interface NativeBundleExportResult {
  schema: string;
  formatVersion: number;
  filePath: string | null;
  fileSize: number;
  sha256: string | null;
  /** false 表示导出是 lossy 的（如 include_assets=false），必须向用户提示。 */
  roundTripSafe: boolean;
  groups: number;
  materials: number;
  questions: number;
  standaloneQuestions: number;
  assets: number;
  tombstones: number;
  warnings: NativeBundleWarning[];
  /** 未指定 file_path 时由后端直接返回完整 JSON 文本。 */
  json?: string;
}

function parseWarnings(value: unknown): NativeBundleWarning[] {
  if (!Array.isArray(value)) return [];
  return value
    .filter((row): row is Record<string, unknown> => !!row && typeof row === 'object')
    .map((row) => ({
      code: typeof row.code === 'string' ? row.code : 'UNKNOWN',
      message: typeof row.message === 'string' ? row.message : '',
      refId: typeof row.ref_id === 'string' ? row.ref_id : null,
    }))
    .filter((warning) => warning.message !== '');
}

export async function exportNativeBundleJson(options: {
  examId: string;
  filePath?: string;
  includeAssets?: boolean;
  includeStudyState?: boolean;
}): Promise<NativeBundleExportResult | null> {
  const raw = await invokeOptional<unknown>(QBANK_GROUP_COMMANDS.exportBundle, {
    request: {
      exam_id: options.examId,
      file_path: options.filePath ?? null,
      include_assets: options.includeAssets ?? true,
      include_study_state: options.includeStudyState ?? true,
    },
  });
  if (!raw || typeof raw !== 'object') return null;
  const record = raw as Record<string, unknown>;
  const count = (value: unknown) => (typeof value === 'number' ? value : 0);
  return {
    schema: typeof record.schema === 'string' ? record.schema : '',
    formatVersion: count(record.format_version),
    filePath: typeof record.file_path === 'string' ? record.file_path : null,
    fileSize: count(record.file_size),
    sha256: typeof record.sha256 === 'string' ? record.sha256 : null,
    roundTripSafe: record.round_trip_safe !== false,
    groups: count(record.groups),
    materials: count(record.materials),
    questions: count(record.questions),
    standaloneQuestions: count(record.standalone_questions),
    assets: count(record.assets),
    tombstones: count(record.tombstones),
    warnings: parseWarnings(record.warnings),
    json: typeof record.json === 'string' ? record.json : undefined,
  };
}

export type NativeBundleImportTarget =
  | { kind: 'new_exam'; name?: string }
  | { kind: 'merge'; exam_id: string };

export interface NativeBundleImportResult {
  dryRun: boolean;
  committed: boolean;
  examId: string | null;
  legacyV1: boolean;
  groupsCreated: number;
  materialsCreated: number;
  questionsCreated: number;
  questionsSkipped: number;
  assetsCreated: number;
  tombstonesApplied: number;
  warnings: NativeBundleWarning[];
  idMap: Array<{ clientRef: string; kind: string; newId: string }>;
}

/**
 * ImportRequest{bundle, target, dry_run, include_study_state}
 * `dry_run` 默认 true：先做可回滚校验，用户确认后才真正写入。
 */
export async function importNativeBundleJson(params: {
  bundle: unknown;
  target: NativeBundleImportTarget;
  dryRun?: boolean;
  includeStudyState?: boolean;
}): Promise<NativeBundleImportResult> {
  const empty: NativeBundleImportResult = {
    dryRun: true,
    committed: false,
    examId: null,
    legacyV1: false,
    groupsCreated: 0,
    materialsCreated: 0,
    questionsCreated: 0,
    questionsSkipped: 0,
    assetsCreated: 0,
    tombstonesApplied: 0,
    warnings: [],
    idMap: [],
  };
  const raw = await invokeOptional<unknown>(QBANK_GROUP_COMMANDS.importBundle, {
    request: {
      bundle: params.bundle,
      target: params.target,
      dry_run: params.dryRun ?? true,
      include_study_state: params.includeStudyState ?? true,
    },
  });
  if (!raw || typeof raw !== 'object') return empty;
  const record = raw as Record<string, unknown>;
  const count = (value: unknown) => (typeof value === 'number' ? value : 0);
  const idMap = Array.isArray(record.id_map)
    ? record.id_map
        .filter((row): row is Record<string, unknown> => !!row && typeof row === 'object')
        .map((row) => ({
          clientRef: typeof row.client_ref === 'string' ? row.client_ref : '',
          kind: typeof row.kind === 'string' ? row.kind : '',
          newId: typeof row.new_id === 'string' ? row.new_id : '',
        }))
        .filter((row) => row.newId !== '')
    : [];
  return {
    dryRun: record.dry_run !== false,
    committed: record.committed === true,
    examId: typeof record.exam_id === 'string' ? record.exam_id : null,
    legacyV1: record.legacy_v1 === true,
    groupsCreated: count(record.groups_created),
    materialsCreated: count(record.materials_created),
    questionsCreated: count(record.questions_created),
    questionsSkipped: count(record.questions_skipped),
    assetsCreated: count(record.assets_created),
    tombstonesApplied: count(record.tombstones_applied),
    warnings: parseWarnings(record.warnings),
    idMap,
  };
}

/**
 * 题组感知的 CSV 投影（后端 `project_csv_text`）。输出显式 lossy：
 * 共享材料展开到 group_context 列、资产不导出、题组关系无法回导。
 * 命令不可用时返回 null，由调用方退回旧的扁平 CSV 导出并保持提示。
 */
export async function exportBundleCsv(options: {
  examId: string;
  filePath: string;
  includeStudyState?: boolean;
}): Promise<{ path: string; rowCount: number } | null> {
  const raw = await invokeOptional<unknown>(QBANK_GROUP_COMMANDS.exportBundleCsv, {
    request: {
      exam_id: options.examId,
      file_path: options.filePath,
      include_study_state: options.includeStudyState ?? true,
    },
  });
  if (!raw || typeof raw !== 'object') return null;
  const record = raw as Record<string, unknown>;
  const path = typeof record.file_path === 'string'
    ? record.file_path
    : typeof record.exportPath === 'string'
      ? record.exportPath
      : options.filePath;
  return {
    path,
    rowCount: typeof record.questions === 'number'
      ? record.questions
      : typeof record.row_count === 'number'
        ? record.row_count
        : 0,
  };
}
