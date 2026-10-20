import React from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  getApiConfigurations: vi.fn(),
  getExamSheetSessionDetail: vi.fn(),
  updateExamSheetCards: vi.fn(),
  showGlobalNotification: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock('@/utils/tauriApi', () => ({
  TauriAPI: {
    getApiConfigurations: mocks.getApiConfigurations,
    getExamSheetSessionDetail: mocks.getExamSheetSessionDetail,
    updateExamSheetCards: mocks.updateExamSheetCards,
  },
}));
vi.mock('@/components/UnifiedNotification', () => ({ showGlobalNotification: mocks.showGlobalNotification }));
vi.mock('@/debug-panel/plugins/QuestionImportDebugPlugin', () => ({ emitImportDebug: vi.fn() }));
vi.mock('@/debug-panel/debugMasterSwitch', () => ({
  debugLog: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), log: vi.fn() },
}));
vi.mock('../custom-scroll-area', () => ({
  CustomScrollArea: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));
vi.mock('../LatexText', () => ({
  LatexText: ({ content }: { content: string }) => <span>{content}</span>,
}));
vi.mock('@/components/shared/UnifiedDragDropZone', () => ({
  DEFAULT_MAX_UPLOAD_FILE_SIZE: 50 * 1024 * 1024,
  UnifiedDragDropZone: ({ children, className }: { children: React.ReactNode; className?: string }) => (
    <div data-testid="dropzone-wrapper" className={className}>{children}</div>
  ),
}));
vi.mock('@/components/shared/UnifiedModelSelector', () => ({
  UnifiedModelSelector: ({ models }: { models: Array<{ id: string; name: string }> }) => (
    <select data-testid="model-select">
      {models.map(m => <option key={m.id}>{m.name}</option>)}
    </select>
  ),
}));
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, opts?: string | Record<string, unknown>) => {
      if (opts && typeof opts === 'object' && 'count' in opts) return `${key}:${opts.count}`;
      if (opts && typeof opts === 'object' && 'error' in opts) return `${key}:${opts.error}`;
      return key;
    },
  }),
  initReactI18next: { type: '3rdParty', init: () => {} },
}));

import { ExamSheetUploader, buildImportSummary } from '../ExamSheetUploader';

const card = (id: string, extra: Record<string, unknown> = {}) => ({
  card_id: id,
  ocr_text: `question ${id}`,
  question_type: 'single_choice',
  ...extra,
});

const detailWith = (cards: ReturnType<typeof card>[]) => ({
  summary: { id: 'exam_1' },
  preview: { pages: [{ cards }] },
}) as any;

const selectFiles = (files: File[]) => {
  const input = screen.getByTestId('exam-uploader-file-input') as HTMLInputElement;
  fireEvent.change(input, { target: { files } });
};

const docx = () => new File(['hello'], 'quiz.docx', {
  type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
});

const question = (id: string, cardId = id) => ({
  id: `entity_${id}`, card_id: cardId, content: `question ${id}`, question_label: id,
  question_type: 'single_choice', answer: 'A', options: [],
});
const questionPage = (ids: string[], page = 1, hasMore = false, total = ids.length) => ({
  questions: ids.map(id => question(id)), page, page_size: 50, has_more: hasMore, total,
});

describe('buildImportSummary', () => {
  it('only counts cards added by this import when a baseline is known', () => {
    const summary = buildImportSummary(
      detailWith([card('old-1'), card('old-2'), card('new-1', { question_type: 'fill_blank' }), card('new-2', { ocr_text: ' ' })]),
      new Set(['entity_old-1', 'entity_old-2']),
      [question('old-1'), question('old-2'), { ...question('new-1'), question_type: 'fill_blank' }, { ...question('new-2'), content: ' ' }],
    );
    expect(summary.cards.map(c => c.card_id)).toEqual(['new-1', 'new-2']);
    expect(summary.questionTypes).toEqual({ fill_blank: 1, single_choice: 1 });
    expect(summary.emptyQuestions).toBe(1);
  });

  it('shows all formal entities but disables filtering when the baseline is unknown', () => {
    const summary = buildImportSummary(detailWith([card('a'), card('b')]), null, [question('a'), question('b')]);
    expect(summary.cards).toHaveLength(2);
    expect(summary.canFilter).toBe(false);
  });

  it('never matches legacy identities by identical question text', () => {
    const summary = buildImportSummary(detailWith([card('legacy')]), new Set(), [question('legacy', 'unrelated')]);
    expect(summary.cards[0].id).toBe('entity_legacy');
    expect(summary.canFilter).toBe(false);
  });
});

