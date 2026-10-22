import { describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));

import {
  deleteQuestionGroup,
  exportNativeBundleJson,
  deleteGroupMaterial,
  fetchQuestionGroup,
  fetchQuestionGroupContext,
  importNativeBundleJson,
  listQuestionGroups,
  searchGroupMaterials,
  isCommandUnavailableError,
  isGroupVersionConflictError,
  isMaterialInUseError,
  previewGroupDelete,
  updateGroupMaterial,
} from '../qbankGroupApi';

const contextPayload = {
  group: { id: 'g1', exam_id: 'exam_1', group_type: 'reading', title: 'Passage A', version: 4 },
  materials: [
    { id: 'm1', group_id: 'g1', order_index: 1, content_type: 'text', text: 'passage' },
  ],
  siblings: [
    {
      id: 'q2',
      sub_label: '(2)',
      group_order: 1,
      content: 'second question',
      answer: 'LEAK',
      explanation: 'LEAK',
    },
  ],
  group_progress: { total: 2, mastered: 0, review: 0, in_progress: 0, new: 2 },
};

describe('fetchQuestionGroupContext', () => {
  it('calls the frozen command and strips sibling answers', async () => {
    mocks.invoke.mockResolvedValueOnce(contextPayload);
    const context = await fetchQuestionGroupContext({
      examId: 'exam_1',
      questionId: 'q1',
      purpose: 'ai_tool',
    });
    expect(mocks.invoke).toHaveBeenCalledWith('qbank_get_question_group_context', {
      request: {
        exam_id: 'exam_1',
        question_id: 'q1',
        card_id: null,
        purpose: 'ai_tool',
      },
    });
    expect(context?.group.id).toBe('g1');
    expect((context?.siblings[0] as Record<string, unknown>).answer).toBeUndefined();
  });

  it('returns null when the backend command is not registered yet', async () => {
    mocks.invoke.mockRejectedValueOnce(new Error('Command qbank_get_question_group_context not found'));
    await expect(
      fetchQuestionGroupContext({ examId: 'exam_1', questionId: 'q1' }),
    ).resolves.toBeNull();
  });

  it('does not swallow real backend errors', async () => {
    mocks.invoke.mockRejectedValueOnce(new Error('PERMISSION_DENIED: qbank'));
    await expect(
      fetchQuestionGroupContext({ examId: 'exam_1', questionId: 'q1' }),
    ).rejects.toThrow(/PERMISSION_DENIED/);
  });
});

describe('group material search', () => {
  it('maps FTS hits to the owning child question', async () => {
    mocks.invoke.mockResolvedValueOnce([
      {
        question_id: 'q2',
        material_id: 'm1',
        group_id: 'g1',
        sub_label: '(2)',
        question_label: '2',
        group_order: 2,
        snippet: 'SHARED <b>PASSAGE</b>',
      },
    ]);
    const hits = await searchGroupMaterials('passage', { examId: 'exam_1', limit: 5 });
    expect(mocks.invoke).toHaveBeenCalledWith('qbank_search_group_materials', {
      keyword: 'passage',
      examId: 'exam_1',
      limit: 5,
    });
    expect(hits[0]).toMatchObject({ questionId: 'q2', materialId: 'm1', groupId: 'g1', subLabel: '(2)' });
  });
});

describe('flat group payloads', () => {
  it('normalizes QuestionGroupBundle and list rows (no group wrapper)', async () => {
    mocks.invoke.mockResolvedValueOnce({
      id: 'g1',
      exam_id: 'exam_1',
      group_type: 'reading',
      title: 'Passage A',
      source_type: 'imported',
      version: 2,
      publish_state: 'complete',
      materials: [{ id: 'm1', group_id: 'g1', exam_id: 'exam_1', order_index: 1, content_type: 'text', text: 'PASSAGE' }],
      siblings: [{ id: 'q1', content: 'STEM', answer: 'LEAK' }],
    });
    const bundle = await fetchQuestionGroup('g1');
    expect(bundle?.group.id).toBe('g1');
    expect(bundle?.materials[0].text).toBe('PASSAGE');
    expect((bundle?.siblings[0] as Record<string, unknown>).answer).toBeUndefined();

    mocks.invoke.mockResolvedValueOnce({
      groups: [{ id: 'g1', exam_id: 'exam_1', group_type: 'other', version: 1 }],
      total: 1,
      page: 1,
      page_size: 20,
      has_more: false,
    });
    const list = await listQuestionGroups('exam_1');
    expect(list?.groups[0].group.id).toBe('g1');
  });
});

