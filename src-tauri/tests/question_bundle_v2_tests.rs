//! Issue #491：native JSON v2 题目包（deep-student.qbank.bundle）导入导出回归。
//!
//! 覆盖：
//! - 导出真实 blob 字节（base64）+ sha256/size 校验；缺失即明确错误
//! - `include_assets=false` 只允许作为显式 lossy 投影
//! - 导入 new_exam / merge / dry_run；单事务、ID 重映射
//! - 相同题干但不同 client_ref 的两道题都保留（不按题干去重）
//! - tombstones 不复活；merge 遇到同 source_ref 保留既有（用户编辑）
//! - 新版本 format_version 拒绝
//!
//! 夹具：真实迁移建库（MigrationCoordinator → DatabaseId::Vfs），临时目录与
//! 合成字节；不读写用户数据库，不访问网络。题组路径走真实
//! `VfsQuestionGroupRepo`（发布/读回），不 mock 存储层。

use std::sync::Arc;

use deep_student_lib::data_governance::migration::coordinator::MigrationCoordinator;
use deep_student_lib::data_governance::schema_registry::DatabaseId;
use deep_student_lib::question_bundle_service::{
    parse_bundle, BundleExportRequest, BundleImportRequest, BundleImportTarget, ParsedBundle,
    QuestionBundleService, BUNDLE_SCHEMA,
};
use deep_student_lib::vfs::repos::{
    CreateQuestionParams, MaterialContentType, NewMaterial, NewQuestionGroup,
    ProposedChildQuestion, ProposedGroupBundle, QuestionGroupType, QuestionImage, QuestionType,
    SourceType, VfsBlobRepo, VfsExamRepo, VfsFileRepo, VfsQuestionGroupRepo, VfsQuestionRepo,
};
use deep_student_lib::vfs::types::VfsCreateExamSheetParams;
use deep_student_lib::vfs::VfsDatabase;
use serde_json::json;
use tempfile::TempDir;

// ============================================================================
// 夹具
// ============================================================================

fn create_vfs_db() -> (TempDir, Arc<VfsDatabase>) {
    let dir = TempDir::new().expect("create VFS temp directory");
    let mut coordinator = MigrationCoordinator::new(dir.path().to_path_buf()).with_audit_db(None);
    coordinator
        .migrate_single(DatabaseId::Vfs)
        .expect("apply production VFS migrations");
    let db = VfsDatabase::new(dir.path()).expect("open migrated VFS database");
    (dir, Arc::new(db))
}

fn create_exam(db: &Arc<VfsDatabase>, name: &str) -> String {
    let conn = db.get_conn_safe().expect("vfs connection");
    let sheet = VfsExamRepo::create_exam_sheet_with_conn(
        &conn,
        VfsCreateExamSheetParams {
            exam_name: Some(name.to_string()),
            temp_id: format!("test_{}", nanoid::nanoid!(8)),
            metadata_json: json!({}),
            preview_json: json!({"questions": [], "total": 0}),
            status: "ready".to_string(),
            folder_id: None,
        },
    )
    .expect("create exam sheet");
    sheet.id
}

const IMAGE_BYTES: &[u8] = b"\x89PNG\r\n\x1a\nfake-image-bytes";

/// 建一道带真实 blob 图片的题，返回题目 id 与图片 hash。
fn create_question_with_image(
    db: &Arc<VfsDatabase>,
    exam_id: &str,
    content: &str,
    source_ref: Option<&str>,
) -> (String, String) {
    let bytes = IMAGE_BYTES;
    let blob =
        VfsBlobRepo::store_blob(db, bytes, Some("image/png"), Some("png")).expect("store blob");
    let file = VfsFileRepo::create_file(
        db,
        &blob.hash,
        "fig1.png",
        bytes.len() as i64,
        "image",
        Some("image/png"),
        Some(&blob.hash),
        None,
    )
    .expect("create vfs file");
    let image = QuestionImage {
        id: file.id,
        name: "fig1.png".to_string(),
        mime: "image/png".to_string(),
        hash: blob.hash.clone(),
    };
    let q = VfsQuestionRepo::create_question(
        db,
        &CreateQuestionParams {
            exam_id: exam_id.to_string(),
            card_id: None,
            question_label: None,
            content: content.to_string(),
            options: None,
            answer: Some("A".to_string()),
            explanation: Some("解析".to_string()),
            structured_data: None,
            question_type: Some(QuestionType::SingleChoice),
            difficulty: None,
            tags: Some(vec!["t1".to_string()]),
            source_type: Some(SourceType::Ocr),
            source_ref: source_ref.map(|s| s.to_string()),
            images: Some(vec![image]),
            parent_id: None,
        },
    )
    .expect("create question");
    (q.id, blob.hash)
}

