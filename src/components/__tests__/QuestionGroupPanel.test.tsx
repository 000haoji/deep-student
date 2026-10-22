import React from 'react';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  notify: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@/components/UnifiedNotification', () => ({
  showGlobalNotification: mocks.notify,
}));

import QuestionGroupPanel from '../QuestionGroupPanel';
import { parseQuestionGroupContext } from '@/utils/qbankGroupContext';

afterEach(() => {
  cleanup();
  mocks.invoke.mockReset();
  mocks.notify.mockReset();
});

const rawContext = {
  group: { id: 'g1', exam_id: 'exam_1', group_type: 'reading', title: 'Passage A', version: 3 },
  materials: [
    { id: 'm2', group_id: 'g1', order_index: 2, content_type: 'text', text: 'SECOND PART' },
    { id: 'm1', group_id: 'g1', order_index: 1, content_type: 'text', text: 'FIRST PART' },
    { id: 'm3', group_id: 'g1', order_index: 3, content_type: 'text', text: 'DELETED', deleted_at: '2026-10-20T00:00:00Z' },
  ],
  siblings: [
    {
      id: 'q2',
      sub_label: '(2)',
      content: 'SIBLING STEM',
      answer: 'SIBLING SECRET ANSWER',
      explanation: 'SIBLING SECRET EXPLANATION',
    },
  ],
  group_progress: { total: 3, mastered: 1, review: 0, in_progress: 0, new: 2 },
};

function context(purpose: 'practice' | 'editor' = 'editor') {
  return parseQuestionGroupContext(rawContext, purpose)!;
}

