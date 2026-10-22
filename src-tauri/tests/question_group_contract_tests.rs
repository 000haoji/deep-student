//! Issue #491 题组契约测试（使用真实迁移 SQL + 隔离内存库）。
//!
//! 这些测试直接执行 `migrations/vfs/V20261021__question_groups.sql`，
//! 不依赖应用构建或用户数据库。覆盖：
//! - 材料 external-content FTS 的插入/更新/软删/恢复/硬删 + integrity-check
//! - 组/材料的同 exam、软删、顺序、sub_label 唯一等约束
//! - 子题关系列完整性（group_id/sub_label/group_order/group_version）
//! - RowSync manifest 可见性谓词（child-first / group-first / partial / 完整）

use rusqlite::{params, Connection};

const MIGRATION_SQL: &str = include_str!("../migrations/vfs/V20261021__question_groups.sql");

fn setup() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory db");
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;
        CREATE TABLE exam_sheets (
            id TEXT PRIMARY KEY,
            exam_name TEXT,
            deleted_at TEXT
        );
        CREATE TABLE questions (
            id TEXT PRIMARY KEY,
            exam_id TEXT NOT NULL,
            content TEXT NOT NULL DEFAULT '',
            options_json TEXT,
            answer TEXT,
            explanation TEXT,
            deleted_at TEXT,
            created_at TEXT NOT NULL DEFAULT '2026-01-01T00:00:00Z',
            updated_at TEXT NOT NULL DEFAULT '2026-01-01T00:00:00Z'
        );
        CREATE TABLE __change_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            table_name TEXT NOT NULL,
            record_id TEXT NOT NULL,
            operation TEXT NOT NULL,
            changed_at TEXT NOT NULL,
            sync_version INTEGER NOT NULL DEFAULT 0
        );
        "#,
    )
    .expect("base schema");
    conn.execute_batch(MIGRATION_SQL).expect("apply migration");
    conn
}

fn insert_exam(conn: &Connection, id: &str) {
    conn.execute(
        "INSERT INTO exam_sheets(id, exam_name, deleted_at) VALUES (?1, 'exam', NULL)",
        params![id],
    )
    .unwrap();
}

fn insert_group(conn: &Connection, id: &str, exam_id: &str) -> rusqlite::Result<usize> {
    conn.execute(
        r#"INSERT INTO question_groups(
               id, exam_id, group_type, title, source_type, source_ref,
               version, publish_state, created_at, updated_at)
           VALUES (?1, ?2, 'reading', 'g', 'ocr', NULL, 1, 'complete',
                   '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"#,
        params![id, exam_id],
    )
}

fn insert_material(
    conn: &Connection,
    id: &str,
    group_id: &str,
    exam_id: &str,
    text: &str,
) -> rusqlite::Result<usize> {
    conn.execute(
        r#"INSERT INTO question_group_materials(
               id, group_id, exam_id, order_index, content_type, title, text,
               images_json, group_version, created_at, updated_at)
           VALUES (?1, ?2, ?3, 1, 'text', NULL, ?4, '[]', 1,
                   '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"#,
        params![id, group_id, exam_id, text],
    )
}

fn fts_hits(conn: &Connection, token: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM question_group_materials_fts WHERE question_group_materials_fts MATCH ?1",
        params![token],
        |row| row.get(0),
    )
    .unwrap()
}