fn export_once(db: &Arc<VfsDatabase>, exam_id: &str, include_assets: bool) -> serde_json::Value {
    let result = QuestionBundleService::export_json_v2(
        db,
        &BundleExportRequest {
            exam_id: exam_id.to_string(),
            file_path: None,
            include_assets,
            include_study_state: true,
        },
    )
    .expect("export bundle");
    serde_json::from_str(result.json.as_deref().expect("inline json")).expect("bundle json parses")
}

fn count_questions(db: &Arc<VfsDatabase>, exam_id: &str) -> usize {
    let conn = db.get_conn_safe().expect("vfs connection");
    VfsQuestionRepo::list_questions_with_conn(&conn, exam_id, &Default::default(), 1, 500)
        .expect("list questions")
        .questions
        .len()
}

// ============================================================================
// 导出
// ============================================================================

#[test]
fn export_embeds_real_blob_bytes_and_verifies_hash() {
    let (_dir, db) = create_vfs_db();
    let exam_id = create_exam(&db, "导出包");
    let (qid, hash) = create_question_with_image(&db, &exam_id, "题干", Some("src:1"));

    let bundle = export_once(&db, &exam_id, true);
    assert_eq!(bundle["schema"], BUNDLE_SCHEMA);
    assert_eq!(bundle["format_version"], 2);

    let standalone = bundle["standalone_questions"]
        .as_array()
        .expect("standalone array");
    assert_eq!(standalone.len(), 1);
    assert_eq!(standalone[0]["client_ref"], qid);
    assert_eq!(standalone[0]["question"]["images"][0]["asset_id"], hash);

    let assets = bundle["assets"].as_array().expect("assets array");
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0]["sha256"], hash);
    let data = assets[0]["data"].as_str().expect("asset data");
    assert!(!data.is_empty(), "assets 必须是真实 blob 字节的 base64");
    assert_eq!(
        assets[0]["size"].as_u64().unwrap() as usize,
        IMAGE_BYTES.len()
    );

    // 资产与 sha256 自洽
    use base64::Engine as _;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(data)
        .expect("base64");
    assert_eq!(
        VfsBlobRepo::compute_hash(&decoded),
        hash,
        "base64 内容必须与声明 hash 一致"
    );
}

#[test]
fn export_missing_blob_is_explicit_error() {
    let (_dir, db) = create_vfs_db();
    let exam_id = create_exam(&db, "缺资产");
    let (_qid, hash) = create_question_with_image(&db, &exam_id, "题干", None);

    // 模拟磁盘 blob 丢失
    let path = db
        .blobs_dir()
        .join(&hash[..2])
        .join(format!("{}.png", hash));
    std::fs::remove_file(&path).expect("remove blob to simulate missing asset");

    let err = QuestionBundleService::export_json_v2(
        &db,
        &BundleExportRequest {
            exam_id: exam_id.clone(),
            file_path: None,
            include_assets: true,
            include_study_state: true,
        },
    )
    .expect_err("缺失资产必须报错，不能静默导出残缺包");
    assert!(
        err.message.contains("asset_missing"),
        "错误信息应包含 asset_missing: {}",
        err.message
    );
}

