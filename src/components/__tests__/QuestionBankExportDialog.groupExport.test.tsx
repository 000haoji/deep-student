import React from 'react';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  pickSavePath: vi.fn(),
  saveTextFile: vi.fn(),
  notify: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@/utils/fileManager', () => ({
  fileManager: {
    pickSavePath: mocks.pickSavePath,
    saveTextFile: mocks.saveTextFile,
  },
}));
vi.mock('@/components/UnifiedNotification', () => ({
  showGlobalNotification: mocks.notify,
}));
vi.mock('@/utils/systemFileOpener', () => ({
  canOpenFilesExternally: () => false,
  canRevealInFolder: () => false,
  revealFileOrOpen: vi.fn(),
}));
vi.mock('@/app/navigation/androidBackCoordinator', () => ({
  registerBackHandler: () => () => {},
  BACK_PRIORITY: { overlay: 1 },
}));
vi.mock('@/components/layout', () => ({
  useMobileSubviewChrome: () => false,
}));

import { QuestionBankExportDialog } from '../QuestionBankExportDialog';
import type { Question } from '@/api/questionBankApi';

afterEach(() => {
  cleanup();
  mocks.invoke.mockReset();
  mocks.pickSavePath.mockReset();
  mocks.saveTextFile.mockReset();
  mocks.notify.mockReset();
});

const groupedQuestion: Question = {
  id: 'q1',
  questionLabel: '1',
  content: 'FIRST SUB QUESTION',
  questionType: 'short_answer',
  answer: 'A',
  status: 'new',
  groupId: 'g1',
  subLabel: '(1)',
  groupOrder: 0,
  group_context: {
    group: { id: 'g1', exam_id: 'exam_1', group_type: 'reading', title: 'Passage A', version: 1 },
    materials: [
      { id: 'm1', group_id: 'g1', order_index: 1, content_type: 'text', text: 'SHARED MATERIAL TEXT' },
    ],
    siblings: [],
    sibling_count: 2,
    group_progress: null,
  },
};

function renderDialog(questions: Question[]) {
  return render(
    <QuestionBankExportDialog
      open
      onOpenChange={vi.fn()}
      questions={questions}
      examName="Exam"
      examId="exam_1"
    />,
  );
}

describe('QuestionBankExportDialog #491 题组导出', () => {
  it('exports native JSON v2 through the backend bundle service', async () => {
    mocks.pickSavePath.mockResolvedValue('/tmp/exam.json');
    mocks.invoke.mockResolvedValue({
      schema: 'deep-student.qbank.bundle',
      format_version: 2,
      file_path: '/tmp/exam.json',
      file_size: 2048,
      sha256: 'deadbeef',
      round_trip_safe: true,
      groups: 1,
      materials: 1,
      questions: 1,
      standalone_questions: 0,
      assets: 1,
      tombstones: 0,
      warnings: [],
    });

    renderDialog([groupedQuestion]);
    // JSON 默认格式即原生 v2：明确可往返
    expect(screen.getByText(/原生 JSON v2/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '导出' }));

    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('qbank_export_bundle_json', {
        request: {
          exam_id: 'exam_1',
          file_path: '/tmp/exam.json',
          include_assets: true,
          include_study_state: true,
        },
      }),
    );
    expect(mocks.saveTextFile).not.toHaveBeenCalled();
  });

  it('warns that TXT expands shared materials and cannot round-trip groups', () => {
    renderDialog([groupedQuestion]);
    fireEvent.click(screen.getByRole('button', { name: 'TXT/Markdown' }));
    expect(screen.getByText(/展开到每道题/)).toBeInTheDocument();
  });

  it('expands group materials into each flat TXT card', async () => {
    mocks.saveTextFile.mockResolvedValue({ canceled: false, path: '/tmp/exam.md' });
    renderDialog([groupedQuestion]);
    fireEvent.click(screen.getByRole('button', { name: 'TXT/Markdown' }));
    fireEvent.click(screen.getByRole('button', { name: '导出' }));

    await waitFor(() => expect(mocks.saveTextFile).toHaveBeenCalled());
    const payload = mocks.saveTextFile.mock.calls[0][0];
    expect(payload.content).toContain('SHARED MATERIAL TEXT');
    expect(payload.content).toContain('FIRST SUB QUESTION');
  });

  it('saves the returned native JSON when the backend omits file_path', async () => {
    mocks.pickSavePath.mockResolvedValue('/tmp/exam.json');
    mocks.saveTextFile.mockResolvedValue({ canceled: false, path: '/tmp/exam.json' });
    mocks.invoke.mockResolvedValue({
      schema: 'deep-student.qbank.bundle',
      format_version: 2,
      file_size: 32,
      sha256: 'abc',
      round_trip_safe: true,
      groups: 1,
      materials: 1,
      questions: 1,
      standalone_questions: 0,
      assets: 1,
      tombstones: 0,
      warnings: [],
      json: '{"schema":"deep-student.qbank.bundle","format_version":2}',
    });

    renderDialog([groupedQuestion]);
    fireEvent.click(screen.getByRole('button', { name: '导出' }));

    await waitFor(() => expect(mocks.saveTextFile).toHaveBeenCalled());
    expect(mocks.saveTextFile.mock.calls[0][0].content).toContain('deep-student.qbank.bundle');
  });

  it('uses the bundle CSV projection for grouped banks and marks it lossy', async () => {
    mocks.pickSavePath.mockResolvedValue('/tmp/exam.csv');
    mocks.invoke.mockResolvedValue({ file_path: '/tmp/exam.csv', questions: 1 });

    renderDialog([groupedQuestion]);
    fireEvent.click(screen.getByRole('button', { name: 'CSV' }));
    fireEvent.click(screen.getByRole('button', { name: '导出' }));

    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('qbank_export_bundle_csv', {
        request: {
          exam_id: 'exam_1',
          file_path: '/tmp/exam.csv',
          include_study_state: false,
        },
      }),
    );
    await waitFor(() =>
      expect(mocks.notify).toHaveBeenCalledWith('warning', expect.stringContaining('扁平投影')),
    );
  });

  it('does not fabricate a native bundle when the backend service is unavailable', async () => {
    mocks.pickSavePath.mockResolvedValue('/tmp/exam.json');
    mocks.invoke.mockRejectedValueOnce(new Error('Command qbank_export_bundle_json not found'));
    mocks.saveTextFile.mockResolvedValue({ canceled: false, path: '/tmp/exam.json' });

    renderDialog([groupedQuestion]);
    fireEvent.click(screen.getByRole('button', { name: '导出' }));

    await waitFor(() =>
      expect(mocks.notify).toHaveBeenCalledWith(
        'warning',
        expect.stringContaining('原生题组导出服务不可用'),
      ),
    );
    // 兼容扁平 JSON 可以写，但只作为明确标注的降级输出
    expect(mocks.saveTextFile).toHaveBeenCalledTimes(1);
    expect(mocks.saveTextFile.mock.calls[0][0].content).not.toContain('deep-student.qbank.bundle');
  });
});
