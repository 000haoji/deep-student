/**
 * #491 题组 / 共享材料共享契约（前端）
 *
 * 与 Rust 侧 `QuestionGroup` / `QuestionGroupMaterial` / 单题 `group_context`
 * 的 serde 输出一一对应（snake_case）。
 *
 * 关键约束：
 * - 组版本字段为 `version`；所有组/材料变更与组删除都携带 expected_group_version（材料无独立版本）。
 * - 材料 `order_index` 为 1..M 连续整数，按升序展示。
 * - 原生 JSON v2 由后端 bundle 服务产出（`deep-student.qbank.bundle` / `format_version: 2`），
 *   前端只透传与校验，不在 UI 侧另造无 assets 的导出包。
 *
 * 本文件只放类型；解析/裁剪见 `@/utils/qbankGroupContext`，命令适配见 `@/api/qbankGroupApi`。
 */
import type { QuestionImage } from '@/api/questionBankApi';

export type QuestionGroupType = 'reading' | 'shared_condition' | 'other';

export type GroupMaterialContentType = 'text' | 'formula' | 'figure' | 'mixed';

/**
 * 上下文用途：决定兄弟题字段裁剪。
 * - practice / ai_tool：仅题干、选项、标签、顺序；不含兄弟答案与作答状态
 * - editor / export：可含 status，仍然不含 answer/explanation/structured_data
 */
export type QuestionGroupPurpose = 'practice' | 'ai_tool' | 'editor' | 'export';

export interface QuestionGroup {
  id: string;
  exam_id: string;
  group_type: QuestionGroupType;
  title?: string | null;
  source_ref?: string | null;
  /** 组版本号；材料/组的任何变更都递增它，CAS 以它为基准。 */
  version: number;
  created_at?: string | null;
  updated_at?: string | null;
  deleted_at?: string | null;
}

export interface QuestionGroupMaterial {
  id: string;
  group_id: string;
  /** 1..M 连续顺序。 */
  order_index: number;
  content_type: GroupMaterialContentType;
  text?: string | null;
  images?: QuestionImage[] | null;
  title?: string | null;
  exam_id?: string | null;
  source_ref?: string | null;
  page_index?: number | null;
  bbox?: unknown;
  created_at?: string | null;
  updated_at?: string | null;
  deleted_at?: string | null;
}

/**
 * 兄弟题（组内其他可答子题）。刻意不包含 answer / explanation /
 * structured_data / user_answer / is_correct —— 防止经 UI 或 AI 工具提前泄露答案。
 */
export interface QuestionSibling {
  id: string;
  sub_label?: string | null;
  question_label?: string | null;
  group_order?: number | null;
  question_type?: string | null;
  /** 仅 editor / export 用途保留；练习与 AI 工具裁剪掉。 */
  status?: string | null;
  content?: string | null;
  options?: Array<{ key: string; content: string }> | null;
  tags?: string[] | null;
}

/** 后端组进度快照口径（按子题聚合，组头不计题）。 */
export interface QuestionGroupProgress {
  total: number;
  mastered: number;
  review: number;
  in_progress: number;
  new: number;
}

export interface QuestionGroupContext {
  group: QuestionGroup;
  /** 按 order_index 升序，已剔除软删除。 */
  materials: QuestionGroupMaterial[];
  siblings: QuestionSibling[];
  sibling_count?: number;
  group_progress?: QuestionGroupProgress | null;
}

export interface GroupDeletePreview {
  group_id: string;
  version: number;
  material_ids: string[];
  child_questions: Array<{
    id: string;
    sub_label?: string | null;
    question_label?: string | null;
    content_preview?: string | null;
  }>;
  total: number;
}

export interface GroupMaterialUpdateParams {
  order_index?: number;
  content_type?: GroupMaterialContentType;
  text?: string | null;
  images?: QuestionImage[] | null;
  page_index?: number | null;
  bbox?: unknown;
}

export interface GroupUpdateParams {
  group_type?: QuestionGroupType;
  title?: string | null;
}

/**
 * 原生 JSON v2 bundle 的最小校验面。UI 不构造该结构；它由后端 bundle 服务
 * 生成（含 assets 闭包），前端只在导入/预览时校验格式身份。
 */
export interface NativeBundleV2Header {
  schema: 'deep-student.qbank.bundle';
  format_version: 2;
  exam: unknown;
  sources: unknown[];
  groups: unknown[];
  materials: unknown[];
  children: unknown[];
  standalone_questions: unknown[];
  order: unknown[];
  tombstones: unknown[];
  assets: unknown[];
}
