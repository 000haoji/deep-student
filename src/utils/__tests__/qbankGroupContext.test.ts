import { describe, expect, it } from 'vitest';
import {
  describeExportCompatibility,
  groupContextsToMap,
  isNativeBundleV2,
  normalizeGroupProgress,
  parseQuestionGroupContext,
} from '../qbankGroupContext';

const rawContext = {
  group: {
    id: 'g1',
    exam_id: 'exam_1',
    group_type: 'reading',
    title: 'Passage A',
    version: 3,
    deleted_at: null,
  },
  materials: [
    {
      id: 'm2',
      group_id: 'g1',
      order_index: 2,
      content_type: 'figure',
      text: null,
      images: [{ id: 'img1', name: 'p1.png', mime: 'image/png', hash: 'h' }],
      title: 'figure title',
      exam_id: 'exam_1',
      source_ref: '{"sourceId":"s2"}',
      page_index: 2,
      deleted_at: null,
    },
    {
      id: 'm1',
      group_id: 'g1',
      order_index: 1,
      content_type: 'text',
      text: 'Shared passage text',
      title: null,
      exam_id: 'exam_1',
      source_ref: '{"sourceId":"s1"}',
      page_index: 1,
      deleted_at: null,
    },
    {
      id: 'm3',
      group_id: 'g1',
      order_index: 3,
      content_type: 'text',
      text: 'deleted material',
      deleted_at: '2026-10-20T00:00:00Z',
    },
  ],
  siblings: [
    {
      id: 'q2',
      sub_label: '(2)',
      question_label: '2',
      group_order: 1,
      question_type: 'short_answer',
      status: 'review',
      content: 'Given (1), find x.',
      options: null,
      answer: 'SECRET ANSWER',
      explanation: 'SECRET EXPLANATION',
      structured_data: { blanks: [{ answers: ['SECRET'] }] },
      user_answer: 'SECRET USER ANSWER',
      is_correct: true,
    },
  ],
  group_progress: { total: 3, mastered: 1, review: 1, in_progress: 0, new: 1 },
};

describe('parseQuestionGroupContext', () => {
  it('normalizes 1..M order and drops soft-deleted materials', () => {
    const ctx = parseQuestionGroupContext(rawContext)!;
    expect(ctx).not.toBeNull();
    expect(ctx.materials.map((m) => m.id)).toEqual(['m1', 'm2']);
    expect(ctx.group.version).toBe(3);
    expect(ctx.group_progress).toMatchObject({ total: 3, mastered: 1, review: 1, in_progress: 0, new: 1, answered: 2 });
  });

  it('preserves material title/exam_id/source_ref verbatim', () => {
    const ctx = parseQuestionGroupContext(rawContext)!;
    const figure = ctx.materials[1];
    expect(figure.title).toBe('figure title');
    expect(figure.exam_id).toBe('exam_1');
    expect(figure.source_ref).toBe('{"sourceId":"s2"}');
    expect(figure.images?.[0].id).toBe('img1');
  });

  it('never leaks sibling answers to practice/AI contexts', () => {
    const ctx = parseQuestionGroupContext(rawContext, 'practice')!;
    const sibling = ctx.siblings[0] as Record<string, unknown>;
    expect(sibling.content).toBe('Given (1), find x.');
    expect(sibling).not.toHaveProperty('answer');
    expect(sibling).not.toHaveProperty('explanation');
    expect(sibling).not.toHaveProperty('structured_data');
    expect(sibling).not.toHaveProperty('user_answer');
    expect(sibling).not.toHaveProperty('is_correct');
    expect(sibling).not.toHaveProperty('status');
  });

  it('keeps study status for editor/export but still strips answers', () => {
    const editor = parseQuestionGroupContext(rawContext, 'editor')!;
    const sibling = editor.siblings[0] as Record<string, unknown>;
    expect(sibling.status).toBe('review');
    expect(sibling).not.toHaveProperty('answer');
    expect(sibling).not.toHaveProperty('explanation');
    expect(sibling).not.toHaveProperty('structured_data');
  });

  it('accepts camelCase payloads and sorts unordered materials last', () => {
    const ctx = parseQuestionGroupContext({
      group: { id: 'g9', examId: 'exam_1', groupType: 'shared_condition', version: 1 },
      materials: [
        { id: 'a', groupId: 'g9', contentType: 'text', text: 'second', orderIndex: 5 },
        { id: 'b', groupId: 'g9', contentType: 'formula', text: 'first', orderIndex: 2 },
      ],
      siblings: [{ id: 'q1', content: 'stem', answer: 'x' }],
    })!;
    expect(ctx.group.group_type).toBe('shared_condition');
    expect(ctx.materials.map((m) => m.id)).toEqual(['b', 'a']);
    expect((ctx.siblings[0] as Record<string, unknown>).answer).toBeUndefined();
  });

  it('returns null for standalone questions and soft-deleted groups', () => {
    expect(parseQuestionGroupContext(null)).toBeNull();
    expect(parseQuestionGroupContext({})).toBeNull();
    expect(
      parseQuestionGroupContext({
        group: { id: 'g', exam_id: 'e', group_type: 'other', version: 1, deleted_at: '2026-10-20T00:00:00Z' },
        materials: [],
        siblings: [],
      }),
    ).toBeNull();
  });

  it('deduplicates siblings by id and defaults sibling_count', () => {
    const ctx = parseQuestionGroupContext({
      group: { id: 'g', exam_id: 'e', group_type: 'other', version: 1 },
      materials: [],
      siblings: [
        { id: 'q1', content: 'a' },
        { id: 'q1', content: 'a-dup' },
        { id: 'q2', content: 'b' },
      ],
    })!;
    expect(ctx.siblings.map((s) => s.id)).toEqual(['q1', 'q2']);
    expect(ctx.sibling_count).toBe(2);
  });
});