#[test]
fn export_without_assets_is_explicitly_lossy() {
    let (_dir, db) = create_vfs_db();
    let exam_id = create_exam(&db, "不含资产");
    create_question_with_image(&db, &exam_id, "题干", None);

    let result = QuestionBundleService::export_json_v2(
        &db,
        &BundleExportRequest {
            exam_id,
            file_path: None,
            include_assets: false,
            include_study_state: false,
        },
    )
    .expect("export without assets");

    assert!(!result.round_trip_safe, "不含资产不能宣称可往返");
    assert!(result.assets == 0);
    assert!(result
        .warnings
        .iter()
        .any(|w| w.code == "assets_not_included"));
    let bundle: serde_json::Value = serde_json::from_str(result.json.as_deref().unwrap()).unwrap();
    assert_eq!(bundle["assets"].as_array().unwrap().len(), 0);
    assert!(bundle["standalone_questions"][0]["question"]["study"].is_null());
}

// ============================================================================
// 导入
// ============================================================================

#[test]
fn import_new_exam_roundtrips_assets_and_remaps_ids() {
    let (_dir, db) = create_vfs_db();
    let exam_id = create_exam(&db, "源试卷");
    let (qid, hash) = create_question_with_image(&db, &exam_id, "阅读后答题", Some("src:1"));
    let bundle = export_once(&db, &exam_id, true);

    let result = QuestionBundleService::import_json_v2(
        &db,
        &BundleImportRequest {
            bundle: bundle.clone(),
            target: BundleImportTarget::NewExam {
                name: Some("导入副本".to_string()),
            },
            dry_run: false,
            include_study_state: true,
        },
    )
    .expect("import bundle");

    assert!(result.committed);
    assert_eq!(result.questions_created, 1);
    let new_exam = result.exam_id.expect("new exam id");
    assert_ne!(new_exam, exam_id);
    assert_eq!(count_questions(&db, &new_exam), 1);

    // ID 重映射：新题不是原题 ID
    let mapped = result
        .id_map
        .iter()
        .find(|m| m.client_ref == qid)
        .expect("question id map");
    assert_ne!(mapped.new_id, qid);

    // 图片字节完整落地，hash 不变
    let conn = db.get_conn_safe().unwrap();
    let imported = VfsQuestionRepo::get_question_with_conn(&conn, &mapped.new_id)
        .unwrap()
        .unwrap();
    assert_eq!(imported.images.len(), 1);
    assert_eq!(imported.images[0].hash, hash);
    assert!(imported.images[0].name.ends_with(".png"));
    let blob_path = db
        .blobs_dir()
        .join(&hash[..2])
        .join(format!("{}.png", hash));
    assert!(blob_path.exists(), "导入后 blob 文件必须存在");
}

#[test]
fn import_dry_run_writes_nothing() {
    let (_dir, db) = create_vfs_db();
    let exam_id = create_exam(&db, "源");
    create_question_with_image(&db, &exam_id, "题干", None);
    let bundle = export_once(&db, &exam_id, true);

    let result = QuestionBundleService::import_json_v2(
        &db,
        &BundleImportRequest {
            bundle,
            target: BundleImportTarget::NewExam { name: None },
            dry_run: true,
            include_study_state: true,
        },
    )
    .expect("dry run import");

    assert!(!result.committed);
    assert!(result.exam_id.is_none(), "dry_run 不应创建题目集");
    assert_eq!(result.questions_created, 1, "dry_run 仍报告将写入的数量");

    let conn = db.get_conn_safe().unwrap();
    let exam_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM exam_sheets", [], |r| r.get(0))
        .unwrap();
    assert_eq!(exam_count, 1, "dry_run 不应新建题目集行");
}

#[test]
fn import_rejects_newer_format_version() {
    let (_dir, db) = create_vfs_db();
    let mut bundle = json!({
        "schema": BUNDLE_SCHEMA,
        "format_version": 3,
        "exported_at": "2026-10-21T00:00:00Z",
        "exam": {"name": "x"},
        "sources": [], "groups": [], "materials": [], "children": [],
        "standalone_questions": [], "order": [], "tombstones": [], "assets": []
    });
    bundle["format_version"] = json!(3);

    let err = QuestionBundleService::import_json_v2(
        &db,
        &BundleImportRequest {
            bundle,
            target: BundleImportTarget::NewExam { name: None },
            dry_run: true,
            include_study_state: true,
        },
    )
    .expect_err("更高格式版本必须拒绝");
    assert!(err.message.contains("格式版本"), "{}", err.message);
}

