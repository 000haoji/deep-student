-- ============================================================================
-- V20261021: 题组（question groups）与有序材料（materials）
-- ============================================================================
-- 背景：Issue #491 要求阅读材料/数学共享条件不能塞回单题正文，也不能复用
--       questions.parent_id（那里是变式题语义）。首版模型：一层平级题组 +
--       多个有序材料 + 独立可答子题；不支持任意嵌套与跨题库全局材料库。
-- 设计文档：docs/dev/qbank-question-groups.md
--
-- 关键约束：
--   1. 组头不是 question，不产生作答记录、不计题数。
--   2. questions 新增 nullable group_id / sub_label / group_order；
--      旧数据三列均为 NULL，行为完全不变。
--   3. 子题与材料必须和组属于同一 exam_id，且组未软删（触发器硬校验，
--      RowSync 乱序到达时由同步层受控暂存，不是靠放开约束）。
--   4. 组内顺序唯一（partial unique index，仅对未软删行生效）。
--   5. 可见性：publish_state='complete' 且 manifest 中所有 child/material id
--      均已落地、存活、且 group_version 与组一致（见 repo 的可见性谓词）。
--      单靠 publish_state 不够——RowSync 可能先收到组行再收到子题。
--   6. 材料正文只存一份（question_group_materials），子题不重复存材料正文。
--
-- 同步：两表均带 device_id/local_version/updated_at/deleted_at 并注册
--       __change_log 触发器（模板 V20260720__mastery_events_sync.sql）；
--       分类见 src/data_governance/sync/classification.rs。
--
-- 幂等性：CREATE TABLE/INDEX IF NOT EXISTS；触发器 DROP IF EXISTS 后重建。
-- 注意：本迁移包含 4 个 ALTER TABLE ADD COLUMN（无法写成 IF NOT EXISTS）。
-- 幂等由迁移协调器保证：`MigrationCoordinator` 在执行前解析未记录迁移的
-- ADD COLUMN，若目标列已全部存在则预标记该迁移完成（coordinator.rs 的
-- duplicate-column 预修复）；且迁移逐条执行（set_grouped(false)）。
-- ============================================================================

-- @danger-ack: unique_constraint reason="组内顺序唯一必须由 DB 保证；partial unique index 仅约束未软删行，不改变存量数据"