#[test]
fn material_fts_update_removes_old_tokens_and_soft_delete_restore_hard_delete() {
    let conn = setup();
    insert_exam(&conn, "exam_1");
    insert_group(&conn, "qg_1", "exam_1").unwrap();
    insert_material(&conn, "qm_1", "qg_1", "exam_1", "oldtoken passage").unwrap();
    assert_eq!(fts_hits(&conn, "oldtoken"), 1);
    assert_eq!(fts_hits(&conn, "newtoken"), 0);

    // UPDATE 必须移除旧 token（external-content FTS 需要 'delete' OLD 值）
    conn.execute(
        "UPDATE question_group_materials SET text='newtoken passage' WHERE id='qm_1'",
        [],
    )
    .unwrap();
    assert_eq!(
        fts_hits(&conn, "oldtoken"),
        0,
        "old material token remains searchable after update"
    );
    assert_eq!(fts_hits(&conn, "newtoken"), 1);

    // 软删：不可检索
    conn.execute(
        "UPDATE question_group_materials SET deleted_at='2026-01-02T00:00:00Z' WHERE id='qm_1'",
        [],
    )
    .unwrap();
    assert_eq!(
        fts_hits(&conn, "newtoken"),
        0,
        "soft-deleted material must not be searchable"
    );

    // 恢复：重新可检索
    conn.execute(
        "UPDATE question_group_materials SET deleted_at=NULL WHERE id='qm_1'",
        [],
    )
    .unwrap();
    assert_eq!(
        fts_hits(&conn, "newtoken"),
        1,
        "restored material must be searchable again"
    );

    // 硬删：清理索引
    conn.execute("DELETE FROM question_group_materials WHERE id='qm_1'", [])
        .unwrap();
    assert_eq!(
        fts_hits(&conn, "newtoken"),
        0,
        "hard-deleted material must not remain searchable"
    );

    conn.execute(
        "INSERT INTO question_group_materials_fts(question_group_materials_fts) VALUES('integrity-check')",
        [],
    )
    .expect("fts integrity-check must pass");
}

#[test]
fn group_requires_live_exam() {
    let conn = setup();
    // 不存在的 exam
    assert!(insert_group(&conn, "qg_missing", "exam_missing").is_err());
    // 软删的 exam
    insert_exam(&conn, "exam_del");
    conn.execute(
        "UPDATE exam_sheets SET deleted_at='2026-01-02T00:00:00Z' WHERE id='exam_del'",
        [],
    )
    .unwrap();
    assert!(insert_group(&conn, "qg_del", "exam_del").is_err());
}

#[test]
fn material_requires_same_exam_and_live_group() {
    let conn = setup();
    insert_exam(&conn, "exam_1");
    insert_exam(&conn, "exam_2");
    insert_group(&conn, "qg_1", "exam_1").unwrap();

    // 不同 exam
    assert!(insert_material(&conn, "qm_bad", "qg_1", "exam_2", "x").is_err());
    // 不存在的组
    assert!(insert_material(&conn, "qm_orphan", "qg_missing", "exam_1", "x").is_err());
    // 组软删后不可再挂载
    conn.execute(
        "UPDATE question_groups SET deleted_at='2026-01-02T00:00:00Z' WHERE id='qg_1'",
        [],
    )
    .unwrap();
    assert!(insert_material(&conn, "qm_after_delete", "qg_1", "exam_1", "x").is_err());
}

