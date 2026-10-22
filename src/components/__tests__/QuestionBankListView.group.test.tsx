import React from 'react';
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi, beforeEach } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { QuestionBankListView } from '../QuestionBankListView';
import type { Question } from '@/api/questionBankApi';

const groupContext = {
  group: { id: 'g1', exam_id: 'exam_1', group_type: 'shared_condition', title: 'Shared', version: 1 },
  materials: [{ id: 'm1', group_id: 'g1', order_index: 1, content_type: 'text', text: 'SHARED MATERIAL PREVIEW' }],
  siblings: [],
};

function question(id: string, label: string, extra: Partial<Question> = {}): Question {
  return {
    id,
    questionLabel: label,
    content: `content ${id}`,
    questionType: 'short_answer',
    status: 'new',
    ...extra,
  };
}

const questions: Question[] = [
  question('q1', '1', {
    groupId: 'g1',
    subLabel: '(1)',
    groupOrder: 1,
    group_context: groupContext as unknown as Question['group_context'],
  }),
  question('q2', '2', {
    groupId: 'g1',
    subLabel: '(2)',
    groupOrder: 2,
    group_context: groupContext as unknown as Question['group_context'],
  }),
  question('q3', '3'),
];

describe('QuestionBankListView #491 题组展示', () => {
  beforeEach(() => {
    window.localStorage.setItem('qbank.listView.viewType', 'list');
  });
  afterEach(() => {
    cleanup();
    window.localStorage.clear();
  });

  it('shows group badge and sub labels, and renders shared material only once per block', () => {
    render(<QuestionBankListView questions={questions} />);

    expect(screen.getAllByText('共享条件', { exact: false }).length).toBeGreaterThan(0);
    expect(screen.getByText('(1)')).toBeInTheDocument();
    expect(screen.getByText('(2)')).toBeInTheDocument();
    // 组头不计题：只有 3 行子题
    expect(screen.getAllByText(/content q/)).toHaveLength(3);
    // 同一组材料只在连续组块首行出现一次
    expect(screen.getAllByText('SHARED MATERIAL PREVIEW')).toHaveLength(1);
  });
});