-- ----------------------------------------------------------------------------
-- 1. 题组主表
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS question_groups (
    id TEXT PRIMARY KEY NOT NULL,                 -- 格式：qg_{nanoid(10)}
    exam_id TEXT NOT NULL,                        -- 所属题目集
    group_type TEXT NOT NULL DEFAULT 'other'
        CHECK (group_type IN ('reading', 'shared_condition', 'other')),
    title TEXT,
    source_type TEXT NOT NULL DEFAULT 'ocr',      -- ocr | imported | ai_generated | manual
    source_ref TEXT,                              -- 稳定来源身份（页/区块/来源文档）
    import_baseline_json TEXT,                    -- 字段级导入基线
    version INTEGER NOT NULL DEFAULT 1,           -- 关系/材料变更 CAS 版本（单调递增）
    publish_state TEXT NOT NULL DEFAULT 'complete'
        CHECK (publish_state IN ('staging', 'complete')),
    -- RowSync 完整性 manifest：预期成员 id 列表。全部落地且版本一致才可见。
    expected_child_ids_json TEXT NOT NULL DEFAULT '[]',
    expected_material_ids_json TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    device_id TEXT,
    local_version INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_question_groups_exam
    ON question_groups(exam_id, deleted_at);
CREATE INDEX IF NOT EXISTS idx_question_groups_publish
    ON question_groups(exam_id, publish_state) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_question_groups_source_ref
    ON question_groups(exam_id, source_ref) WHERE source_ref IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_question_groups_local_version
    ON question_groups(local_version);
CREATE INDEX IF NOT EXISTS idx_question_groups_device_version
    ON question_groups(device_id, local_version);

-- ----------------------------------------------------------------------------
-- 2. 题组材料（一组多个，有序；首版不做跨组复用）
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS question_group_materials (
    id TEXT PRIMARY KEY NOT NULL,                 -- 格式：qm_{nanoid(10)}
    group_id TEXT NOT NULL,
    exam_id TEXT NOT NULL,
    order_index INTEGER NOT NULL CHECK (order_index > 0), -- 组内顺序，从 1 开始连续
    content_type TEXT NOT NULL DEFAULT 'text'
        CHECK (content_type IN ('text', 'formula', 'figure', 'mixed')),
    title TEXT,
    text TEXT NOT NULL DEFAULT '',
    images_json TEXT NOT NULL DEFAULT '[]',
    source_ref TEXT,
    page_index INTEGER,
    bbox_json TEXT,
    group_version INTEGER NOT NULL DEFAULT 1,     -- 写入时的组版本（可见性校验）
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    device_id TEXT,
    local_version INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (group_id) REFERENCES question_groups(id)
);

CREATE INDEX IF NOT EXISTS idx_qgm_group_order
    ON question_group_materials(group_id, order_index) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_qgm_exam
    ON question_group_materials(exam_id, deleted_at);
CREATE INDEX IF NOT EXISTS idx_qgm_local_version
    ON question_group_materials(local_version);
CREATE INDEX IF NOT EXISTS idx_qgm_device_version
    ON question_group_materials(device_id, local_version);

-- @danger-ack: unique_constraint reason="同一组内 order_index 唯一（仅未软删行），防止并发插入重复顺序"
CREATE UNIQUE INDEX IF NOT EXISTS uidx_qgm_group_order
    ON question_group_materials(group_id, order_index) WHERE deleted_at IS NULL;

-- ----------------------------------------------------------------------------
-- 3. questions 扩展列（旧题保持 NULL，语义不变）
-- ----------------------------------------------------------------------------
ALTER TABLE questions ADD COLUMN group_id TEXT;
ALTER TABLE questions ADD COLUMN sub_label TEXT;
ALTER TABLE questions ADD COLUMN group_order INTEGER;
-- 写入时的组版本：让 RowSync 能判断"材料/子题是否属于当前组版本"，
-- 旧版本内容齐全不代表新版本完整。
-- @danger-ack: add_not_null_column reason="新列带 DEFAULT 0，存量行自动回填 0，不需要数据改写，也不改变旧题行为（group_id 为 NULL）"
ALTER TABLE questions ADD COLUMN group_version INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_questions_group
    ON questions(group_id, group_order) WHERE group_id IS NOT NULL;

-- @danger-ack: unique_constraint reason="同组内 group_order 唯一（仅未软删子题），避免重复顺序"
CREATE UNIQUE INDEX IF NOT EXISTS uidx_questions_group_order
    ON questions(group_id, group_order)
    WHERE group_id IS NOT NULL AND deleted_at IS NULL;

-- @danger-ack: unique_constraint reason="同组内 sub_label 唯一（非空时），防止两小题被导入成同一小问号"
CREATE UNIQUE INDEX IF NOT EXISTS uidx_questions_group_sub_label
    ON questions(group_id, sub_label)
    WHERE group_id IS NOT NULL AND sub_label IS NOT NULL AND deleted_at IS NULL;

-- ----------------------------------------------------------------------------
-- 3.1 组必须挂在存在且未软删的题目集上（exam_id 无法用 ALTER 加 FK）
-- ----------------------------------------------------------------------------
DROP TRIGGER IF EXISTS trg_question_groups_exam_integrity_insert;
CREATE TRIGGER trg_question_groups_exam_integrity_insert
BEFORE INSERT ON question_groups
WHEN NOT EXISTS (
    SELECT 1 FROM exam_sheets e WHERE e.id = NEW.exam_id AND e.deleted_at IS NULL
)
BEGIN
    SELECT RAISE(ABORT, 'QBANK_GROUP_EXAM_MISSING: question group exam must exist and be live');
END;

DROP TRIGGER IF EXISTS trg_question_groups_exam_integrity_update;
CREATE TRIGGER trg_question_groups_exam_integrity_update
BEFORE UPDATE OF exam_id ON question_groups
WHEN NOT EXISTS (
    SELECT 1 FROM exam_sheets e WHERE e.id = NEW.exam_id AND e.deleted_at IS NULL
)
BEGIN
    SELECT RAISE(ABORT, 'QBANK_GROUP_EXAM_MISSING: question group exam must exist and be live');
END;

-- ----------------------------------------------------------------------------
-- 3.2 子题关系列必须完整或全空（SQLite 无法给存量表加表级 CHECK）
--     group_id 非空时 group_order 必填（>0），否则视为损坏关系。
-- ----------------------------------------------------------------------------
DROP TRIGGER IF EXISTS trg_questions_group_fields_insert;
CREATE TRIGGER trg_questions_group_fields_insert
BEFORE INSERT ON questions
WHEN (
        NEW.group_id IS NULL
        AND (NEW.sub_label IS NOT NULL OR NEW.group_order IS NOT NULL OR NEW.group_version <> 0)
     )
     OR (
        NEW.group_id IS NOT NULL
        AND (NEW.group_order IS NULL OR NEW.group_order <= 0 OR NEW.group_version <= 0)
     )
BEGIN
    SELECT RAISE(ABORT, 'QBANK_GROUP_FIELDS_INCONSISTENT: group_id/sub_label/group_order/group_version must form a complete relation');
END;

DROP TRIGGER IF EXISTS trg_questions_group_fields_update;
CREATE TRIGGER trg_questions_group_fields_update
BEFORE UPDATE OF group_id, sub_label, group_order, group_version ON questions
WHEN (
        NEW.group_id IS NULL
        AND (NEW.sub_label IS NOT NULL OR NEW.group_order IS NOT NULL OR NEW.group_version <> 0)
     )
     OR (
        NEW.group_id IS NOT NULL
        AND (NEW.group_order IS NULL OR NEW.group_order <= 0 OR NEW.group_version <= 0)
     )
BEGIN
    SELECT RAISE(ABORT, 'QBANK_GROUP_FIELDS_INCONSISTENT: group_id/sub_label/group_order/group_version must form a complete relation');
END;

-- ----------------------------------------------------------------------------
-- 4. 完整性触发器：同 exam、组存在且未软删
--   - 仅 INSERT 或 UPDATE OF group_id 时校验；组软删后同步旧子题行不会被动阻断。
-- ----------------------------------------------------------------------------
DROP TRIGGER IF EXISTS trg_qgm_group_integrity_insert;
CREATE TRIGGER trg_qgm_group_integrity_insert
BEFORE INSERT ON question_group_materials
WHEN NOT EXISTS (
    SELECT 1 FROM question_groups g
    WHERE g.id = NEW.group_id AND g.exam_id = NEW.exam_id AND g.deleted_at IS NULL
)
BEGIN
    SELECT RAISE(ABORT, 'QBANK_GROUP_PARENT_MISSING: material group must exist, share exam_id and be live');
END;

DROP TRIGGER IF EXISTS trg_qgm_group_integrity_update;
CREATE TRIGGER trg_qgm_group_integrity_update
BEFORE UPDATE OF group_id, exam_id ON question_group_materials
WHEN NOT EXISTS (
    SELECT 1 FROM question_groups g
    WHERE g.id = NEW.group_id AND g.exam_id = NEW.exam_id AND g.deleted_at IS NULL
)
BEGIN
    SELECT RAISE(ABORT, 'QBANK_GROUP_PARENT_MISSING: material group must exist, share exam_id and be live');
END;

DROP TRIGGER IF EXISTS trg_questions_group_integrity_insert;
CREATE TRIGGER trg_questions_group_integrity_insert
BEFORE INSERT ON questions
WHEN NEW.group_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM question_groups g
    WHERE g.id = NEW.group_id AND g.exam_id = NEW.exam_id AND g.deleted_at IS NULL
)
BEGIN
    SELECT RAISE(ABORT, 'QBANK_GROUP_PARENT_MISSING: child question group must exist, share exam_id and be live');
