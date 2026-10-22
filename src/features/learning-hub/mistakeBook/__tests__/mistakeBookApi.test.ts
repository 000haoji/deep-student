import { beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import { listMistakes } from '../mistakeBookApi';

describe('listMistakes', () => {
  beforeEach(() => invoke.mockReset());

  it('sends snake_case filters and maps backend questions for the shared question UI', async () => {
    invoke.mockResolvedValue({
      items: [{
        id: 'q1',
        exam_id: 'exam-a',
        exam_name: '高数',
        content: '求极限',
        question_type: 'single_choice',
        options: [{ key: 'A', content: '0' }],
        answer: 'A',
        user_answer: 'B',
        tags: ['极限'],
        status: 'review',
        attempt_count: 3,
        correct_count: 1,
        last_attempt_at: '2026-10-04T00:00:00Z',
        source_ref: '{"resourceIds":["res_1"]}',
      }],
      total: 7,
      page: 1,
      has_more: true,
      exams: [{ exam_id: 'exam-a', exam_name: '高数', count: 7 }],
    });

    const page = await listMistakes({ examId: 'exam-a', search: '  极限 ', sort: 'errors' });

    expect(invoke).toHaveBeenCalledWith('qbank_list_mistakes', {
      request: { filters: { exam_id: 'exam-a', search: '极限', sort: 'errors' }, page: 1, page_size: 50 },
    });
    expect(page).toMatchObject({ total: 7, page: 1, hasMore: true, exams: [{ examId: 'exam-a', examName: '高数', count: 7 }] });
    expect(page.items[0]).toMatchObject({
      examId: 'exam-a',
      examName: '高数',
      question: {
        id: 'q1',
        content: '求极限',
        questionType: 'single_choice',
        userAnswer: 'B',
        attemptCount: 3,
        correctCount: 1,
        lastAttemptAt: '2026-10-04T00:00:00Z',
        sourceRef: '{"resourceIds":["res_1"]}',
      },
    });
  });

  it('carries group context for mistake entries without leaking sibling answers', async () => {
    invoke.mockResolvedValue({
      items: [{
        id: 'q2',
        exam_id: 'exam-a',
        content: '第二问',
        question_type: 'short_answer',
        answer: 'SECRET',
        status: 'review',
        group_id: 'g1',
        sub_label: '(2)',
        group_order: 2,
        group_context: {
          group: { id: 'g1', exam_id: 'exam-a', group_type: 'shared_condition', version: 3 },
          materials: [{ id: 'm1', group_id: 'g1', order_index: 1, content_type: 'text', text: 'SHARED' }],
          siblings: [{ id: 'q1', sub_label: '(1)', content: '第一问', answer: 'SIBLING SECRET' }],
          group_progress: { total: 2, mastered: 0, review: 2, in_progress: 0, new: 0 },
        },
      }],
      total: 1,
      page: 1,
      has_more: false,
      exams: [],
    });

    const page = await listMistakes({ examId: 'exam-a' });
    const question = page.items[0].question;
    expect(question.groupId).toBe('g1');
    expect(question.subLabel).toBe('(2)');
    expect(question.groupOrder).toBe(2);
    expect(question.group_context?.materials[0].text).toBe('SHARED');
    expect((question.group_context?.siblings[0] as Record<string, unknown>).answer).toBeUndefined();
  });

  it('treats blank filters as no filter', async () => {
    invoke.mockResolvedValue({ items: [], total: 0, page: 2, has_more: false, exams: [] });
    await listMistakes({ examId: null, search: '   ', page: 2 });
    expect(invoke).toHaveBeenCalledWith('qbank_list_mistakes', {
      request: { filters: { exam_id: null, search: null, sort: 'recent' }, page: 2, page_size: 50 },
    });
  });
});