#[test]
fn import_keeps_duplicate_identical_questions() {
    let (_dir, db) = create_vfs_db();
    let exam_id = create_exam(&db, "源");
    let bundle = json!({
        "schema": BUNDLE_SCHEMA,
        "format_version": 2,
        "exported_at": "2026-10-21T00:00:00Z",
        "exam": {"name": "重复题干"},
        "sources": [], "groups": [], "materials": [], "children": [],
        "standalone_questions": [
            {"client_ref": "a", "question": {"content": "同一题干", "answer": "A"}},
            {"client_ref": "b", "question": {"content": "同一题干", "answer": "A"}}
        ],
        "order": ["a", "b"],
        "tombstones": [], "assets": []
    });

    let result = QuestionBundleService::import_json_v2(
        &db,
        &BundleImportRequest {
            bundle,
            target: BundleImportTarget::NewExam { name: None },
            dry_run: false,
            include_study_state: true,
        },
    )
    .expect("import duplicates");

    assert_eq!(
        result.questions_created, 2,
        "相同题干不同 client_ref 必须各自保留"
    );
    assert_eq!(result.questions_skipped, 0);
    let exam = result.exam_id.unwrap();
    assert_eq!(count_questions(&db, &exam), 2);
}

#[test]
fn import_tombstones_are_not_resurrected() {
    let (_dir, db) = create_vfs_db();
    let bundle = json!({
        "schema": BUNDLE_SCHEMA,
        "format_version": 2,
        "exported_at": "2026-10-21T00:00:00Z",
        "exam": {"name": "墓碑"},
        "sources": [], "groups": [], "materials": [], "children": [],
        "standalone_questions": [
            {"client_ref": "keep", "question": {"content": "保留", "answer": "A"}},
            {"client_ref": "gone", "question": {"content": "已删", "answer": "B"}}
        ],
        "order": ["keep", "gone"],
        "tombstones": [{"kind": "question", "ref_id": "gone", "deleted_at": "2026-10-20T00:00:00Z"}],
        "assets": []
    });

    let result = QuestionBundleService::import_json_v2(
        &db,
        &BundleImportRequest {
            bundle,
            target: BundleImportTarget::NewExam { name: None },
            dry_run: false,
            include_study_state: true,
        },
    )
    .expect("import with tombstones");

    assert_eq!(result.questions_created, 1);
    assert_eq!(result.questions_skipped, 1);
    assert_eq!(result.tombstones_applied, 1);
    let exam = result.exam_id.unwrap();
    let conn = db.get_conn_safe().unwrap();
    let rows = VfsQuestionRepo::list_questions_with_conn(&conn, &exam, &Default::default(), 1, 50)
        .unwrap();
    assert_eq!(rows.questions.len(), 1);
    assert!(rows.questions[0].content.contains("保留"));
}

#[test]
fn import_merge_keeps_existing_edited_question() {
    let (_dir, db) = create_vfs_db();
    let exam_id = create_exam(&db, "目标");
    let (qid, _hash) =
        create_question_with_image(&db, &exam_id, "用户后来改过的题干", Some("src:dup"));

    let bundle = json!({
        "schema": BUNDLE_SCHEMA,
        "format_version": 2,
        "exported_at": "2026-10-21T00:00:00Z",
        "exam": {"name": "包内"},
        "sources": [], "groups": [], "materials": [], "children": [],
        "standalone_questions": [
            {"client_ref": "x", "question": {
                "content": "包内旧题干", "answer": "A", "source_ref": "src:dup"
            }}
        ],
        "order": ["x"],
        "tombstones": [], "assets": []
    });

    let result = QuestionBundleService::import_json_v2(
        &db,
        &BundleImportRequest {
            bundle,
            target: BundleImportTarget::Merge {
                exam_id: exam_id.clone(),
            },
            dry_run: false,
            include_study_state: true,
        },
    )
    .expect("merge import");

    assert!(result.committed);
    assert_eq!(result.questions_created, 0);
    assert_eq!(result.questions_skipped, 1);
    assert!(result
        .warnings
        .iter()
        .any(|w| w.code == "existing_source_ref_skipped"));

    let conn = db.get_conn_safe().unwrap();
    let kept = VfsQuestionRepo::get_question_with_conn(&conn, &qid)
        .unwrap()
        .unwrap();
    assert_eq!(kept.content, "用户后来改过的题干", "merge 不得覆盖用户编辑");
    assert_eq!(count_questions(&db, &exam_id), 1);
}

