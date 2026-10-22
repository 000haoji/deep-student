import React from 'react';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: mocks.invoke,
  convertFileSrc: (path: string) => path,
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}));
vi.mock('@/features/chat/components/renderers', () => ({
  MarkdownRenderer: ({ content }: { content: string }) => <div>{content}</div>,
  StreamingMarkdownRenderer: ({ content }: { content: string }) => <div>{content}</div>,
}));

import { QuestionBankEditor } from '../QuestionBankEditor';
import type { Question } from '@/api/questionBankApi';

afterEach(() => {
  cleanup();
  mocks.invoke.mockReset();
});

const owner = { examId: 'exam_1', viewInstanceId: 'group-context-test' };

const rawGroupContext = {
  group: {
    id: 'g1',
    exam_id: 'exam_1',
    group_type: 'reading',
    title: 'Passage A',
    version: 2,
  },
  materials: [
    { id: 'm1', group_id: 'g1', order_index: 1, content_type: 'text', text: 'SHARED PASSAGE TEXT' },
  ],
  siblings: [
    {
      id: 'q2',
      sub_label: '(2)',
      content: 'SECOND SUB QUESTION STEM',
      answer: 'BROTHER ANSWER LEAK',
      explanation: 'BROTHER EXPLANATION LEAK',
    },
  ],
  group_progress: { total: 3, mastered: 1, review: 0, in_progress: 0, new: 2 },
  sibling_count: 3,
};

const groupedQuestion: Question = {
  id: 'q1',
  questionLabel: '1',
  content: 'FIRST SUB QUESTION STEM',
  questionType: 'single_choice',
  options: [
    { key: 'A', content: 'one' },
    { key: 'B', content: 'two' },
  ],
  answer: 'B',
  explanation: 'because',
  status: 'new',
  groupId: 'g1',
  subLabel: '(1)',
  groupOrder: 0,
  group_context: rawGroupContext as unknown as Question['group_context'],
};

const secondChild: Question = {
  ...groupedQuestion,
  id: 'q2',
  questionLabel: '2',
  content: 'SECOND SUB QUESTION STEM',
  subLabel: '(2)',
  groupOrder: 1,
  answer: 'A',
};

function renderEditor(extra: Partial<React.ComponentProps<typeof QuestionBankEditor>> = {}) {
  return render(
    <QuestionBankEditor
      sessionId="exam_1"
      practiceSessionOwner={owner}
      questions={[groupedQuestion, secondChild]}
      currentIndex={0}
      showTimer={false}
      onNavigate={vi.fn()}
      {...extra}
    />,
  );
}

describe('QuestionBankEditor #491 题组上下文', () => {
  it('shows shared material and sibling stems, never sibling answers, in practice mode', () => {
    renderEditor();

    const panel = screen.getByTestId('question-group-panel');
    expect(panel).toHaveTextContent('SHARED PASSAGE TEXT');
    expect(panel).toHaveTextContent('SECOND SUB QUESTION STEM');
    expect(panel).toHaveTextContent('组进度 1/3');
    expect(panel).not.toHaveTextContent('BROTHER ANSWER LEAK');
    expect(panel).not.toHaveTextContent('BROTHER EXPLANATION LEAK');
    // 练习模式不暴露材料编辑/整组删除
    expect(screen.queryByRole('button', { name: '编辑材料' })).toBeNull();
    expect(screen.queryByRole('button', { name: '删除整组' })).toBeNull();
  });

  it('renders the same context in browse mode with material editing affordances', () => {
    renderEditor({ browseMode: true });

    expect(screen.getByTestId('question-group-panel')).toHaveTextContent('SHARED PASSAGE TEXT');
    expect(screen.getByRole('button', { name: '编辑材料' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '删除整组' })).toBeInTheDocument();
    // 浏览模式揭示本题答案，但不揭示兄弟题答案
    expect(screen.queryByText('BROTHER ANSWER LEAK')).toBeNull();
  });

  it('keeps the group context in random practice mode', () => {
    renderEditor({ practiceMode: 'random' });
    expect(screen.getByTestId('question-group-panel')).toHaveTextContent('SHARED PASSAGE TEXT');
    expect(screen.queryByText('BROTHER ANSWER LEAK')).toBeNull();
  });

  it('counts only child questions, not the group header', () => {
    renderEditor();
    // 2 个子题参与练习计数，组头（sibling_count=3 表示组内 3 个可答子题）不计题
    expect(screen.getByText('1/2')).toBeInTheDocument();
    expect(screen.getByTestId('question-group-card-count')).toHaveTextContent('组内 3 题');
  });

  it('deletes only the child question and never touches the group', async () => {
    const onDeleteQuestion = vi.fn().mockResolvedValue(undefined);
    renderEditor({ onDeleteQuestion, settingsPanelOpen: true });

    const deleteButtons = screen.getAllByRole('button', { name: '删除' });
    fireEvent.click(deleteButtons[0]);
    fireEvent.click(screen.getAllByRole('button', { name: '删除' })[0]);

    await waitFor(() => expect(onDeleteQuestion).toHaveBeenCalledWith('q1'));
    expect(
      mocks.invoke.mock.calls.some(([command]) =>
        String(command).includes('question_group') || String(command).includes('group_material'),
      ),
    ).toBe(false);
  });
});