describe('CAS and deletion contracts', () => {
  it('sends expected_group_version for material updates', async () => {
    mocks.invoke.mockResolvedValueOnce(true);
    await updateGroupMaterial('m1', { text: 'updated' }, 7);
    expect(mocks.invoke).toHaveBeenCalledWith('qbank_update_group_material', {
      request: {
        material_id: 'm1',
        update: { text: 'updated' },
        expected_version: 7,
      },
    });
  });

  it('requires an explicit preview version and confirmed flag for group deletion', async () => {
    mocks.invoke.mockResolvedValueOnce({
      group_id: 'g1',
      version: 5,
      material_ids: ['m1'],
      child_questions: [{ id: 'q1', sub_label: '(1)' }, { id: 'q2', sub_label: '(2)' }],
      total: 3,
    });
    const preview = await previewGroupDelete('g1');
    expect(preview).toMatchObject({ group_id: 'g1', version: 5, material_ids: ['m1'] });
    expect(preview?.child_questions.map((child) => child.id)).toEqual(['q1', 'q2']);

    mocks.invoke.mockResolvedValueOnce(true);
    await deleteQuestionGroup('g1', preview!.version, true);
    expect(mocks.invoke).toHaveBeenLastCalledWith('qbank_delete_question_group', {
      request: {
        group_id: 'g1',
        expected_version: 5,
        confirmed: true,
      },
    });
  });

  it('sends the group version CAS when deleting a single material', async () => {
    mocks.invoke.mockResolvedValueOnce(true);
    await deleteGroupMaterial('m1', 6);
    expect(mocks.invoke).toHaveBeenCalledWith('qbank_delete_group_material', {
      request: { material_id: 'm1', expected_version: 6 },
    });
  });

  it('classifies CAS conflicts and material-in-use errors', () => {
    expect(isGroupVersionConflictError('GROUP_VERSION_CONFLICT: expected 3 got 4')).toBe(true);
    expect(isGroupVersionConflictError('题目组版本冲突，请刷新后重试')).toBe(true);
    expect(isGroupVersionConflictError('PERMISSION_DENIED')).toBe(false);
    expect(isMaterialInUseError('MATERIAL_IN_USE: material m1')).toBe(true);
    expect(isMaterialInUseError('该材料仍被子题引用')).toBe(true);
    expect(isCommandUnavailableError('Command qbank_x not found')).toBe(true);
    expect(isCommandUnavailableError('INVALID_ARGS: pages')).toBe(false);
  });
});

describe('native bundle v2 adapters', () => {
  it('sends the frozen ExportRequest shape', async () => {
    mocks.invoke.mockResolvedValueOnce({
      schema: 'deep-student.qbank.bundle',
      format_version: 2,
      file_path: '/tmp/out.json',
      file_size: 1024,
      sha256: 'abc',
      round_trip_safe: true,
      groups: 1,
      materials: 2,
      questions: 3,
      standalone_questions: 1,
      assets: 2,
      tombstones: 0,
      warnings: [{ code: 'FIGURE_INLINED', message: 'figure asset inlined' }],
    });
    const result = await exportNativeBundleJson({ examId: 'exam_1' });
    expect(mocks.invoke).toHaveBeenCalledWith('qbank_export_bundle_json', {
      request: {
        exam_id: 'exam_1',
        file_path: null,
        include_assets: true,
        include_study_state: true,
      },
    });
    expect(result).toMatchObject({
      schema: 'deep-student.qbank.bundle',
      formatVersion: 2,
      filePath: '/tmp/out.json',
      roundTripSafe: true,
      questions: 3,
      standaloneQuestions: 1,
      assets: 2,
    });
    expect(result?.warnings).toEqual([
      { code: 'FIGURE_INLINED', message: 'figure asset inlined', refId: null },
    ]);
  });

  it('defaults imports to dry_run and parses committed/legacy/id_map', async () => {
    mocks.invoke.mockResolvedValueOnce({
      schema: 'deep-student.qbank.bundle',
      format_version: 2,
      dry_run: true,
      committed: false,
      legacy_v1: true,
      groups_created: 1,
      materials_created: 2,
      warnings: [{ code: 'LEGACY_V1', message: 'legacy bundle upgraded' }],
      id_map: [{ client_ref: 'g-ref', kind: 'group', new_id: 'qg_1' }],
    });
    const result = await importNativeBundleJson({
      bundle: { schema: 'deep-student.qbank.bundle', format_version: 2 },
      target: { kind: 'merge', exam_id: 'exam_1' },
    });
    expect(mocks.invoke).toHaveBeenCalledWith('qbank_import_bundle_json', {
      request: {
        bundle: { schema: 'deep-student.qbank.bundle', format_version: 2 },
        target: { kind: 'merge', exam_id: 'exam_1' },
        dry_run: true,
        include_study_state: true,
      },
    });
    expect(result).toMatchObject({
      dryRun: true,
      committed: false,
      legacyV1: true,
      groupsCreated: 1,
      materialsCreated: 2,
    });
    expect(result.warnings[0].code).toBe('LEGACY_V1');
    expect(result.idMap[0]).toEqual({ clientRef: 'g-ref', kind: 'group', newId: 'qg_1' });
  });
});