#[test]
fn import_merge_requires_existing_exam() {
    let (_dir, db) = create_vfs_db();
    let bundle = json!({
        "schema": BUNDLE_SCHEMA,
        "format_version": 2,
        "exported_at": "2026-10-21T00:00:00Z",
        "exam": {"name": "x"},
        "sources": [], "groups": [], "materials": [], "children": [],
        "standalone_questions": [{"client_ref": "a", "question": {"content": "q", "answer": "A"}}],
        "order": ["a"], "tombstones": [], "assets": []
    });
    let err = QuestionBundleService::import_json_v2(
        &db,
        &BundleImportRequest {
            bundle,
            target: BundleImportTarget::Merge {
                exam_id: "exam_missing".to_string(),
            },
            dry_run: true,
            include_study_state: true,
        },
    )
    .unwrap_err();
    assert!(err.message.contains("不存在"), "{}", err.message);
}

#[test]
fn import_legacy_v1_array_still_works() {
    let (_dir, db) = create_vfs_db();
    let result = QuestionBundleService::import_json_v2(
        &db,
        &BundleImportRequest {
            bundle: json!([
                {"content": "旧格式题", "answer": "A"},
                {"content": "旧格式题2", "answer": "B"}
            ]),
            target: BundleImportTarget::NewExam {
                name: Some("v1".to_string()),
            },
            dry_run: false,
            include_study_state: true,
        },
    )
    .expect("v1 import");

    assert!(result.legacy_v1);
    assert_eq!(result.questions_created, 2);
    assert_eq!(result.id_map.len(), 2);
}

#[test]
fn parse_roundtrip_of_exported_bundle_stays_v2() {
    let (_dir, db) = create_vfs_db();
    let exam_id = create_exam(&db, "往返");
    create_question_with_image(&db, &exam_id, "题干", None);
    let bundle = export_once(&db, &exam_id, true);
    match parse_bundle(bundle).expect("exported bundle parses") {
        ParsedBundle::V2(b) => assert_eq!(b.schema, BUNDLE_SCHEMA),
        _ => panic!("导出的包必须是 v2"),
    }
}

// ============================================================================
// 题组（真实 VfsQuestionGroupRepo 接线）
// ============================================================================

fn child_params(
    exam_id: &str,
    content: &str,
    answer: &str,
    sub_label: &str,
    group_order: i32,
) -> ProposedChildQuestion {
    ProposedChildQuestion {
        client_ref: Some(format!("ref-{}", group_order)),
        question: CreateQuestionParams {
            exam_id: exam_id.to_string(),
            card_id: None,
            question_label: None,
            content: content.to_string(),
            options: Some(vec![
                deep_student_lib::vfs::repos::QuestionOption {
                    key: "A".into(),
                    content: "甲".into(),
                },
                deep_student_lib::vfs::repos::QuestionOption {
                    key: "B".into(),
                    content: "乙".into(),
                },
            ]),
            answer: Some(answer.to_string()),
            explanation: Some("解析".to_string()),
            structured_data: None,
            question_type: Some(QuestionType::SingleChoice),
            difficulty: None,
            tags: None,
            source_type: Some(SourceType::Ocr),
            source_ref: Some(format!("src:child:{}", group_order)),
            images: None,
            parent_id: None,
        },
        sub_label: Some(sub_label.to_string()),
        group_order,
    }
}