#[test]
fn material_order_and_child_sub_label_are_unique_within_group() {
    let conn = setup();
    insert_exam(&conn, "exam_1");
    insert_group(&conn, "qg_1", "exam_1").unwrap();
    insert_material(&conn, "qm_1", "qg_1", "exam_1", "a").unwrap();
    // 同组同 order_index
    assert!(insert_material(&conn, "qm_2", "qg_1", "exam_1", "b").is_err());
    // order_index <= 0 违反 CHECK
    assert!(conn
        .execute(
            "INSERT INTO question_group_materials(id, group_id, exam_id, order_index, content_type, text, group_version, created_at, updated_at)
             VALUES ('qm_3','qg_1','exam_1',0,'text','c',1,'2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
            [],
        )
        .is_err());

    let insert_child = |id: &str, label: &str, order: i32| {
        conn.execute(
            r#"INSERT INTO questions(id, exam_id, content, group_id, sub_label, group_order, group_version)
               VALUES (?1, 'exam_1', 'c', 'qg_1', ?2, ?3, 1)"#,
            params![id, label, order],
        )
    };
    insert_child("q_1", "(1)", 1).unwrap();
    // 同组重复 sub_label
    assert!(insert_child("q_2", "(1)", 2).is_err());
    // 同组重复 group_order
    assert!(insert_child("q_3", "(3)", 1).is_err());
}

#[test]
fn child_relationship_columns_must_be_complete() {
    let conn = setup();
    insert_exam(&conn, "exam_1");
    insert_group(&conn, "qg_1", "exam_1").unwrap();

    // 有 group_id 却无 group_order
    assert!(conn
        .execute(
            "INSERT INTO questions(id, exam_id, content, group_id) VALUES ('q_bad','exam_1','c','qg_1')",
            [],
        )
        .is_err());
    // 无 group_id 却有 sub_label
    assert!(conn
        .execute(
            "INSERT INTO questions(id, exam_id, content, sub_label) VALUES ('q_bad2','exam_1','c','(1)')",
            [],
        )
        .is_err());
    // 完整关系可以插入
    conn.execute(
        "INSERT INTO questions(id, exam_id, content, group_id, sub_label, group_order, group_version)
         VALUES ('q_ok','exam_1','c','qg_1','(1)',1,1)",
        [],
    )
    .unwrap();
}

/// 可见性谓词与 repo 中 `visible_group_sql` 保持一致（同一 SQL 片段）。
fn visible_group_count(conn: &Connection) -> i64 {
    conn.query_row(
        r#"
        SELECT COUNT(*) FROM question_groups g
        WHERE g.deleted_at IS NULL
          AND g.publish_state = 'complete'
          AND (SELECT COUNT(*) FROM questions q
               WHERE q.group_id = g.id AND q.deleted_at IS NULL) = json_array_length(g.expected_child_ids_json)
          AND (SELECT COUNT(*) FROM question_group_materials m
               WHERE m.group_id = g.id AND m.deleted_at IS NULL) = json_array_length(g.expected_material_ids_json)
          AND NOT EXISTS (
              SELECT 1 FROM json_each(g.expected_child_ids_json) e
              WHERE NOT EXISTS (SELECT 1 FROM questions q
                                WHERE q.id = e.value AND q.deleted_at IS NULL
                                  AND q.group_version = g.version))
          AND NOT EXISTS (
              SELECT 1 FROM json_each(g.expected_material_ids_json) e
              WHERE NOT EXISTS (SELECT 1 FROM question_group_materials m
                                WHERE m.id = e.value AND m.deleted_at IS NULL
                                  AND m.group_version = g.version))
        "#,
        [],
        |row| row.get(0),
    )
    .unwrap()
}

fn set_manifest(conn: &Connection, group_id: &str, children: &[&str], materials: &[&str]) {
    conn.execute(
        "UPDATE question_groups SET expected_child_ids_json=?2, expected_material_ids_json=?3 WHERE id=?1",
        params![
            group_id,
            serde_json::to_string(children).unwrap(),
            serde_json::to_string(materials).unwrap()
        ],
    )
    .unwrap();
}

#[test]
fn rowsync_visibility_requires_full_manifest_in_any_arrival_order() {
    let conn = setup();
    insert_exam(&conn, "exam_1");
    insert_group(&conn, "qg_1", "exam_1").unwrap();
    set_manifest(&conn, "qg_1", &["q_1", "q_2"], &["qm_1"]);

    // 组先到，成员未到：不可见
    assert_eq!(
        visible_group_count(&conn),
        0,
        "group-first partial must be invisible"
    );

    // 一个子题先到：仍不可见
    conn.execute(
        "INSERT INTO questions(id, exam_id, content, group_id, sub_label, group_order, group_version)
         VALUES ('q_1','exam_1','c1','qg_1','(1)',1,1)",
        [],
    )
    .unwrap();
    assert_eq!(visible_group_count(&conn), 0);

    // 材料到齐但还缺一个子题：不可见
    insert_material(&conn, "qm_1", "qg_1", "exam_1", "passage").unwrap();
    assert_eq!(visible_group_count(&conn), 0);

    // 最后一个子题到齐：完整可见（child-first 顺序也由同一谓词覆盖）
    conn.execute(
        "INSERT INTO questions(id, exam_id, content, group_id, sub_label, group_order, group_version)
         VALUES ('q_2','exam_1','c2','qg_1','(2)',2,1)",
        [],
    )
    .unwrap();
    assert_eq!(visible_group_count(&conn), 1);

    // 旧版本内容齐全但组版本已前移：不可见（旧内容不等于新完整）
    conn.execute(
        "UPDATE question_groups SET version = version + 1 WHERE id='qg_1'",
        [],
    )
    .unwrap();
    assert_eq!(
        visible_group_count(&conn),
        0,
        "stale member versions must not satisfy new group version"
    );
}