END;

DROP TRIGGER IF EXISTS trg_questions_group_integrity_update;
CREATE TRIGGER trg_questions_group_integrity_update
BEFORE UPDATE OF group_id, exam_id ON questions
WHEN NEW.group_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM question_groups g
    WHERE g.id = NEW.group_id AND g.exam_id = NEW.exam_id AND g.deleted_at IS NULL
)
BEGIN
    SELECT RAISE(ABORT, 'QBANK_GROUP_PARENT_MISSING: child question group must exist, share exam_id and be live');
END;

-- ----------------------------------------------------------------------------
-- 5. 材料全文检索（独立 FTS，不向 questions_fts 塞组头/材料）
-- ----------------------------------------------------------------------------
CREATE VIRTUAL TABLE IF NOT EXISTS question_group_materials_fts USING fts5(
    text,
    title,
    content='question_group_materials',
    content_rowid='rowid',
    tokenize='unicode61'
);

DROP TRIGGER IF EXISTS trg_qgm_fts_insert;
CREATE TRIGGER trg_qgm_fts_insert
AFTER INSERT ON question_group_materials
WHEN NEW.deleted_at IS NULL
BEGIN
    INSERT INTO question_group_materials_fts(rowid, text, title)
    VALUES (NEW.rowid, NEW.text, COALESCE(NEW.title, ''));
