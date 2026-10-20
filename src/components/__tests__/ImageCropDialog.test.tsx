import React from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), create: vi.fn(), revoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@/app/navigation/androidBackCoordinator', () => ({ registerBackHandler: () => () => {}, BACK_PRIORITY: { overlay: 1 } }));
vi.mock('@/components/layout', () => ({ useMobileSubviewChrome: () => false }));
vi.mock('@/components/UnifiedNotification', () => ({ showGlobalNotification: vi.fn() }));
vi.mock('../custom-scroll-area', () => ({ CustomScrollArea: ({ children }: { children: React.ReactNode }) => <div>{children}</div> }));
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));

import { ImageCropDialog } from '../ImageCropDialog';

const directory = [0, 2, 3].map(pageIndex => ({ blobHash: `blob_${pageIndex}`, dataUrl: '', pageIndex }));
const sourceRef = JSON.stringify({ schema_version: 1, kind: 'document_import', page_indices: [2], page_spans: [{ page_index: 2, bbox: [0.1, 0.2, 0.5, 0.6] }] });

beforeEach(() => {
  vi.clearAllMocks();
  mocks.create.mockImplementation(() => `blob:page-${mocks.create.mock.calls.length}`);
  URL.createObjectURL = mocks.create;
  URL.revokeObjectURL = mocks.revoke;
  mocks.invoke.mockImplementation(async (_cmd: string, args: any) => args.metadataOnly
    ? directory
    : [{ ...directory.find(page => page.pageIndex === args.pageIndex), dataUrl: 'data:image/png;base64,QUJD' }]);
});

describe('ImageCropDialog on-demand source images', () => {
  it('locates the source page and loads only that page, not the whole document', async () => {
    render(<ImageCropDialog open onOpenChange={() => {}} examId="exam_synthetic" questionId="q_synthetic" sourceRef={sourceRef} />);
    const image = await screen.findByAltText('Page 3');
    expect(image).toHaveAttribute('src', 'blob:page-1');
    expect(mocks.invoke).toHaveBeenCalledWith('qbank_get_source_images', { examId: 'exam_synthetic', metadataOnly: true });
    expect(mocks.invoke).toHaveBeenCalledWith('qbank_get_source_images', { examId: 'exam_synthetic', pageIndex: 2 });
    expect(mocks.invoke).toHaveBeenCalledTimes(2);
  });

  it('prefers global preview page indices when a second document has local page zero', async () => {
    const appendedRef = JSON.stringify({ schema_version: 1, kind: 'document_import',
      page_indices: [0], preview_page_indices: [3], page_offset: 3,
      page_spans: [{ page_index: 0, bbox: [0.1, 0.2, 0.5, 0.6] }],
    });
    render(<ImageCropDialog open onOpenChange={() => {}} examId="exam_synthetic" questionId="q_appended" sourceRef={appendedRef} />);
    await screen.findByAltText('Page 4');
    expect(mocks.invoke).toHaveBeenCalledWith('qbank_get_source_images', { examId: 'exam_synthetic', pageIndex: 3 });
    expect(mocks.invoke).not.toHaveBeenCalledWith('qbank_get_source_images', { examId: 'exam_synthetic', pageIndex: 0 });
  });

  it('revokes the previous page and clears the last image when closed', async () => {
    const props = { onOpenChange: () => {}, examId: 'exam_synthetic', questionId: 'q_synthetic', sourceRef };
    const { rerender } = render(<ImageCropDialog {...props} open />);
    await screen.findByAltText('Page 3');
    fireEvent.click(screen.getByRole('button', { name: 'next' }));
    await screen.findByAltText('Page 4');
    expect(mocks.revoke).toHaveBeenCalledWith('blob:page-1');
    rerender(<ImageCropDialog {...props} open={false} />);
    await waitFor(() => expect(mocks.revoke).toHaveBeenCalledWith('blob:page-2'));
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('keeps manual legacy questions usable and ignores a late single-page result after closing', async () => {
    let resolvePage!: (value: unknown) => void;
    mocks.invoke.mockImplementation(async (_cmd: string, args: any) => args.metadataOnly
      ? directory
      : new Promise(resolve => { resolvePage = resolve; }));
    const props = { onOpenChange: () => {}, examId: 'exam_synthetic', questionId: 'q_manual' };
    const { rerender } = render(<ImageCropDialog {...props} open />);
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('qbank_get_source_images', { examId: 'exam_synthetic', pageIndex: 0 }));
    rerender(<ImageCropDialog {...props} open={false} />);
    await act(async () => resolvePage([{ ...directory[0], dataUrl: 'data:image/png;base64,QUJD' }]));
    expect(mocks.create).not.toHaveBeenCalled();
  });
});