describe('ExamSheetUploader (streamlined layout)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.invoke.mockReset();
    mocks.updateExamSheetCards.mockReset();
    mocks.getApiConfigurations.mockResolvedValue([]);
    mocks.getExamSheetSessionDetail.mockResolvedValue(detailWith([card('old-1')]));
    (globalThis as any).URL.createObjectURL = vi.fn(() => 'blob:preview');
    (globalThis as any).URL.revokeObjectURL = vi.fn();
  });

  it('empty state shows one title, the dropzone and the manual-create link — no stepper, tips or action bar', () => {
    render(<ExamSheetUploader sessionId="exam_1" onBack={() => {}} onManualCreate={() => {}} />);
    expect(screen.getAllByRole('heading')).toHaveLength(1);
    expect(screen.getByTestId('exam-uploader-dropzone')).toBeInTheDocument();
    expect(screen.getByText('exam_sheet:uploader.manual_create_link')).toBeInTheDocument();
    expect(screen.queryByText(/uploader\.steps\./)).toBeNull();
    expect(screen.queryByText(/uploader\.header_desc|uploader\.tips_combined/)).toBeNull();
    expect(screen.queryByTestId('exam-uploader-start')).toBeNull();
  });

  it('collapses the dropzone into a replace strip once a document is chosen and hides the empty model picker', async () => {
    render(<ExamSheetUploader sessionId="exam_1" onBack={() => {}} onManualCreate={() => {}} />);
    selectFiles([docx()]);

    expect(await screen.findByTestId('exam-uploader-selected-document')).toHaveTextContent('quiz.docx');
    expect(screen.queryByTestId('exam-uploader-dropzone')).toBeNull();
    expect(screen.getByTestId('exam-uploader-add-more')).toHaveTextContent('exam_sheet:uploader.replace_file');
    expect(screen.queryByTestId('exam-uploader-model')).toBeNull();
    expect(screen.queryByText('exam_sheet:uploader.manual_create_link')).toBeNull();
    expect(screen.getByTestId('exam-uploader-start')).toBeInTheDocument();
  });

  it('offers the model picker for images too (images go through the same model pipeline)', async () => {
    mocks.getApiConfigurations.mockResolvedValue([{ id: 'm1', name: 'Model One', model: 'x', enabled: true }]);
    render(<ExamSheetUploader sessionId="exam_1" />);
    await waitFor(() => expect(mocks.getApiConfigurations).toHaveBeenCalled());
    selectFiles([new File(['img'], 'p1.png', { type: 'image/png' })]);

    expect(await screen.findByTestId('exam-uploader-selected-images')).toBeInTheDocument();
    expect(await screen.findByTestId('exam-uploader-model')).toHaveTextContent('Model One');
    expect(screen.getByTestId('exam-uploader-add-more')).toHaveTextContent('exam_sheet:uploader.add_more_images');
  });

  it('summary lists only the newly imported questions and only deletes the ones the user unchecks', async () => {
    const onUploadSuccess = vi.fn();
    let imported = false;
    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'qbank_list_questions') return questionPage(imported ? ['old-1', 'new-1', 'new-2'] : ['old-1']);
      if (cmd === 'import_question_bank_stream') {
        imported = true;
        return detailWith([card('old-1'), card('new-1'), card('new-2')]);
      }
      return true;
    });
    mocks.updateExamSheetCards.mockResolvedValue(detailWith([card('old-1'), card('new-1')]));

    render(<ExamSheetUploader sessionId="exam_1" onUploadSuccess={onUploadSuccess} />);
    selectFiles([docx()]);
    await act(async () => {
      fireEvent.click(await screen.findByTestId('exam-uploader-start'));
    });

    const rows = await screen.findAllByTestId('exam-uploader-summary-question');
    expect(rows).toHaveLength(2);
    expect(screen.queryByText('question old-1')).toBeNull();
    expect(screen.getByText('exam_sheet:uploader.import_done:2')).toBeInTheDocument();

    fireEvent.click(rows[1].querySelector('input[type="checkbox"]')!);
    await act(async () => {
      fireEvent.click(screen.getByText('exam_sheet:uploader.view_questions_filtered:1'));
    });

    expect(mocks.updateExamSheetCards).toHaveBeenCalledWith({ session_id: 'exam_1', delete_card_ids: ['new-2'] });
    expect(onUploadSuccess).toHaveBeenCalledTimes(1);
  });

  it('loads every formal question page instead of silently counting an incomplete preview', async () => {
    let imported = false;
    mocks.invoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === 'qbank_list_questions') {
        if (!imported) return questionPage([]);
        return args.request.page === 1
          ? questionPage(['a'], 1, true, 2)
          : questionPage(['b'], 2, false, 2);
      }
      if (cmd === 'import_question_bank_stream') {
        imported = true;
        return detailWith([card('a')]);
      }
      return true;
    });
    render(<ExamSheetUploader sessionId="exam_1" />);
    selectFiles([docx()]);
    fireEvent.click(screen.getByTestId('exam-uploader-start'));
    expect(await screen.findAllByTestId('exam-uploader-summary-question')).toHaveLength(2);
    expect(screen.getByText('exam_sheet:uploader.import_done:2')).toBeInTheDocument();
    expect(screen.getByTestId('exam-uploader-integrity-warning')).toBeInTheDocument();
    expect(screen.getAllByRole('checkbox').every(input => (input as HTMLInputElement).disabled)).toBe(true);
  });

  it('shows persistent partial/degenerate warnings even when no Completed event was received', async () => {
    let imported = false;
    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'qbank_list_questions') return questionPage(imported ? ['a'] : []);
      if (cmd === 'import_question_bank_stream') {
        imported = true;
        const detail = detailWith([card('a')]);
        detail.summary.metadata = { import_report: {
          schema_version: 1, status: 'partial', failed_pages: [2], failed_chunks: [],
          degenerate_count: 1, quarantined_entries: [], conflicts: [], resume_available: true,
        } };
        return detail;
      }
      return true;
    });
    render(<ExamSheetUploader sessionId="exam_1" />);
    selectFiles([docx()]);
    fireEvent.click(screen.getByTestId('exam-uploader-start'));
    await screen.findByTestId('exam-uploader-summary');
    expect(screen.getByText('exam_sheet:uploader.incomplete_warning')).toBeInTheDocument();
    expect(screen.getByText('exam_sheet:uploader.degenerate_warning:1')).toBeInTheDocument();
  });

  it('does not navigate away or discard exclusions when unified filtering fails', async () => {
    let imported = false;
    const onUploadSuccess = vi.fn();
    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'qbank_list_questions') return questionPage(imported ? ['a', 'b'] : []);
      if (cmd === 'import_question_bank_stream') {
        imported = true;
        return detailWith([card('a'), card('b')]);
      }
      return true;
    });
    mocks.updateExamSheetCards.mockRejectedValue(new Error('QBANK_IMPORT_REVIEW_REQUIRED'));
    render(<ExamSheetUploader sessionId="exam_1" onUploadSuccess={onUploadSuccess} />);
    selectFiles([docx()]);
    fireEvent.click(screen.getByTestId('exam-uploader-start'));
    const rows = await screen.findAllByTestId('exam-uploader-summary-question');
    fireEvent.click(rows[1].querySelector('input')!);
    fireEvent.click(screen.getByText('exam_sheet:uploader.view_questions_filtered:1'));
    await waitFor(() => expect(mocks.updateExamSheetCards).toHaveBeenCalled());
    expect(onUploadSuccess).not.toHaveBeenCalled();
    expect(screen.getByTestId('exam-uploader-summary')).toBeInTheDocument();
    expect(rows[1].querySelector('input')).not.toBeChecked();
  });

  it('rejects an image batch above 200 MiB before any FileReader or backend import', async () => {
    const files = Array.from({ length: 5 }, (_, index) => {
      const file = new File(['synthetic'], `page${index}.png`, { type: 'image/png' });
      Object.defineProperty(file, 'size', { value: 45 * 1024 * 1024 });
      return file;
    });
    render(<ExamSheetUploader sessionId="exam_1" />);
    selectFiles(files);
    expect(await screen.findByRole('alert')).toBeInTheDocument();
    expect(screen.queryByTestId('exam-uploader-start')).toBeNull();
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it('reads images sequentially rather than starting the entire batch concurrently', async () => {
    let imported = false;
    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'qbank_list_questions') return questionPage(imported ? ['a'] : []);
      if (cmd === 'import_question_bank_stream') { imported = true; return detailWith([card('a')]); }
      return true;
    });
    const readers: Array<{ onload: null | (() => void); result: string }> = [];
    const OriginalReader = globalThis.FileReader;
    const read = vi.fn(function (this: { onload: null | (() => void); result: string }) { readers.push(this); });
    globalThis.FileReader = class {
      onload = null;
      onerror = null;
      result = 'data:image/png;base64,QUJD';
      readAsDataURL = read;
    } as any;
    try {
      render(<ExamSheetUploader sessionId="exam_1" />);
      selectFiles([new File(['a'], 'a.png', { type: 'image/png' }), new File(['b'], 'b.png', { type: 'image/png' })]);
      fireEvent.click(screen.getByTestId('exam-uploader-start'));
      await waitFor(() => expect(read).toHaveBeenCalled());
      expect(read).toHaveBeenCalledTimes(1);
      await act(async () => readers[0].onload?.());
      expect(read).toHaveBeenCalledTimes(2);
      await act(async () => readers[1].onload?.());
      await screen.findByTestId('exam-uploader-summary');
    } finally { globalThis.FileReader = OriginalReader; }
  });

  it('shows a failure once, with retry as the primary action instead of a second retry button', async () => {
    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'qbank_list_questions') return questionPage([]);
      if (cmd === 'import_question_bank_stream') throw new Error('未找到可用的模型');
      return true;
    });
    render(<ExamSheetUploader sessionId="exam_1" onBack={() => {}} />);
    selectFiles([new File(['img'], 'p1.png', { type: 'image/png' })]);
    await act(async () => {
      fireEvent.click(await screen.findByTestId('exam-uploader-start'));
    });

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('未找到可用的模型');
    expect(alert.querySelector('button')).toBeNull();
    expect(screen.getByTestId('exam-uploader-start')).toHaveTextContent('common:retry');
    // 失败只在页内说一次，不再额外弹全局错误通知
    expect(mocks.showGlobalNotification).not.toHaveBeenCalledWith('error', expect.anything());
  });
});