fn publish_reading_group(db: &Arc<VfsDatabase>, exam_id: &str, with_material: bool) -> String {
    let conn = db.get_conn_safe().unwrap();
    let materials = if with_material {
        vec![NewMaterial {
            group_id: String::new(),
            order_index: 1,
            content_type: MaterialContentType::Text,
            title: None,
            text: "共享阅读材料正文".to_string(),
            images: Vec::new(),
            source_ref: Some("doc:1".to_string()),
            page_index: Some(1),
            bbox: None,
        }]
    } else {
        Vec::new()
    };
    let bundle = ProposedGroupBundle {
        group: NewQuestionGroup {
            exam_id: exam_id.to_string(),
            group_type: QuestionGroupType::Reading,
            title: Some("Passage A".to_string()),
            source_type: Some(SourceType::Ocr),
            source_ref: Some("doc:1".to_string()),
            import_baseline: None,
            publish_state: None,
        },
        materials,
        children: vec![
            child_params(exam_id, "子题一", "A", "(1)", 1),
            child_params(exam_id, "子题二", "B", "(2)", 2),
        ],
    };
    let published = VfsQuestionGroupRepo::publish_group_bundle_with_conn(&conn, &bundle)
        .expect("publish group");
    published.group.id
}

#[test]
fn export_and_import_keep_group_material_and_children() {
    let (_dir, db) = create_vfs_db();
    let exam_id = create_exam(&db, "英语阅读");
    let group_id = publish_reading_group(&db, &exam_id, true);

    let exported = QuestionBundleService::export_json_v2(
        &db,
        &BundleExportRequest {
            exam_id: exam_id.clone(),
            file_path: None,
            include_assets: true,
            include_study_state: true,
        },
    )
    .expect("export grouped exam");
    assert_eq!(exported.groups, 1);
    assert_eq!(exported.materials, 1);
    assert_eq!(exported.questions, 2);
    assert_eq!(exported.standalone_questions, 0);
    assert!(exported.round_trip_safe);

    let bundle: serde_json::Value =
        serde_json::from_str(exported.json.as_deref().unwrap()).unwrap();
    assert_eq!(bundle["groups"][0]["group_ref"], group_id);
    assert_eq!(bundle["materials"][0]["order_index"], 1);
    assert_eq!(bundle["materials"][0]["group_ref"], group_id);
    assert_eq!(bundle["children"][0]["group_order"], 1);
    assert_eq!(bundle["children"][1]["group_order"], 2);

    let imported = QuestionBundleService::import_json_v2(
        &db,
        &BundleImportRequest {
            bundle,
            target: BundleImportTarget::NewExam {
                name: Some("导入的阅读卷".to_string()),
            },
            dry_run: false,
            include_study_state: true,
        },
    )
    .expect("import grouped bundle");
    assert!(imported.committed);
    assert_eq!(imported.groups_created, 1);
    assert_eq!(imported.materials_created, 1);
    assert_eq!(imported.questions_created, 2);

    let new_exam = imported.exam_id.expect("new exam");
    let conn = db.get_conn_safe().unwrap();
    let list = VfsQuestionGroupRepo::list_groups_with_conn(&conn, &new_exam, 1, 20).unwrap();
    assert_eq!(list.groups.len(), 1, "导入后必须有一个可见题组");
    let new_group_id = list.groups[0].id.clone();
    let materials = VfsQuestionGroupRepo::get_materials_with_conn(&conn, &new_group_id).unwrap();
    assert_eq!(materials.len(), 1);
    assert_eq!(materials[0].text, "共享阅读材料正文");
    let siblings = VfsQuestionGroupRepo::get_siblings_with_conn(&conn, &new_group_id).unwrap();
    assert_eq!(siblings.len(), 2);
    assert_eq!(siblings[0].group_order, Some(1));
    assert_eq!(siblings[1].sub_label.as_deref(), Some("(2)"));

    // ID 全部重映射：新组/新材料/新子题都和源不同
    assert_ne!(new_group_id, group_id);
    assert!(imported
        .id_map
        .iter()
        .filter(|m| m.kind == "material")
        .all(|m| !m.new_id.is_empty()));
}