describe('groupContextsToMap', () => {
  it('indexes contexts by group id and keeps last version', () => {
    const older = parseQuestionGroupContext(rawContext)!;
    const newer = parseQuestionGroupContext({ ...rawContext, group: { ...rawContext.group, version: 9 } })!;
    const map = groupContextsToMap([older, newer]);
    expect(map.get('g1')?.group.version).toBe(9);
  });
});

describe('normalizeGroupProgress', () => {
  it('keeps backend counters and derives answered by 按题去重', () => {
    const progress = normalizeGroupProgress({ total: 5, mastered: 2, review: 1, in_progress: 1, new: 1 })!;
    expect(progress).toMatchObject({ total: 5, mastered: 2, review: 1, in_progress: 1, new: 1 });
    expect(progress.answered).toBe(4);
  });

  it('accepts legacy answered/total without dropping unknown counters', () => {
    const progress = normalizeGroupProgress({ answered: 2, total: 3 })!;
    expect(progress.total).toBe(3);
    expect(progress.answered).toBe(2);
  });

  it('returns null for absent or invalid progress', () => {
    expect(normalizeGroupProgress(null)).toBeNull();
    expect(normalizeGroupProgress({ total: 'x' })).toBeNull();
  });
});

describe('isNativeBundleV2', () => {
  const header = {
    schema: 'deep-student.qbank.bundle',
    format_version: 2,
    exam: { id: 'exam_1', name: 'Exam' },
    sources: [],
    groups: [],
    materials: [],
    children: [],
    standalone_questions: [],
    order: [],
    tombstones: [],
    assets: [],
  };

  it('accepts the backend bundle identity with all required arrays', () => {
    expect(isNativeBundleV2(header)).toBe(true);
  });

  it('rejects the legacy ad-hoc camelCase export shape', () => {
    expect(isNativeBundleV2({ schemaVersion: 2, name: 'Exam', questions: [] })).toBe(false);
    expect(isNativeBundleV2({ ...header, format_version: 1 })).toBe(false);
    // 只认 schema，不接受旧 format 字段混用
    expect(isNativeBundleV2({ ...header, schema: undefined, format: 'deep-student.qbank.bundle' })).toBe(false);
    // 缺少 assets/order 等必填数组即判为非权威包
    expect(isNativeBundleV2({ ...header, assets: undefined })).toBe(false);
    expect(isNativeBundleV2(null)).toBe(false);
  });
});

describe('describeExportCompatibility', () => {
  it('marks native JSON v2 as round-trip safe', () => {
    expect(describeExportCompatibility('json', true)).toEqual({ roundTrip: true, notice: 'none' });
    expect(describeExportCompatibility('json', false)).toEqual({ roundTrip: true, notice: 'none' });
  });

  it('warns that flat formats expand shared materials without lossless re-import', () => {
    expect(describeExportCompatibility('txt', true)).toEqual({ roundTrip: false, notice: 'group_expand' });
    expect(describeExportCompatibility('csv', true)).toEqual({ roundTrip: false, notice: 'group_expand' });
  });
});