END;

DROP TRIGGER IF EXISTS trg_qgm_fts_update;
CREATE TRIGGER trg_qgm_fts_update
AFTER UPDATE ON question_group_materials
BEGIN
    -- external-content FTS 必须用 'delete' 命令携带 OLD 值才能移除旧 token；
    -- 直接 DELETE FROM ..._fts 只删 rowid 映射，不会清理倒排索引（实测旧 token 仍可命中）。
    INSERT INTO question_group_materials_fts(question_group_materials_fts, rowid, text, title)
    SELECT 'delete', OLD.rowid, OLD.text, COALESCE(OLD.title, '')
    WHERE OLD.deleted_at IS NULL;
    INSERT INTO question_group_materials_fts(rowid, text, title)
    SELECT NEW.rowid, NEW.text, COALESCE(NEW.title, '')
    WHERE NEW.deleted_at IS NULL;
END;

DROP TRIGGER IF EXISTS trg_qgm_fts_delete;
CREATE TRIGGER trg_qgm_fts_delete
AFTER DELETE ON question_group_materials
BEGIN
    INSERT INTO question_group_materials_fts(question_group_materials_fts, rowid, text, title)
    SELECT 'delete', OLD.rowid, OLD.text, COALESCE(OLD.title, '')
    WHERE OLD.deleted_at IS NULL;
END;

-- ----------------------------------------------------------------------------
-- 6. 同步变更日志触发器（record_id = 行自身 id）
-- ----------------------------------------------------------------------------
DROP TRIGGER IF EXISTS trg__change_log_question_groups_insert;
CREATE TRIGGER trg__change_log_question_groups_insert
AFTER INSERT ON question_groups
BEGIN
    INSERT INTO __change_log (table_name, record_id, operation, changed_at)
    VALUES ('question_groups', NEW.id, 'INSERT', datetime('now'));
END;

DROP TRIGGER IF EXISTS trg__change_log_question_groups_update;
CREATE TRIGGER trg__change_log_question_groups_update
AFTER UPDATE ON question_groups
BEGIN
    INSERT INTO __change_log (table_name, record_id, operation, changed_at)
    VALUES ('question_groups', NEW.id, 'UPDATE', datetime('now'));
END;

DROP TRIGGER IF EXISTS trg__change_log_question_groups_delete;
CREATE TRIGGER trg__change_log_question_groups_delete
AFTER DELETE ON question_groups
BEGIN
    INSERT INTO __change_log (table_name, record_id, operation, changed_at)
    VALUES ('question_groups', OLD.id, 'DELETE', datetime('now'));
END;

DROP TRIGGER IF EXISTS trg__change_log_qgm_insert;
CREATE TRIGGER trg__change_log_qgm_insert
AFTER INSERT ON question_group_materials
BEGIN
    INSERT INTO __change_log (table_name, record_id, operation, changed_at)
    VALUES ('question_group_materials', NEW.id, 'INSERT', datetime('now'));
END;

DROP TRIGGER IF EXISTS trg__change_log_qgm_update;
CREATE TRIGGER trg__change_log_qgm_update
AFTER UPDATE ON question_group_materials
BEGIN
    INSERT INTO __change_log (table_name, record_id, operation, changed_at)
    VALUES ('question_group_materials', NEW.id, 'UPDATE', datetime('now'));
END;

DROP TRIGGER IF EXISTS trg__change_log_qgm_delete;
CREATE TRIGGER trg__change_log_qgm_delete
AFTER DELETE ON question_group_materials
BEGIN
    INSERT INTO __change_log (table_name, record_id, operation, changed_at)
    VALUES ('question_group_materials', OLD.id, 'DELETE', datetime('now'));
END;

-- ----------------------------------------------------------------------------
-- 7. RowSync 乱序受控暂存（本地辅助表，不参与同步）
--    当子题/材料先于组到达时写入此处；每批应用结束按依赖顺序重放。
--    有界尝试，保留 last_error，禁止静默丢弃。
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS __question_group_sync_staging (
    table_name TEXT NOT NULL,
    record_id TEXT NOT NULL,
    operation TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    payload_hash TEXT NOT NULL DEFAULT '',
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    first_seen_at TEXT NOT NULL,
    last_attempt_at TEXT,
    PRIMARY KEY (table_name, record_id)
);