describe('QuestionGroupPanel', () => {
  it('shows shared materials in order, once, with progress and sibling stems only', () => {
    render(<QuestionGroupPanel context={context()} mode="edit" />);

    const panel = screen.getByTestId('question-group-panel');
    expect(panel).toHaveTextContent('Passage A');
    expect(panel).toHaveTextContent('组内 1 题');
    expect(panel).toHaveTextContent('组进度 1/3');
    expect(panel).toHaveTextContent('FIRST PART');
    expect(panel).toHaveTextContent('SECOND PART');
    expect(panel).not.toHaveTextContent('DELETED');
    expect(panel).toHaveTextContent('SIBLING STEM');
    // 兄弟答案/解析绝不进入 DOM
    expect(panel).not.toHaveTextContent('SIBLING SECRET ANSWER');
    expect(panel).not.toHaveTextContent('SIBLING SECRET EXPLANATION');
    // 材料只出现一次（按 order_index 排序，不重复展开）
    expect(screen.getAllByText('FIRST PART')).toHaveLength(1);
  });

  it('renders material figure images through the shared image loader', async () => {
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === 'vfs_get_attachment_content') {
        return { found: true, content: 'aGk=' };
      }
      throw new Error(`unexpected command ${command}`);
    });

    const imageContext = parseQuestionGroupContext({
      group: { id: 'g1', exam_id: 'exam_1', group_type: 'reading', version: 1 },
      materials: [{
        id: 'mi',
        group_id: 'g1',
        order_index: 1,
        content_type: 'figure',
        images: [{ id: 'img1', name: 'p1.png', mime: 'image/png', hash: 'h' }],
      }],
      siblings: [],
    }, 'practice')!;
    render(<QuestionGroupPanel context={imageContext} mode="practice" />);
    const image = await screen.findByAltText('p1.png');
    expect(image).toHaveAttribute('src', 'data:image/png;base64,aGk=');
  });

  it('excludes the current question from sibling stems when backend includes it', () => {
    const withSelf = parseQuestionGroupContext({
      ...rawContext,
      siblings: [
        { id: 'q1', sub_label: '(1)', content: 'CURRENT QUESTION STEM' },
        { id: 'q2', sub_label: '(2)', content: 'SIBLING STEM' },
      ],
    }, 'practice')!;
    render(<QuestionGroupPanel context={withSelf} currentQuestionId="q1" mode="practice" />);
    expect(screen.getByText('SIBLING STEM')).toBeInTheDocument();
    expect(screen.queryByText('CURRENT QUESTION STEM')).toBeNull();
  });

  it('does not expose edit/delete affordances in practice mode', () => {
    render(<QuestionGroupPanel context={context('practice')} mode="practice" />);
    expect(screen.queryByRole('button', { name: '删除整组' })).toBeNull();
    expect(screen.queryByRole('button', { name: '编辑材料' })).toBeNull();
    expect(screen.getByText('SIBLING STEM')).toBeInTheDocument();
  });

  it('saves material edits with the group version CAS and refreshes context', async () => {
    const onMaterialUpdated = vi.fn();
    mocks.invoke.mockImplementation(async (command: string, args: Record<string, unknown>) => {
      if (command === 'qbank_update_group_material') return true;
      if (command === 'qbank_get_question_group') {
        return {
          group: { ...rawContext.group, version: 4 },
          materials: [{ ...rawContext.materials[1], text: 'EDITED PART' }],
          siblings: [],
        };
      }
      throw new Error(`unexpected command ${command}`);
    });

    render(
      <QuestionGroupPanel context={context()} mode="edit" onMaterialUpdated={onMaterialUpdated} />,
    );
    fireEvent.click(screen.getAllByRole('button', { name: '编辑材料' })[0]);
    const textarea = screen.getByLabelText('编辑材料');
    fireEvent.change(textarea, { target: { value: 'EDITED PART' } });
    fireEvent.click(screen.getByRole('button', { name: '保存材料' }));

    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith(
        'qbank_update_group_material',
        {
          request: {
            material_id: 'm1',
            update: { text: 'EDITED PART' },
            expected_version: 3,
          },
        },
      ),
    );
    await waitFor(() => expect(onMaterialUpdated).toHaveBeenCalled());
    expect(onMaterialUpdated.mock.calls[0][0].group.version).toBe(4);
  });

  it('surfaces CAS conflicts instead of silently overwriting', async () => {
    mocks.invoke.mockRejectedValueOnce(new Error('GROUP_VERSION_CONFLICT: expected 3 got 4'));

    render(<QuestionGroupPanel context={context()} mode="edit" />);
    fireEvent.click(screen.getAllByRole('button', { name: '编辑材料' })[0]);
    fireEvent.change(screen.getByLabelText('编辑材料'), { target: { value: 'x' } });
    fireEvent.click(screen.getByRole('button', { name: '保存材料' }));

    await waitFor(() =>
      expect(mocks.notify).toHaveBeenCalledWith(
        'error',
        '题组已被修改，请刷新后重试',
      ),
    );
  });

  it('previews and confirms whole-group deletion with affected children', async () => {
    const onGroupDeleted = vi.fn();
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === 'qbank_preview_group_delete') {
        return {
          group_id: 'g1',
          version: 3,
          material_ids: ['m1', 'm2'],
          child_questions: [
            { id: 'q1', sub_label: '(1)', content_preview: 'first' },
            { id: 'q2', sub_label: '(2)', content_preview: 'second' },
          ],
          total: 4,
        };
      }
      if (command === 'qbank_delete_question_group') return true;
      throw new Error(`unexpected command ${command}`);
    });

    render(<QuestionGroupPanel context={context()} mode="edit" onGroupDeleted={onGroupDeleted} />);
    fireEvent.click(screen.getByRole('button', { name: '删除整组' }));

    const confirm = await screen.findByTestId('question-group-delete-confirm');
    expect(confirm).toHaveTextContent('将软删除该组及其材料，以及 4 道子题');
    expect(confirm).toHaveTextContent('(1)');
    expect(confirm).toHaveTextContent('(2)');

    fireEvent.click(screen.getByRole('button', { name: '确认删除整组' }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('qbank_delete_question_group', {
        request: {
          group_id: 'g1',
          expected_version: 3,
          confirmed: true,
        },
      }),
    );
    await waitFor(() => expect(onGroupDeleted).toHaveBeenCalledWith('g1'));
  });

  it('cancels deletion when the impact preview is unavailable', async () => {
    mocks.invoke.mockRejectedValueOnce(new Error('Command qbank_preview_group_delete not found'));
    render(<QuestionGroupPanel context={context()} mode="edit" />);
    fireEvent.click(screen.getByRole('button', { name: '删除整组' }));

    await waitFor(() =>
      expect(mocks.notify).toHaveBeenCalledWith(
        'error',
        '无法预览该组的删除影响，已取消操作',
      ),
    );
    expect(screen.queryByTestId('question-group-delete-confirm')).toBeNull();
    expect(mocks.invoke).not.toHaveBeenCalledWith('qbank_delete_question_group', expect.anything());
  });
});