#[test]
fn import_group_failure_rolls_back_whole_exam() {
    let (_dir, db) = create_vfs_db();
    // 第一组合法；第二组 reading 但没有任何材料 → repo 拒绝，整包必须回滚
    let bundle = json!({
        "schema": BUNDLE_SCHEMA,
        "format_version": 2,
        "exported_at": "2026-10-21T00:00:00Z",
        "exam": {"name": "半组测试"},
        "sources": [],
        "groups": [
            {"group_ref": "g1", "group_type": "reading", "title": null, "source_ref": null, "version": 1},
            {"group_ref": "g2", "group_type": "reading", "title": null, "source_ref": null, "version": 1}
        ],
        "materials": [
            {"material_ref": "m1", "group_ref": "g1", "order_index": 1,
             "content_type": "text", "title": null, "text": "材料一", "images": [],
             "source_id": null, "source_ref": null, "page_index": null, "bbox": null}
        ],
        "children": [
            {"client_ref": "c1", "group_ref": "g1", "group_order": 1, "sub_label": "(1)",
             "question": {"content": "组1题", "answer": "A"}},
            {"client_ref": "c2", "group_ref": "g2", "group_order": 1, "sub_label": "(1)",
             "question": {"content": "组2题", "answer": "B"}}
        ],
        "standalone_questions": [],
        "order": ["c1", "c2"],
        "tombstones": [],
        "assets": []
    });

    let err = QuestionBundleService::import_json_v2(
        &db,
        &BundleImportRequest {
            bundle,
            target: BundleImportTarget::NewExam {
                name: Some("半组".to_string()),
            },
            dry_run: false,
            include_study_state: true,
        },
    )
    .expect_err("缺少材料的 reading 组必须让整包回滚");
    assert!(
        err.message.contains("材料") || err.message.contains("题组"),
        "{}",
        err.message
    );

    let conn = db.get_conn_safe().unwrap();
    let exams: i64 = conn
        .query_row("SELECT COUNT(*) FROM exam_sheets", [], |r| r.get(0))
        .unwrap();
    let groups: i64 = conn
        .query_row("SELECT COUNT(*) FROM question_groups", [], |r| r.get(0))
        .unwrap();
    let questions: i64 = conn
        .query_row("SELECT COUNT(*) FROM questions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(exams, 0, "失败导入不得留下新建题目集");
    assert_eq!(groups, 0, "失败导入不得留下半组");
    assert_eq!(questions, 0, "失败导入不得留下孤立子题");
}

#[test]
fn export_rejects_when_group_material_image_blob_missing() {
    let (_dir, db) = create_vfs_db();
    let exam_id = create_exam(&db, "材料带图");
    let bytes = IMAGE_BYTES;
    let blob = VfsBlobRepo::store_blob(&db, bytes, Some("image/png"), Some("png")).unwrap();
    let group_id = {
        let conn = db.get_conn_safe().unwrap();
        let file = VfsFileRepo::create_file(
            &db,
            &blob.hash,
            "mat.png",
            bytes.len() as i64,
            "image",
            Some("image/png"),
            Some(&blob.hash),
            None,
        )
        .unwrap();
        let bundle = ProposedGroupBundle {
            group: NewQuestionGroup {
                exam_id: exam_id.clone(),
                group_type: QuestionGroupType::SharedCondition,
                title: None,
                source_type: Some(SourceType::Ocr),
                source_ref: None,
                import_baseline: None,
                publish_state: None,
            },
            materials: vec![NewMaterial {
                group_id: String::new(),
                order_index: 1,
                content_type: MaterialContentType::Figure,
                title: None,
                text: String::new(),
                images: vec![QuestionImage {
                    id: file.id,
                    name: "mat.png".to_string(),
                    mime: "image/png".to_string(),
                    hash: blob.hash.clone(),
                }],
                source_ref: None,
                page_index: None,
                bbox: None,
            }],
            children: vec![child_params(&exam_id, "看图题", "A", "(1)", 1)],
        };
        VfsQuestionGroupRepo::publish_group_bundle_with_conn(&conn, &bundle)
            .expect("publish group with figure material")
            .group
            .id
    };
    assert!(!group_id.is_empty());

    std::fs::remove_file(
        db.blobs_dir()
            .join(&blob.hash[..2])
            .join(format!("{}.png", blob.hash)),
    )
    .expect("remove material image blob");

    let err = QuestionBundleService::export_json_v2(
        &db,
        &BundleExportRequest {
            exam_id,
            file_path: None,
            include_assets: true,
            include_study_state: true,
        },
    )
    .expect_err("材料图片缺失必须明确报错");
    assert!(err.message.contains("asset_missing"), "{}", err.message);
}

// ============================================================================
// tombstones / 学习状态
// ============================================================================

#[test]
fn export_includes_tombstones_for_soft_deleted_questions() {
    let (_dir, db) = create_vfs_db();
    let exam_id = create_exam(&db, "删除记录");
    let (keep_id, _h) = create_question_with_image(&db, &exam_id, "保留题", None);
    let (gone_id, _h2) = create_question_with_image(&db, &exam_id, "删除题", None);
    {
        let conn = db.get_conn_safe().unwrap();
        VfsQuestionRepo::delete_question_with_conn(&conn, &gone_id).expect("soft delete");
    }

    let exported = QuestionBundleService::export_json_v2(
        &db,
        &BundleExportRequest {
            exam_id: exam_id.clone(),
            file_path: None,
            include_assets: true,
            include_study_state: true,
        },
    )
    .expect("export with tombstones");
    assert_eq!(exported.tombstones, 1, "软删题必须出现在 tombstones");

    let bundle: serde_json::Value =
        serde_json::from_str(exported.json.as_deref().unwrap()).unwrap();
    assert_eq!(bundle["tombstones"][0]["kind"], "question");
    assert_eq!(bundle["tombstones"][0]["ref_id"], gone_id);
    let standalone = bundle["standalone_questions"].as_array().unwrap();
    assert_eq!(standalone.len(), 1);
    assert_eq!(standalone[0]["client_ref"], keep_id);

    let imported = QuestionBundleService::import_json_v2(
        &db,
        &BundleImportRequest {
            bundle,
            target: BundleImportTarget::NewExam {
                name: Some("删除记录副本".to_string()),
            },
            dry_run: false,
            include_study_state: true,
        },
    )
    .expect("import with tombstones");
    assert_eq!(imported.questions_created, 1);
    assert!(imported.tombstones_applied >= 1);
}

#[test]
fn import_restores_study_counters_on_new_questions() {
    let (_dir, db) = create_vfs_db();
    let bundle = json!({
        "schema": BUNDLE_SCHEMA,
        "format_version": 2,
        "exported_at": "2026-10-21T00:00:00Z",
        "exam": {"name": "学习状态"},
        "sources": [], "groups": [], "materials": [], "children": [],
        "standalone_questions": [
            {"client_ref": "a", "question": {
                "content": "带学习状态的题", "answer": "A",
                "study": {
                    "status": "review", "user_answer": "B", "is_correct": false,
                    "user_note": "笔记", "is_favorite": true, "is_bookmarked": false,
                    "attempt_count": 3, "correct_count": 2,
                    "last_attempt_at": "2026-10-20T10:00:00Z"
                }
            }}
        ],
        "order": ["a"],
        "tombstones": [],
        "assets": []
    });

    let result = QuestionBundleService::import_json_v2(
        &db,
        &BundleImportRequest {
            bundle,
            target: BundleImportTarget::NewExam {
                name: Some("学习状态副本".to_string()),
            },
            dry_run: false,
            include_study_state: true,
        },
    )
    .expect("import study state");
    let exam = result.exam_id.unwrap();
    let new_id = result.id_map[0].new_id.clone();

    let conn = db.get_conn_safe().unwrap();
    let q = VfsQuestionRepo::get_question_with_conn(&conn, &new_id)
        .unwrap()
        .expect("question exists");
    assert_eq!(q.attempt_count, 3);
    assert_eq!(q.correct_count, 2);
    assert_eq!(q.user_note.as_deref(), Some("笔记"));
    assert!(q.is_favorite);
    assert_eq!(q.status.as_str(), "review");
    assert_eq!(q.last_attempt_at.as_deref(), Some("2026-10-20T10:00:00Z"));

    // 统计表同事务重算，total_attempts 与恢复的计数一致
    let stats = VfsQuestionRepo::refresh_stats_with_conn(&conn, &exam).unwrap();
    assert_eq!(stats.total_attempts, 3);
    assert_eq!(stats.total_correct, 2);
}
