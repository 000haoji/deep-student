use std::path::{Path, PathBuf};
use std::fs;
use std::sync::Mutex;
use anyhow::{Context, Result};
use rusqlite::{Connection, params, OptionalExtension};
use chrono::{Utc, DateTime};
use crate::models::{MistakeItem, ChatMessage, Statistics, SubjectConfig, SubjectPrompts, DocumentTask, TaskStatus, AnkiCard, SubLibrary, CreateSubLibraryRequest, UpdateSubLibraryRequest, ReviewAnalysisItem};

// Re-export for external use
// pub use std::sync::MutexGuard; // Removed unused import

const CURRENT_DB_VERSION: u32 = 10;

pub struct Database {
    conn: Mutex<Connection>,
    db_path: PathBuf,
}

impl Database {
    /// Get a reference to the underlying connection for batch operations
    pub fn conn(&self) -> &Mutex<Connection> {
        &self.conn
    }

    /// 创建新的数据库连接并初始化/迁移数据库
    pub fn new(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("创建数据库目录失败: {:?}", parent))?;
        }

        let conn = Connection::open(db_path)
            .with_context(|| format!("打开数据库连接失败: {:?}", db_path))?;

        let db = Database { conn: Mutex::new(conn), db_path: db_path.to_path_buf() };
        db.initialize_schema()?;
        Ok(db)
    }

    fn initialize_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "BEGIN;
            CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY NOT NULL
            );
            CREATE TABLE IF NOT EXISTS mistakes (
                id TEXT PRIMARY KEY,
                subject TEXT NOT NULL,
                created_at TEXT NOT NULL,
                question_images TEXT NOT NULL, -- JSON数组
                analysis_images TEXT NOT NULL, -- JSON数组
                user_question TEXT NOT NULL,
                ocr_text TEXT NOT NULL,
                tags TEXT NOT NULL, -- JSON数组
                mistake_type TEXT NOT NULL,
                status TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS chat_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                mistake_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                thinking_content TEXT, -- 可选的思维链内容
                FOREIGN KEY(mistake_id) REFERENCES mistakes(id) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS review_analyses (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                subject TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                mistake_ids TEXT NOT NULL, -- JSON数组，关联的错题ID
                consolidated_input TEXT NOT NULL, -- 合并后的输入内容
                user_question TEXT NOT NULL,
                status TEXT NOT NULL,
                tags TEXT NOT NULL, -- JSON数组
                analysis_type TEXT NOT NULL DEFAULT 'consolidated_review'
            );
            CREATE TABLE IF NOT EXISTS review_chat_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                review_analysis_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                thinking_content TEXT, -- 思维链内容
                rag_sources TEXT, -- RAG来源信息，JSON格式
                FOREIGN KEY(review_analysis_id) REFERENCES review_analyses(id) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS subject_configs (
                id TEXT PRIMARY KEY,
                subject_name TEXT NOT NULL UNIQUE,
                display_name TEXT NOT NULL,
                description TEXT NOT NULL,
                is_enabled INTEGER NOT NULL DEFAULT 1,
                prompts TEXT NOT NULL, -- JSON格式存储SubjectPrompts
                mistake_types TEXT NOT NULL, -- JSON数组
                default_tags TEXT NOT NULL, -- JSON数组
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS document_tasks (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                original_document_name TEXT NOT NULL,
                segment_index INTEGER NOT NULL,
                content_segment TEXT NOT NULL,
                status TEXT NOT NULL CHECK(status IN ('Pending', 'Processing', 'Streaming', 'Completed', 'Failed', 'Truncated', 'Cancelled')),
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                error_message TEXT,
                subject_name TEXT NOT NULL,
                anki_generation_options_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS anki_cards (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL REFERENCES document_tasks(id) ON DELETE CASCADE,
                front TEXT NOT NULL,
                back TEXT NOT NULL,
                tags_json TEXT DEFAULT '[]',
                images_json TEXT DEFAULT '[]',
                is_error_card INTEGER NOT NULL DEFAULT 0,
                error_content TEXT,
                card_order_in_task INTEGER DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );
            CREATE INDEX IF NOT EXISTS idx_document_tasks_document_id ON document_tasks(document_id);
            CREATE INDEX IF NOT EXISTS idx_document_tasks_status ON document_tasks(status);
            CREATE INDEX IF NOT EXISTS idx_anki_cards_task_id ON anki_cards(task_id);
            CREATE INDEX IF NOT EXISTS idx_anki_cards_is_error_card ON anki_cards(is_error_card);
            COMMIT;"
        )?;

        let current_version: u32 = conn.query_row(
            "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1",
            [],
            |row| row.get(0),
        ).optional()?.unwrap_or(0);

        if current_version < CURRENT_DB_VERSION {
            // 迁移逻辑
            if current_version < 2 {
                self.migrate_v1_to_v2(&conn)?;
            }
            if current_version < 3 {
                self.migrate_v2_to_v3(&conn)?;
            }
            if current_version < 4 {
                self.migrate_v3_to_v4(&conn)?;
            }
            if current_version < 5 {
                self.migrate_v4_to_v5(&conn)?;
            }
            if current_version < 6 {
                self.migrate_v5_to_v6(&conn)?;
            }
            if current_version < 7 {
                self.migrate_v6_to_v7(&conn)?;
            }
            if current_version < 8 {
                self.migrate_v7_to_v8(&conn)?;
            }
            if current_version < 9 {
                self.migrate_v8_to_v9(&conn)?;
            }
            if current_version < 10 {
                self.migrate_v9_to_v10(&conn)?;
            }
            conn.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
                params![CURRENT_DB_VERSION],
            )?;
        }

        // 调用思维链列迁移函数
        self.migrate_add_thinking_column(&conn)?;
        
        // 调用RAG来源信息列迁移函数
        self.migrate_add_rag_sources_column(&conn)?;
        
        // 调用科目配置prompts迁移函数
        self.migrate_subject_config_prompts(&conn)?;
        Ok(())
    }

    fn migrate_add_thinking_column(&self, conn: &rusqlite::Connection) -> anyhow::Result<()> {
        let mut stmt = conn.prepare("PRAGMA table_info(chat_messages);")?;
        let column_exists = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(Result::ok)
            .any(|name| name == "thinking_content");

        if !column_exists {
            conn.execute(
                "ALTER TABLE chat_messages ADD COLUMN thinking_content TEXT;",
                [],
            )?;
            println!("✅ SQLite: thinking_content 列已添加");
        }
        Ok(())
    }

    fn migrate_add_rag_sources_column(&self, conn: &rusqlite::Connection) -> anyhow::Result<()> {
        let mut stmt = conn.prepare("PRAGMA table_info(chat_messages);")?;
        let column_exists = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(Result::ok)
            .any(|name| name == "rag_sources");

        if !column_exists {
            conn.execute(
                "ALTER TABLE chat_messages ADD COLUMN rag_sources TEXT;",
                [],
            )?;
            println!("✅ SQLite: rag_sources 列已添加");
        }
        Ok(())
    }

    fn migrate_subject_config_prompts(&self, conn: &rusqlite::Connection) -> anyhow::Result<()> {
        // 检查是否存在subject_configs表
        let table_exists = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='subject_configs';")?
            .query_map([], |_| Ok(()))?
            .any(|_| true);
        
        if !table_exists {
            println!("⏭️ SQLite: subject_configs表不存在，跳过prompts迁移");
            return Ok(());
        }

        // 获取所有现有的科目配置
        let mut stmt = conn.prepare("SELECT id, prompts FROM subject_configs")?;
        let configs: Vec<(String, String)> = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?.collect::<rusqlite::Result<Vec<_>>>()?;

        let mut updated_count = 0;
        
        for (id, prompts_json) in configs {
            // 尝试解析现有的prompts JSON
            match serde_json::from_str::<SubjectPrompts>(&prompts_json) {
                Ok(_) => {
                    // 如果能成功解析，说明字段已经完整，跳过
                    continue;
                },
                Err(_) => {
                    // 解析失败，尝试修复
                    println!("🔧 修复科目配置prompts: {}", id);
                    
                    // 尝试解析为旧格式（没有consolidated_review_prompt字段的JSON）
                    let mut prompts_value: serde_json::Value = match serde_json::from_str(&prompts_json) {
                        Ok(v) => v,
                        Err(e) => {
                            println!("❌ 跳过无法解析的prompts JSON: {} - {}", id, e);
                            continue;
                        }
                    };
                    
                    // 检查是否缺少consolidated_review_prompt字段
                    if !prompts_value.get("consolidated_review_prompt").is_some() {
                        // 添加默认的consolidated_review_prompt字段
                        prompts_value["consolidated_review_prompt"] = serde_json::Value::String(
                            "你是一个资深老师，请仔细阅读以下学生提交的多道错题的详细信息（包括题目原文、原始提问和历史交流）。请基于所有这些信息，对学生提出的总体回顾问题进行全面、深入的分析和解答。请注意识别错题间的关联，总结共性问题，并给出针对性的学习建议。".to_string()
                        );
                        
                        // 更新数据库中的prompts字段
                        let updated_prompts_json = serde_json::to_string(&prompts_value)?;
                        conn.execute(
                            "UPDATE subject_configs SET prompts = ?1 WHERE id = ?2",
                            params![updated_prompts_json, id],
                        )?;
                        
                        updated_count += 1;
                        println!("✅ 已修复科目配置prompts: {}", id);
                    }
                }
            }
        }
        
        if updated_count > 0 {
            println!("✅ SQLite: 已修复 {} 个科目配置的prompts字段", updated_count);
        } else {
            println!("✅ SQLite: 所有科目配置的prompts字段都已是最新格式");
        }
        
        Ok(())
    }

    fn migrate_v1_to_v2(&self, conn: &rusqlite::Connection) -> anyhow::Result<()> {
        println!("🔄 数据库迁移: v1 -> v2 (添加Anki增强功能表)");
        
        // 检查document_tasks表是否已存在
        let document_tasks_exists = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='document_tasks';")?
            .query_map([], |_| Ok(()))?
            .any(|_| true);
        
        if !document_tasks_exists {
            conn.execute(
                "CREATE TABLE document_tasks (
                    id TEXT PRIMARY KEY,
                    document_id TEXT NOT NULL,
                    original_document_name TEXT NOT NULL,
                    segment_index INTEGER NOT NULL,
                    content_segment TEXT NOT NULL,
                    status TEXT NOT NULL CHECK(status IN ('Pending', 'Processing', 'Streaming', 'Completed', 'Failed', 'Truncated', 'Cancelled')),
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    error_message TEXT,
                    subject_name TEXT NOT NULL,
                    anki_generation_options_json TEXT NOT NULL
                );",
                [],
            )?;
            println!("✅ 创建document_tasks表");
        }
        
        // 检查anki_cards表是否已存在
        let anki_cards_exists = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='anki_cards';")?
            .query_map([], |_| Ok(()))?
            .any(|_| true);
        
        if !anki_cards_exists {
            conn.execute(
                "CREATE TABLE anki_cards (
                    id TEXT PRIMARY KEY,
                    task_id TEXT NOT NULL REFERENCES document_tasks(id) ON DELETE CASCADE,
                    front TEXT NOT NULL,
                    back TEXT NOT NULL,
                    tags_json TEXT DEFAULT '[]',
                    images_json TEXT DEFAULT '[]',
                    is_error_card INTEGER NOT NULL DEFAULT 0,
                    error_content TEXT,
                    card_order_in_task INTEGER DEFAULT 0,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );",
                [],
            )?;
            println!("✅ 创建anki_cards表");
        }
        
        // 创建索引
        conn.execute("CREATE INDEX IF NOT EXISTS idx_document_tasks_document_id ON document_tasks(document_id);", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_document_tasks_status ON document_tasks(status);", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_anki_cards_task_id ON anki_cards(task_id);", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_anki_cards_is_error_card ON anki_cards(is_error_card);", [])?;
        
        println!("✅ 数据库迁移完成: v1 -> v2");
        Ok(())
    }

    fn migrate_v2_to_v3(&self, conn: &rusqlite::Connection) -> anyhow::Result<()> {
        println!("🔄 数据库迁移: v2 -> v3 (添加RAG配置表)");
        
        // 检查rag_configurations表是否已存在
        let rag_config_exists = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='rag_configurations';")?
            .query_map([], |_| Ok(()))?
            .any(|_| true);
        
        if !rag_config_exists {
            conn.execute(
                "CREATE TABLE rag_configurations (
                    id TEXT PRIMARY KEY,
                    chunk_size INTEGER NOT NULL DEFAULT 512,
                    chunk_overlap INTEGER NOT NULL DEFAULT 50,
                    chunking_strategy TEXT NOT NULL DEFAULT 'fixed_size',
                    min_chunk_size INTEGER NOT NULL DEFAULT 20,
                    default_top_k INTEGER NOT NULL DEFAULT 5,
                    default_rerank_enabled INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );",
                [],
            )?;
            println!("✅ 创建rag_configurations表");
            
            // 插入默认配置
            let now = Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO rag_configurations (id, chunk_size, chunk_overlap, chunking_strategy, min_chunk_size, default_top_k, default_rerank_enabled, created_at, updated_at)
                 VALUES ('default', 512, 50, 'fixed_size', 20, 5, 0, ?1, ?2)",
                params![now, now],
            )?;
            println!("✅ 插入默认RAG配置");
        }
        
        println!("✅ 数据库迁移完成: v2 -> v3");
        Ok(())
    }

    fn migrate_v3_to_v4(&self, _conn: &rusqlite::Connection) -> anyhow::Result<()> {
        println!("📦 开始数据库迁移 v3 -> v4: 添加RAG来源信息支持");
        
        // v3到v4的迁移主要通过migrate_add_rag_sources_column处理
        // 这里可以添加其他v4特有的迁移逻辑
        
        println!("✅ 数据库迁移 v3 -> v4 完成");
        Ok(())
    }

    fn migrate_v4_to_v5(&self, conn: &rusqlite::Connection) -> anyhow::Result<()> {
        println!("📦 开始数据库迁移 v4 -> v5: 升级回顾分析表结构");
        
        // 强制创建review_analyses和review_chat_messages表（如果不存在）
        conn.execute(
            "CREATE TABLE IF NOT EXISTS review_analyses (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                subject TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                mistake_ids TEXT NOT NULL,
                consolidated_input TEXT NOT NULL,
                user_question TEXT NOT NULL,
                status TEXT NOT NULL,
                tags TEXT NOT NULL,
                analysis_type TEXT NOT NULL DEFAULT 'consolidated_review'
            )",
            [],
        )?;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS review_chat_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                review_analysis_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                thinking_content TEXT,
                rag_sources TEXT,
                FOREIGN KEY(review_analysis_id) REFERENCES review_analyses(id) ON DELETE CASCADE
            )",
            [],
        )?;
        
        println!("✅ 强制创建了review_analyses和review_chat_messages表");
        
        // 迁移旧的review_sessions到新的review_analyses
        self.migrate_review_sessions_to_review_analyses(conn)?;
        
        println!("✅ 数据库迁移 v4 -> v5 完成");
        Ok(())
    }

    fn migrate_v5_to_v6(&self, conn: &rusqlite::Connection) -> anyhow::Result<()> {
        println!("📦 开始数据库迁移 v5 -> v6: 修复回顾分析表结构");
        
        // 强制重新创建review_analyses和review_chat_messages表，确保schema正确
        conn.execute("DROP TABLE IF EXISTS review_chat_messages", [])?;
        conn.execute("DROP TABLE IF EXISTS review_analyses", [])?;
        
        conn.execute(
            "CREATE TABLE review_analyses (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                subject TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                mistake_ids TEXT NOT NULL,
                consolidated_input TEXT NOT NULL,
                user_question TEXT NOT NULL,
                status TEXT NOT NULL,
                tags TEXT NOT NULL,
                analysis_type TEXT NOT NULL DEFAULT 'consolidated_review'
            )",
            [],
        )?;
        
        conn.execute(
            "CREATE TABLE review_chat_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                review_analysis_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                thinking_content TEXT,
                rag_sources TEXT,
                FOREIGN KEY(review_analysis_id) REFERENCES review_analyses(id) ON DELETE CASCADE
            )",
            [],
        )?;
        
        println!("✅ 重新创建了review_analyses和review_chat_messages表");
        println!("✅ 数据库迁移 v5 -> v6 完成");
        Ok(())
    }

    fn migrate_v6_to_v7(&self, conn: &rusqlite::Connection) -> anyhow::Result<()> {
        println!("📦 开始数据库迁移 v6 -> v7: 添加错题总结字段");
        
        // 为mistakes表添加新的总结字段
        conn.execute(
            "ALTER TABLE mistakes ADD COLUMN mistake_summary TEXT",
            [],
        )?;
        
        conn.execute(
            "ALTER TABLE mistakes ADD COLUMN user_error_analysis TEXT",
            [],
        )?;
        
        println!("✅ 已为mistakes表添加mistake_summary和user_error_analysis字段");
        println!("✅ 数据库迁移 v6 -> v7 完成");
        Ok(())
    }

    fn migrate_v7_to_v8(&self, conn: &rusqlite::Connection) -> anyhow::Result<()> {
        println!("📦 开始数据库迁移 v7 -> v8: 添加模板支持字段");
        
        // 为anki_cards表添加扩展字段和模板ID字段
        let add_extra_fields = conn.execute(
            "ALTER TABLE anki_cards ADD COLUMN extra_fields_json TEXT DEFAULT '{}'",
            [],
        );
        
        let add_template_id = conn.execute(
            "ALTER TABLE anki_cards ADD COLUMN template_id TEXT",
            [],
        );
        
        match (add_extra_fields, add_template_id) {
            (Ok(_), Ok(_)) => {
                println!("✅ 已为anki_cards表添加extra_fields_json和template_id字段");
            }
            (Err(e1), Err(e2)) => {
                println!("⚠️ 添加字段时遇到错误，可能字段已存在: {} / {}", e1, e2);
            }
            (Ok(_), Err(e)) => {
                println!("⚠️ 添加template_id字段时遇到错误，可能字段已存在: {}", e);
            }
            (Err(e), Ok(_)) => {
                println!("⚠️ 添加extra_fields_json字段时遇到错误，可能字段已存在: {}", e);
            }
        }
        
        // 创建自定义模板表
        conn.execute(
            "CREATE TABLE IF NOT EXISTS custom_anki_templates (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                description TEXT,
                author TEXT,
                version TEXT NOT NULL DEFAULT '1.0.0',
                preview_front TEXT NOT NULL,
                preview_back TEXT NOT NULL,
                note_type TEXT NOT NULL DEFAULT 'Basic',
                fields_json TEXT NOT NULL DEFAULT '[]',
                generation_prompt TEXT NOT NULL,
                front_template TEXT NOT NULL,
                back_template TEXT NOT NULL,
                css_style TEXT NOT NULL,
                field_extraction_rules_json TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                is_active INTEGER NOT NULL DEFAULT 1,
                is_built_in INTEGER NOT NULL DEFAULT 0
            );",
            [],
        )?;
        
        // 创建模板表索引
        conn.execute("CREATE INDEX IF NOT EXISTS idx_custom_anki_templates_is_active ON custom_anki_templates(is_active);", [])?;
        conn.execute("CREATE INDEX IF NOT EXISTS idx_custom_anki_templates_is_built_in ON custom_anki_templates(is_built_in);", [])?;
        
        println!("✅ 已创建custom_anki_templates表");
        println!("✅ 数据库迁移 v7 -> v8 完成");
        Ok(())
    }
    
    // 自定义模板管理方法
    
    /// 创建自定义模板
    pub fn create_custom_template(&self, request: &crate::models::CreateTemplateRequest) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let template_id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        
        conn.execute(
            "INSERT INTO custom_anki_templates 
             (id, name, description, author, version, preview_front, preview_back, note_type,
              fields_json, generation_prompt, front_template, back_template, css_style,
              field_extraction_rules_json, created_at, updated_at, is_active, is_built_in)
             VALUES (?1, ?2, ?3, ?4, '1.0.0', ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, 1, 0)",
            params![
                template_id,
                request.name,
                request.description,
                request.author,
                request.preview_front,
                request.preview_back,
                request.note_type,
                serde_json::to_string(&request.fields)?,
                request.generation_prompt,
                request.front_template,
                request.back_template,
                request.css_style,
                serde_json::to_string(&request.field_extraction_rules)?,
                now.clone(),
                now
            ]
        )?;
        
        Ok(template_id)
    }
    
    /// 获取所有自定义模板
    pub fn get_all_custom_templates(&self) -> Result<Vec<crate::models::CustomAnkiTemplate>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, author, version, preview_front, preview_back, note_type,
                    fields_json, generation_prompt, front_template, back_template, css_style,
                    field_extraction_rules_json, created_at, updated_at, is_active, is_built_in
             FROM custom_anki_templates ORDER BY created_at DESC"
        )?;
        
        let template_iter = stmt.query_map([], |row| {
            let fields_json: String = row.get(8)?;
            let fields: Vec<String> = serde_json::from_str(&fields_json).unwrap_or_default();
            
            let rules_json: String = row.get(13)?;
            let field_extraction_rules: std::collections::HashMap<String, crate::models::FieldExtractionRule> = 
                serde_json::from_str(&rules_json).unwrap_or_default();
            
            let created_at_str: String = row.get(14)?;
            let updated_at_str: String = row.get(15)?;
            
            Ok(crate::models::CustomAnkiTemplate {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                author: row.get(3)?,
                version: row.get(4)?,
                preview_front: row.get(5)?,
                preview_back: row.get(6)?,
                note_type: row.get(7)?,
                fields,
                generation_prompt: row.get(9)?,
                front_template: row.get(10)?,
                back_template: row.get(11)?,
                css_style: row.get(12)?,
                field_extraction_rules,
                created_at: DateTime::parse_from_rfc3339(&created_at_str).unwrap().with_timezone(&Utc),
                updated_at: DateTime::parse_from_rfc3339(&updated_at_str).unwrap().with_timezone(&Utc),
                is_active: row.get::<_, i32>(16)? != 0,
                is_built_in: row.get::<_, i32>(17)? != 0,
            })
        })?;
        
        let mut templates = Vec::new();
        for template in template_iter {
            templates.push(template?);
        }
        
        Ok(templates)
    }
    
    /// 获取指定ID的自定义模板
    pub fn get_custom_template_by_id(&self, template_id: &str) -> Result<Option<crate::models::CustomAnkiTemplate>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, author, version, preview_front, preview_back, note_type,
                    fields_json, generation_prompt, front_template, back_template, css_style,
                    field_extraction_rules_json, created_at, updated_at, is_active, is_built_in
             FROM custom_anki_templates WHERE id = ?1"
        )?;
        
        let result = stmt.query_row(params![template_id], |row| {
            let fields_json: String = row.get(8)?;
            let fields: Vec<String> = serde_json::from_str(&fields_json).unwrap_or_default();
            
            let rules_json: String = row.get(13)?;
            let field_extraction_rules: std::collections::HashMap<String, crate::models::FieldExtractionRule> = 
                serde_json::from_str(&rules_json).unwrap_or_default();
            
            let created_at_str: String = row.get(14)?;
            let updated_at_str: String = row.get(15)?;
            
            Ok(crate::models::CustomAnkiTemplate {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                author: row.get(3)?,
                version: row.get(4)?,
                preview_front: row.get(5)?,
                preview_back: row.get(6)?,
                note_type: row.get(7)?,
                fields,
                generation_prompt: row.get(9)?,
                front_template: row.get(10)?,
                back_template: row.get(11)?,
                css_style: row.get(12)?,
                field_extraction_rules,
                created_at: DateTime::parse_from_rfc3339(&created_at_str).unwrap().with_timezone(&Utc),
                updated_at: DateTime::parse_from_rfc3339(&updated_at_str).unwrap().with_timezone(&Utc),
                is_active: row.get::<_, i32>(16)? != 0,
                is_built_in: row.get::<_, i32>(17)? != 0,
            })
        }).optional()?;
        
        Ok(result)
    }
    
    /// 更新自定义模板
    pub fn update_custom_template(&self, template_id: &str, request: &crate::models::UpdateTemplateRequest) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        
        let mut query_parts = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        
        // 将需要长期存储的值移动到这里，避免借用生命周期问题
        let mut owned_fields_json = None;
        let mut owned_rules_json = None;
        let mut owned_active_val = None;
        
        if let Some(name) = &request.name {
            query_parts.push("name = ?".to_string());
            params.push(Box::new(name.clone()));
        }
        if let Some(description) = &request.description {
            query_parts.push("description = ?".to_string());
            params.push(Box::new(description.clone()));
        }
        if let Some(author) = &request.author {
            query_parts.push("author = ?".to_string());
            params.push(Box::new(author.clone()));
        }
        if let Some(preview_front) = &request.preview_front {
            query_parts.push("preview_front = ?".to_string());
            params.push(Box::new(preview_front.clone()));
        }
        if let Some(preview_back) = &request.preview_back {
            query_parts.push("preview_back = ?".to_string());
            params.push(Box::new(preview_back.clone()));
        }
        if let Some(note_type) = &request.note_type {
            query_parts.push("note_type = ?".to_string());
            params.push(Box::new(note_type.clone()));
        }
        if let Some(fields) = &request.fields {
            query_parts.push("fields_json = ?".to_string());
            let fields_json = serde_json::to_string(fields)?;
            owned_fields_json = Some(fields_json.clone());
            params.push(Box::new(fields_json));
        }
        if let Some(generation_prompt) = &request.generation_prompt {
            query_parts.push("generation_prompt = ?".to_string());
            params.push(Box::new(generation_prompt.clone()));
        }
        if let Some(front_template) = &request.front_template {
            query_parts.push("front_template = ?".to_string());
            params.push(Box::new(front_template.clone()));
        }
        if let Some(back_template) = &request.back_template {
            query_parts.push("back_template = ?".to_string());
            params.push(Box::new(back_template.clone()));
        }
        if let Some(css_style) = &request.css_style {
            query_parts.push("css_style = ?".to_string());
            params.push(Box::new(css_style.clone()));
        }
        if let Some(field_extraction_rules) = &request.field_extraction_rules {
            query_parts.push("field_extraction_rules_json = ?".to_string());
            let rules_json = serde_json::to_string(field_extraction_rules)?;
            owned_rules_json = Some(rules_json.clone());
            params.push(Box::new(rules_json));
        }
        if let Some(is_active) = &request.is_active {
            query_parts.push("is_active = ?".to_string());
            let active_val = if *is_active { 1 } else { 0 };
            owned_active_val = Some(active_val);
            params.push(Box::new(active_val));
        }
        
        if query_parts.is_empty() {
            return Ok(());
        }
        
        query_parts.push("updated_at = ?".to_string());
        params.push(Box::new(now));
        params.push(Box::new(template_id.to_string()));
        
        let query = format!(
            "UPDATE custom_anki_templates SET {} WHERE id = ?",
            query_parts.join(", ")
        );
        
        conn.execute(&query, rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())))?;
        Ok(())
    }
    
    /// 删除自定义模板
    pub fn delete_custom_template(&self, template_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM custom_anki_templates WHERE id = ?1 AND is_built_in = 0", params![template_id])?;
        Ok(())
    }

    fn migrate_review_sessions_to_review_analyses(&self, conn: &rusqlite::Connection) -> anyhow::Result<()> {
        // 检查旧表是否存在
        let old_table_exists = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='review_sessions';")?
            .query_map([], |_| Ok(()))?
            .any(|_| true);

        if !old_table_exists {
            println!("⏭️ 旧的review_sessions表不存在，跳过迁移");
            return Ok(());
        }

        println!("🔄 迁移review_sessions数据到review_analyses");

        // 创建新表（如果不存在）
        conn.execute(
            "CREATE TABLE IF NOT EXISTS review_analyses (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                subject TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                mistake_ids TEXT NOT NULL,
                consolidated_input TEXT NOT NULL,
                user_question TEXT NOT NULL,
                status TEXT NOT NULL,
                tags TEXT NOT NULL,
                analysis_type TEXT NOT NULL DEFAULT 'consolidated_review'
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS review_chat_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                review_analysis_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                thinking_content TEXT,
                rag_sources TEXT,
                FOREIGN KEY(review_analysis_id) REFERENCES review_analyses(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // 迁移数据
        let mut stmt = conn.prepare("SELECT id, subject, mistake_ids, analysis_result, created_at FROM review_sessions")?;
        let old_sessions: Vec<(String, String, String, String, String)> = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?, // id
                row.get::<_, String>(1)?, // subject  
                row.get::<_, String>(2)?, // mistake_ids
                row.get::<_, String>(3)?, // analysis_result
                row.get::<_, String>(4)?, // created_at
            ))
        })?.collect::<rusqlite::Result<Vec<_>>>()?;

        let migration_count = old_sessions.len();
        
        for (id, subject, mistake_ids, analysis_result, created_at) in old_sessions {
            // 插入到新表
            conn.execute(
                "INSERT OR IGNORE INTO review_analyses 
                 (id, name, subject, created_at, updated_at, mistake_ids, consolidated_input, user_question, status, tags, analysis_type)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    id,
                    format!("回顾分析-{}", chrono::Utc::now().format("%Y%m%d")), // 默认名称
                    subject,
                    created_at,
                    chrono::Utc::now().to_rfc3339(), // updated_at
                    mistake_ids,
                    analysis_result, // 作为consolidated_input
                    "统一回顾分析", // 默认用户问题
                    "completed", // 默认状态
                    "[]", // 空标签数组
                    "consolidated_review"
                ]
            )?;

            // 迁移聊天记录
            let mut chat_stmt = conn.prepare("SELECT role, content, timestamp FROM review_chat_messages WHERE review_id = ?1")?;
            let chat_messages: Vec<(String, String, String)> = chat_stmt.query_map([&id], |row| {
                Ok((
                    row.get::<_, String>(0)?, // role
                    row.get::<_, String>(1)?, // content
                    row.get::<_, String>(2)?, // timestamp
                ))
            })?.collect::<rusqlite::Result<Vec<_>>>()?;

            for (role, content, timestamp) in chat_messages {
                conn.execute(
                    "INSERT INTO review_chat_messages 
                     (review_analysis_id, role, content, timestamp, thinking_content, rag_sources)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![id, role, content, timestamp, None::<String>, None::<String>]
                )?;
            }
        }

        // 删除旧表（可选，为了保险起见先保留）
        // conn.execute("DROP TABLE IF EXISTS review_sessions", [])?;
        // conn.execute("DROP TABLE IF EXISTS review_chat_messages", [])?;

        println!("✅ review_sessions迁移完成，迁移了{}条记录", migration_count);
        Ok(())
    }

    /// 保存错题及其聊天记录
    pub fn save_mistake(&self, mistake: &MistakeItem) -> Result<()> {
        // 验证JSON格式以防止存储损坏的数据
        self.validate_mistake_json_fields(mistake)?;
        
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        tx.execute(
            "INSERT OR REPLACE INTO mistakes (id, subject, created_at, question_images, analysis_images, user_question, ocr_text, tags, mistake_type, status, updated_at, mistake_summary, user_error_analysis)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                mistake.id,
                mistake.subject,
                mistake.created_at.to_rfc3339(),
                serde_json::to_string(&mistake.question_images)?,
                serde_json::to_string(&mistake.analysis_images)?,
                mistake.user_question,
                mistake.ocr_text,
                serde_json::to_string(&mistake.tags)?,
                mistake.mistake_type,
                mistake.status,
                Utc::now().to_rfc3339(),
                mistake.mistake_summary,
                mistake.user_error_analysis,
            ],
        )?;

        // 删除旧的聊天记录，然后插入新的
        tx.execute("DELETE FROM chat_messages WHERE mistake_id = ?1", params![mistake.id])?;
        for message in &mistake.chat_history {
            // 序列化RAG来源信息为JSON
            let rag_sources_json = message.rag_sources.as_ref()
                .map(|sources| serde_json::to_string(sources).unwrap_or_default());
            
            tx.execute(
                "INSERT INTO chat_messages (mistake_id, role, content, timestamp, thinking_content, rag_sources) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![mistake.id, message.role, message.content, message.timestamp.to_rfc3339(), message.thinking_content, rag_sources_json],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// 保存回顾分析及其聊天记录 - 复用错题分析的保存模式
    pub fn save_review_analysis(&self, review: &ReviewAnalysisItem) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        tx.execute(
            "INSERT OR REPLACE INTO review_analyses 
             (id, name, subject, created_at, updated_at, mistake_ids, consolidated_input, user_question, status, tags, analysis_type)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                review.id,
                review.name,
                review.subject,
                review.created_at.to_rfc3339(),
                review.updated_at.to_rfc3339(),
                serde_json::to_string(&review.mistake_ids)?,
                review.consolidated_input,
                review.user_question,
                review.status,
                serde_json::to_string(&review.tags)?,
                review.analysis_type,
            ],
        )?;

        // 删除旧的聊天记录，然后插入新的
        tx.execute("DELETE FROM review_chat_messages WHERE review_analysis_id = ?1", params![review.id])?;
        for message in &review.chat_history {
            let rag_sources_json = message.rag_sources.as_ref()
                .map(|sources| serde_json::to_string(sources))
                .transpose()?;

            tx.execute(
                "INSERT INTO review_chat_messages 
                 (review_analysis_id, role, content, timestamp, thinking_content, rag_sources)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    review.id,
                    message.role,
                    message.content,
                    message.timestamp.to_rfc3339(),
                    message.thinking_content,
                    rag_sources_json,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// 根据ID获取错题及其聊天记录
    pub fn get_mistake_by_id(&self, id: &str) -> Result<Option<MistakeItem>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, subject, created_at, question_images, analysis_images, user_question, ocr_text, tags, mistake_type, status, updated_at, mistake_summary, user_error_analysis FROM mistakes WHERE id = ?1"
        )?;
        let mistake_item = stmt.query_row(params![id], |row| {
            let created_at_str: String = row.get(2)?;
            let updated_at_str: String = row.get(10)?;
            let question_images_str: String = row.get(3)?;
            let analysis_images_str: String = row.get(4)?;
            let tags_str: String = row.get(7)?;
            
            let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                .map_err(|_| rusqlite::Error::InvalidColumnType(2, "created_at".to_string(), rusqlite::types::Type::Text))?
                .with_timezone(&Utc);
            let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
                .map_err(|_| rusqlite::Error::InvalidColumnType(10, "updated_at".to_string(), rusqlite::types::Type::Text))?
                .with_timezone(&Utc);
            let question_images: Vec<String> = serde_json::from_str(&question_images_str)
                .map_err(|_| rusqlite::Error::InvalidColumnType(3, "question_images".to_string(), rusqlite::types::Type::Text))?;
            let analysis_images: Vec<String> = serde_json::from_str(&analysis_images_str)
                .map_err(|_| rusqlite::Error::InvalidColumnType(4, "analysis_images".to_string(), rusqlite::types::Type::Text))?;
            let tags: Vec<String> = serde_json::from_str(&tags_str)
                .map_err(|_| rusqlite::Error::InvalidColumnType(7, "tags".to_string(), rusqlite::types::Type::Text))?;
            
            Ok(MistakeItem {
                id: row.get(0)?,
                subject: row.get(1)?,
                created_at,
                question_images,
                analysis_images,
                user_question: row.get(5)?,
                ocr_text: row.get(6)?,
                tags,
                mistake_type: row.get(8)?,
                status: row.get(9)?,
                updated_at,
                chat_history: vec![], // 稍后填充
                mistake_summary: row.get(11)?,        // 新增字段
                user_error_analysis: row.get(12)?,    // 新增字段
            })
        }).optional()?;

        if let Some(mut item) = mistake_item {
            let mut chat_stmt = conn.prepare(
                "SELECT role, content, timestamp, thinking_content, rag_sources FROM chat_messages WHERE mistake_id = ?1 ORDER BY timestamp ASC"
            )?;
            let chat_iter = chat_stmt.query_map(params![id], |row| {
                let timestamp_str: String = row.get(2)?;
                let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
                    .map_err(|_| rusqlite::Error::InvalidColumnType(2, "timestamp".to_string(), rusqlite::types::Type::Text))?
                    .with_timezone(&Utc);
                
                // 反序列化RAG来源信息
                let rag_sources: Option<Vec<crate::models::RagSourceInfo>> = 
                    if let Ok(Some(rag_sources_str)) = row.get::<_, Option<String>>(4) {
                        serde_json::from_str(&rag_sources_str).ok()
                    } else {
                        None
                    };
                
                Ok(ChatMessage {
                    role: row.get(0)?,
                    content: row.get(1)?,
                    timestamp,
                    thinking_content: row.get(3)?,
                    rag_sources,
                    image_paths: None,
                    image_base64: None,
                })
            })?;
            for msg_result in chat_iter {
                item.chat_history.push(msg_result?);
            }
            Ok(Some(item))
        } else {
            Ok(None)
        }
    }

    /// 根据ID获取回顾分析及其聊天记录 - 复用错题分析的查询模式
    pub fn get_review_analysis_by_id(&self, id: &str) -> Result<Option<ReviewAnalysisItem>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, subject, created_at, updated_at, mistake_ids, consolidated_input, user_question, status, tags, analysis_type 
             FROM review_analyses WHERE id = ?1"
        )?;
        let review_item = stmt.query_row(params![id], |row| {
            let created_at_str: String = row.get(3)?;
            let updated_at_str: String = row.get(4)?;
            let mistake_ids_str: String = row.get(5)?;
            let tags_str: String = row.get(9)?;
            
            let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                .map_err(|_| rusqlite::Error::InvalidColumnType(3, "created_at".to_string(), rusqlite::types::Type::Text))?
                .with_timezone(&Utc);
            let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
                .map_err(|_| rusqlite::Error::InvalidColumnType(4, "updated_at".to_string(), rusqlite::types::Type::Text))?
                .with_timezone(&Utc);
            let mistake_ids: Vec<String> = serde_json::from_str(&mistake_ids_str)
                .map_err(|_| rusqlite::Error::InvalidColumnType(5, "mistake_ids".to_string(), rusqlite::types::Type::Text))?;
            let tags: Vec<String> = serde_json::from_str(&tags_str)
                .map_err(|_| rusqlite::Error::InvalidColumnType(9, "tags".to_string(), rusqlite::types::Type::Text))?;
            
            Ok(ReviewAnalysisItem {
                id: row.get(0)?,
                name: row.get(1)?,
                subject: row.get(2)?,
                created_at,
                updated_at,
                mistake_ids,
                consolidated_input: row.get(6)?,
                user_question: row.get(7)?,
                status: row.get(8)?,
                tags,
                analysis_type: row.get(10)?,
                chat_history: vec![], // 稍后填充
            })
        }).optional()?;

        if let Some(mut item) = review_item {
            let mut chat_stmt = conn.prepare(
                "SELECT role, content, timestamp, thinking_content, rag_sources 
                 FROM review_chat_messages WHERE review_analysis_id = ?1 ORDER BY timestamp ASC"
            )?;
            let chat_iter = chat_stmt.query_map(params![id], |row| {
                let timestamp_str: String = row.get(2)?;
                let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
                    .map_err(|_| rusqlite::Error::InvalidColumnType(2, "timestamp".to_string(), rusqlite::types::Type::Text))?
                    .with_timezone(&Utc);
                
                // 反序列化RAG来源信息
                let rag_sources: Option<Vec<crate::models::RagSourceInfo>> = 
                    if let Ok(Some(rag_sources_str)) = row.get::<_, Option<String>>(4) {
                        serde_json::from_str(&rag_sources_str).ok()
                    } else {
                        None
                    };
                
                Ok(ChatMessage {
                    role: row.get(0)?,
                    content: row.get(1)?,
                    timestamp,
                    thinking_content: row.get(3)?,
                    rag_sources,
                    image_paths: None,
                    image_base64: None,
                })
            })?;
            for msg_result in chat_iter {
                item.chat_history.push(msg_result?);
            }
            Ok(Some(item))
        } else {
            Ok(None)
        }
    }

    /// 获取错题列表（支持筛选）
    pub fn get_mistakes(
        &self,
        subject_filter: Option<&str>,
        type_filter: Option<&str>,
        tags_filter: Option<&[String]>, // 标签包含任意一个即可
    ) -> Result<Vec<MistakeItem>> {
        let mut query = "SELECT id, subject, created_at, question_images, analysis_images, user_question, ocr_text, tags, mistake_type, status, updated_at FROM mistakes WHERE 1=1".to_string();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(s) = subject_filter {
            query.push_str(" AND subject = ?");
            params_vec.push(Box::new(s.to_string()));
        }
        if let Some(t) = type_filter {
            query.push_str(" AND mistake_type = ?");
            params_vec.push(Box::new(t.to_string()));
        }
        // 注意: SQLite JSON 函数通常需要特定构建或扩展。
        // 这里的标签过滤是一个简化版本，实际可能需要更复杂的查询或在应用层过滤。
        // 一个更健壮的方法是使用JSON1扩展的json_each或json_extract。
        // 为简单起见，如果提供了标签过滤器，我们暂时获取所有数据并在Rust中过滤。
        // 或者，如果标签数量不多，可以构建 LIKE '%tag1%' OR LIKE '%tag2%' 这样的查询。

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&query)?;
        
        let mut rows = stmt.query(rusqlite::params_from_iter(params_vec.iter().map(|p| p.as_ref())))?;
        let mut mistakes = Vec::new();
        while let Some(row) = rows.next()? {
            let created_at_str: String = row.get(2).map_err(|e| anyhow::anyhow!("Failed to get created_at: {}", e))?;
            let updated_at_str: String = row.get(10).map_err(|e| anyhow::anyhow!("Failed to get updated_at: {}", e))?;
            let tags_json: String = row.get(7).map_err(|e| anyhow::anyhow!("Failed to get tags: {}", e))?;
            let current_tags: Vec<String> = serde_json::from_str(&tags_json)
                .map_err(|e| anyhow::anyhow!("Failed to parse tags JSON: {}", e))?;

            // 应用层标签过滤
            if let Some(filter_tags) = tags_filter {
                if filter_tags.is_empty() || !filter_tags.iter().any(|ft| current_tags.contains(ft)) {
                    if !filter_tags.is_empty() { // 如果过滤器非空但不匹配，则跳过
                         continue;
                    }
                }
            }

            let question_images_str: String = row.get(3).map_err(|e| anyhow::anyhow!("Failed to get question_images: {}", e))?;
            let analysis_images_str: String = row.get(4).map_err(|e| anyhow::anyhow!("Failed to get analysis_images: {}", e))?;
            
            let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse created_at: {}", e))?
                .with_timezone(&Utc);
            let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse updated_at: {}", e))?
                .with_timezone(&Utc);
            let question_images: Vec<String> = serde_json::from_str(&question_images_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse question_images JSON: {}", e))?;
            let analysis_images: Vec<String> = serde_json::from_str(&analysis_images_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse analysis_images JSON: {}", e))?;
            
            let item = MistakeItem {
                id: row.get(0).map_err(|e| anyhow::anyhow!("Failed to get id: {}", e))?,
                subject: row.get(1).map_err(|e| anyhow::anyhow!("Failed to get subject: {}", e))?,
                created_at,
                question_images,
                analysis_images,
                user_question: row.get(5).map_err(|e| anyhow::anyhow!("Failed to get user_question: {}", e))?,
                ocr_text: row.get(6).map_err(|e| anyhow::anyhow!("Failed to get ocr_text: {}", e))?,
                tags: current_tags,
                mistake_type: row.get(8).map_err(|e| anyhow::anyhow!("Failed to get mistake_type: {}", e))?,
                status: row.get(9).map_err(|e| anyhow::anyhow!("Failed to get status: {}", e))?,
                updated_at,
                chat_history: vec![], // 列表视图通常不需要完整的聊天记录
                mistake_summary: None,       // 列表视图不加载总结
                user_error_analysis: None,   // 列表视图不加载分析
            };
            mistakes.push(item);
        }
        Ok(mistakes)
    }

    /// 获取回顾分析列表（复用错题分析的列表模式）
    pub fn get_review_analyses(
        &self,
        subject_filter: Option<&str>,
        status_filter: Option<&str>,
    ) -> Result<Vec<ReviewAnalysisItem>> {
        let mut query = "SELECT id, name, subject, created_at, updated_at, mistake_ids, consolidated_input, user_question, status, tags, analysis_type FROM review_analyses WHERE 1=1".to_string();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(s) = subject_filter {
            query.push_str(" AND subject = ?");
            params_vec.push(Box::new(s.to_string()));
        }
        if let Some(st) = status_filter {
            query.push_str(" AND status = ?");
            params_vec.push(Box::new(st.to_string()));
        }
        
        query.push_str(" ORDER BY created_at DESC");

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&query)?;
        
        let mut rows = stmt.query(rusqlite::params_from_iter(params_vec.iter().map(|p| p.as_ref())))?;
        let mut analyses = Vec::new();
        while let Some(row) = rows.next()? {
            let created_at_str: String = row.get(3)?;
            let updated_at_str: String = row.get(4)?;
            let mistake_ids_json: String = row.get(5)?;
            let tags_json: String = row.get(9)?;
            
            let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse created_at: {}", e))?
                .with_timezone(&Utc);
            let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse updated_at: {}", e))?
                .with_timezone(&Utc);
            let mistake_ids: Vec<String> = serde_json::from_str(&mistake_ids_json)
                .map_err(|e| anyhow::anyhow!("Failed to parse mistake_ids JSON: {}", e))?;
            let tags: Vec<String> = serde_json::from_str(&tags_json)
                .map_err(|e| anyhow::anyhow!("Failed to parse tags JSON: {}", e))?;
            
            let item = ReviewAnalysisItem {
                id: row.get(0)?,
                name: row.get(1)?,
                subject: row.get(2)?,
                created_at,
                updated_at,
                mistake_ids,
                consolidated_input: row.get(6)?,
                user_question: row.get(7)?,
                status: row.get(8)?,
                tags,
                analysis_type: row.get(10)?,
                chat_history: vec![], // 列表视图不需要完整的聊天记录
            };
            analyses.push(item);
        }
        Ok(analyses)
    }

    /// 删除错题（同时删除关联的聊天记录，通过FOREIGN KEY CASCADE）
    pub fn delete_mistake(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let changes = conn.execute("DELETE FROM mistakes WHERE id = ?1", params![id])?;
        Ok(changes > 0)
    }

    /// 保存设置
    pub fn save_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)",
            params![key, value, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    /// 获取设置
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        ).optional().map_err(Into::into)
    }

    /// 获取统计信息
    pub fn get_statistics(&self) -> Result<Statistics> {
        let conn = self.conn.lock().unwrap();
        
        // 获取错题总数
        let total_mistakes: i32 = conn.query_row(
            "SELECT COUNT(*) FROM mistakes",
            [],
            |row| row.get(0),
        )?;

        // 获取回顾分析总数（暂时为0，等实现回顾功能时更新）
        let total_reviews: i32 = 0;

        // 获取各科目统计
        let mut subject_stats = std::collections::HashMap::new();
        let mut stmt_subjects = conn.prepare("SELECT subject, COUNT(*) as count FROM mistakes GROUP BY subject")?;
        let subject_iter = stmt_subjects.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?))
        })?;
        for subject_result in subject_iter {
            let (subject, count) = subject_result?;
            subject_stats.insert(subject, count);
        }

        // 获取各题目类型统计
        let mut type_stats = std::collections::HashMap::new();
        let mut stmt_types = conn.prepare("SELECT mistake_type, COUNT(*) as count FROM mistakes GROUP BY mistake_type")?;
        let type_iter = stmt_types.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?))
        })?;
        for type_result in type_iter {
            let (mistake_type, count) = type_result?;
            type_stats.insert(mistake_type, count);
        }

        // 获取标签统计
        let mut tag_stats = std::collections::HashMap::new();
        let mut stmt_tags = conn.prepare("SELECT tags FROM mistakes WHERE tags IS NOT NULL AND tags != ''")?;
        let tag_iter = stmt_tags.query_map([], |row| {
            Ok(row.get::<_, String>(0)?)
        })?;
        for tag_result in tag_iter {
            let tags_json = tag_result?;
            if let Ok(tags) = serde_json::from_str::<Vec<String>>(&tags_json) {
                for tag in tags {
                    *tag_stats.entry(tag).or_insert(0) += 1;
                }
            }
        }

        // 获取最近的错题
        let mut recent_mistakes = Vec::new();
        let mut stmt_recent = conn.prepare(
            "SELECT id, subject, user_question, tags, created_at FROM mistakes ORDER BY created_at DESC LIMIT 5"
        )?;
        let recent_iter = stmt_recent.query_map([], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "subject": row.get::<_, String>(1)?,
                "user_question": row.get::<_, String>(2)?,
                "tags": serde_json::from_str::<Vec<String>>(&row.get::<_, String>(3)?).unwrap_or_default(),
                "created_at": row.get::<_, String>(4)?
            }))
        })?;
        for recent_result in recent_iter {
            recent_mistakes.push(recent_result?);
        }
        
        Ok(Statistics {
            total_mistakes,
            total_reviews,
            subject_stats,
            type_stats,
            tag_stats,
            recent_mistakes,
        })
    }

    // TODO: 实现 review_sessions 和 review_chat_messages 表的相关操作

    /// 保存回顾分析会话
    pub fn save_review_session(&self, session: &crate::models::ReviewSession) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        
        // 保存会话基本信息
        tx.execute(
            "INSERT OR REPLACE INTO review_sessions (id, subject, mistake_ids, analysis_summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                session.id,
                session.subject,
                serde_json::to_string(&session.mistake_ids)?,
                session.analysis_summary,
                session.created_at.to_rfc3339(),
                session.updated_at.to_rfc3339()
            ],
        )?;
        
        // 删除旧的聊天记录
        tx.execute("DELETE FROM review_chat_messages WHERE session_id = ?1", params![session.id])?;
        
        // 保存聊天记录
        for msg in &session.chat_history {
            tx.execute(
                "INSERT INTO review_chat_messages (id, session_id, role, content, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    msg.id,
                    msg.session_id,
                    msg.role,
                    msg.content,
                    msg.timestamp.to_rfc3339()
                ],
            )?;
        }
        
        tx.commit()?;
        Ok(())
    }
    
    /// 根据ID获取回顾分析会话
    pub fn get_review_session_by_id(&self, id: &str) -> Result<Option<crate::models::ReviewSession>> {
        let conn = self.conn.lock().unwrap();
        
        let session_result = conn.query_row(
            "SELECT id, subject, mistake_ids, analysis_summary, created_at, updated_at FROM review_sessions WHERE id = ?1",
            params![id],
            |row| {
                let created_at_str: String = row.get(4)?;
                let updated_at_str: String = row.get(5)?;
                let mistake_ids_str: String = row.get(2)?;
                
                let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                    .map_err(|_| rusqlite::Error::InvalidColumnType(4, "created_at".to_string(), rusqlite::types::Type::Text))?
                    .with_timezone(&Utc);
                let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
                    .map_err(|_| rusqlite::Error::InvalidColumnType(5, "updated_at".to_string(), rusqlite::types::Type::Text))?
                    .with_timezone(&Utc);
                let mistake_ids: Vec<String> = serde_json::from_str(&mistake_ids_str)
                    .map_err(|_| rusqlite::Error::InvalidColumnType(2, "mistake_ids".to_string(), rusqlite::types::Type::Text))?;
                
                Ok(crate::models::ReviewSession {
                    id: row.get(0)?,
                    subject: row.get(1)?,
                    mistake_ids,
                    analysis_summary: row.get(3)?,
                    created_at,
                    updated_at,
                    chat_history: vec![], // 稍后填充
                })
            }
        ).optional()?;
        
        if let Some(mut session) = session_result {
            // 获取聊天记录
            let mut chat_stmt = conn.prepare(
                "SELECT id, session_id, role, content, timestamp FROM review_chat_messages WHERE session_id = ?1 ORDER BY timestamp ASC"
            )?;
            let chat_iter = chat_stmt.query_map(params![id], |row| {
                let timestamp_str: String = row.get(4)?;
                let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
                    .map_err(|_| rusqlite::Error::InvalidColumnType(4, "timestamp".to_string(), rusqlite::types::Type::Text))?
                    .with_timezone(&Utc);
                Ok(crate::models::ReviewChatMessage {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    role: row.get(2)?,
                    content: row.get(3)?,
                    timestamp,
                })
            })?;
            
            for msg_result in chat_iter {
                session.chat_history.push(msg_result?);
            }
            
            Ok(Some(session))
        } else {
            Ok(None)
        }
    }
    
    /// 获取回顾分析会话列表
    pub fn get_review_sessions(&self, subject_filter: Option<&str>) -> Result<Vec<crate::models::ReviewSession>> {
        let conn = self.conn.lock().unwrap();
        
        let mut query = "SELECT id, subject, mistake_ids, analysis_summary, created_at, updated_at FROM review_sessions WHERE 1=1".to_string();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        
        if let Some(subject) = subject_filter {
            query.push_str(" AND subject = ?");
            params_vec.push(Box::new(subject.to_string()));
        }
        
        query.push_str(" ORDER BY created_at DESC");
        
        let mut stmt = conn.prepare(&query)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(params_vec.iter().map(|p| p.as_ref())))?;
        let mut sessions = Vec::new();
        
        while let Some(row) = rows.next()? {
            let created_at_str: String = row.get(4)?;
            let updated_at_str: String = row.get(5)?;
            let mistake_ids_str: String = row.get(2)?;
            
            let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse created_at: {}", e))?
                .with_timezone(&Utc);
            let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse updated_at: {}", e))?
                .with_timezone(&Utc);
            let mistake_ids: Vec<String> = serde_json::from_str(&mistake_ids_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse mistake_ids JSON: {}", e))?;
            
            let session = crate::models::ReviewSession {
                id: row.get(0)?,
                subject: row.get(1)?,
                mistake_ids,
                analysis_summary: row.get(3)?,
                created_at,
                updated_at,
                chat_history: vec![], // 列表视图不需要完整聊天记录
            };
            sessions.push(session);
        }
        
        Ok(sessions)
    }
    
    /// 删除回顾分析会话
    pub fn delete_review_session(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let changes = conn.execute("DELETE FROM review_sessions WHERE id = ?1", params![id])?;
        Ok(changes > 0)
    }

    /// 删除回顾分析（统一回顾分析功能）
    pub fn delete_review_analysis(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        // 由于设置了 ON DELETE CASCADE，删除主记录时会自动删除关联的聊天消息
        let changes = conn.execute("DELETE FROM review_analyses WHERE id = ?1", params![id])?;
        Ok(changes > 0)
    }
    
    /// 添加回顾分析聊天消息
    pub fn add_review_chat_message(&self, message: &crate::models::ReviewChatMessage) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO review_chat_messages (id, session_id, role, content, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                message.id,
                message.session_id,
                message.role,
                message.content,
                message.timestamp.to_rfc3339()
            ],
        )?;
        
        // 更新会话的更新时间
        conn.execute(
            "UPDATE review_sessions SET updated_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), message.session_id],
        )?;
        
        Ok(())
    }

    // 科目配置管理方法

    /// 保存科目配置
    pub fn save_subject_config(&self, config: &SubjectConfig) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO subject_configs (id, subject_name, display_name, description, is_enabled, prompts, mistake_types, default_tags, created_at, updated_at) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                config.id,
                config.subject_name,
                config.display_name,
                config.description,
                config.is_enabled as i32,
                serde_json::to_string(&config.prompts)?,
                serde_json::to_string(&config.mistake_types)?,
                serde_json::to_string(&config.default_tags)?,
                config.created_at.to_rfc3339(),
                config.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// 根据ID获取科目配置
    pub fn get_subject_config_by_id(&self, id: &str) -> Result<Option<SubjectConfig>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, subject_name, display_name, description, is_enabled, prompts, mistake_types, default_tags, created_at, updated_at 
             FROM subject_configs WHERE id = ?1",
            params![id],
            |row| {
                let created_at_str: String = row.get(8)?;
                let updated_at_str: String = row.get(9)?;
                let prompts_str: String = row.get(5)?;
                let mistake_types_str: String = row.get(6)?;
                let default_tags_str: String = row.get(7)?;
                
                let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                    .map_err(|_| rusqlite::Error::InvalidColumnType(8, "created_at".to_string(), rusqlite::types::Type::Text))?
                    .with_timezone(&Utc);
                let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
                    .map_err(|_| rusqlite::Error::InvalidColumnType(9, "updated_at".to_string(), rusqlite::types::Type::Text))?
                    .with_timezone(&Utc);
                let prompts: SubjectPrompts = self.parse_subject_prompts(&prompts_str)?;
                let mistake_types: Vec<String> = serde_json::from_str(&mistake_types_str)
                    .map_err(|_| rusqlite::Error::InvalidColumnType(6, "mistake_types".to_string(), rusqlite::types::Type::Text))?;
                let default_tags: Vec<String> = serde_json::from_str(&default_tags_str)
                    .map_err(|_| rusqlite::Error::InvalidColumnType(7, "default_tags".to_string(), rusqlite::types::Type::Text))?;
                
                Ok(SubjectConfig {
                    id: row.get(0)?,
                    subject_name: row.get(1)?,
                    display_name: row.get(2)?,
                    description: row.get(3)?,
                    is_enabled: row.get::<_, i32>(4)? != 0,
                    prompts,
                    mistake_types,
                    default_tags,
                    created_at,
                    updated_at,
                })
            }
        ).optional().map_err(Into::into)
    }

    /// 根据科目名称获取科目配置
    pub fn get_subject_config_by_name(&self, subject_name: &str) -> Result<Option<SubjectConfig>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, subject_name, display_name, description, is_enabled, prompts, mistake_types, default_tags, created_at, updated_at 
             FROM subject_configs WHERE subject_name = ?1",
            params![subject_name],
            |row| {
                let created_at_str: String = row.get(8)?;
                let updated_at_str: String = row.get(9)?;
                let prompts_str: String = row.get(5)?;
                let mistake_types_str: String = row.get(6)?;
                let default_tags_str: String = row.get(7)?;
                
                let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                    .map_err(|_| rusqlite::Error::InvalidColumnType(8, "created_at".to_string(), rusqlite::types::Type::Text))?
                    .with_timezone(&Utc);
                let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
                    .map_err(|_| rusqlite::Error::InvalidColumnType(9, "updated_at".to_string(), rusqlite::types::Type::Text))?
                    .with_timezone(&Utc);
                let prompts: SubjectPrompts = self.parse_subject_prompts(&prompts_str)?;
                let mistake_types: Vec<String> = serde_json::from_str(&mistake_types_str)
                    .map_err(|_| rusqlite::Error::InvalidColumnType(6, "mistake_types".to_string(), rusqlite::types::Type::Text))?;
                let default_tags: Vec<String> = serde_json::from_str(&default_tags_str)
                    .map_err(|_| rusqlite::Error::InvalidColumnType(7, "default_tags".to_string(), rusqlite::types::Type::Text))?;
                
                Ok(SubjectConfig {
                    id: row.get(0)?,
                    subject_name: row.get(1)?,
                    display_name: row.get(2)?,
                    description: row.get(3)?,
                    is_enabled: row.get::<_, i32>(4)? != 0,
                    prompts,
                    mistake_types,
                    default_tags,
                    created_at,
                    updated_at,
                })
            }
        ).optional().map_err(Into::into)
    }

    /// 获取所有科目配置
    pub fn get_all_subject_configs(&self, enabled_only: bool) -> Result<Vec<SubjectConfig>> {
        let conn = self.conn.lock().unwrap();
        
        let query = if enabled_only {
            "SELECT id, subject_name, display_name, description, is_enabled, prompts, mistake_types, default_tags, created_at, updated_at 
             FROM subject_configs WHERE is_enabled = 1 ORDER BY display_name"
        } else {
            "SELECT id, subject_name, display_name, description, is_enabled, prompts, mistake_types, default_tags, created_at, updated_at 
             FROM subject_configs ORDER BY display_name"
        };
        
        let mut stmt = conn.prepare(query)?;
        let mut rows = stmt.query([])?;
        let mut configs = Vec::new();
        
        while let Some(row) = rows.next()? {
            let created_at_str: String = row.get(8)?;
            let updated_at_str: String = row.get(9)?;
            let prompts_str: String = row.get(5)?;
            let mistake_types_str: String = row.get(6)?;
            let default_tags_str: String = row.get(7)?;
            
            let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse created_at: {}", e))?
                .with_timezone(&Utc);
            let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse updated_at: {}", e))?
                .with_timezone(&Utc);
            let prompts: SubjectPrompts = self.parse_subject_prompts(&prompts_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse prompts JSON: {:?}", e))?;
            let mistake_types: Vec<String> = serde_json::from_str(&mistake_types_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse mistake_types JSON: {}", e))?;
            let default_tags: Vec<String> = serde_json::from_str(&default_tags_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse default_tags JSON: {}", e))?;
            
            let config = SubjectConfig {
                id: row.get(0)?,
                subject_name: row.get(1)?,
                display_name: row.get(2)?,
                description: row.get(3)?,
                is_enabled: row.get::<_, i32>(4)? != 0,
                prompts,
                mistake_types,
                default_tags,
                created_at,
                updated_at,
            };
            configs.push(config);
        }
        
        Ok(configs)
    }

    /// 删除科目配置
    pub fn delete_subject_config(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let changes = conn.execute("DELETE FROM subject_configs WHERE id = ?1", params![id])?;
        Ok(changes > 0)
    }

    /// 初始化默认科目配置
    pub fn initialize_default_subject_configs(&self) -> Result<()> {
        let default_subjects = vec![
            ("数学", "Mathematics", "数学错题分析和讲解", self.get_math_prompts()),
            ("物理", "Physics", "物理概念和计算题目分析", self.get_physics_prompts()),
            ("化学", "Chemistry", "化学反应和实验题目分析", self.get_chemistry_prompts()),
            ("英语", "English", "英语语法和阅读理解分析", self.get_english_prompts()),
            ("语文", "Chinese", "语文阅读理解和写作分析", self.get_chinese_prompts()),
            ("生物", "Biology", "生物概念和实验题目分析", self.get_biology_prompts()),
            ("历史", "History", "历史事件和知识点分析", self.get_history_prompts()),
            ("地理", "Geography", "地理概念和区域分析", self.get_geography_prompts()),
            ("政治", "Politics", "政治理论和时事分析", self.get_politics_prompts()),
        ];

        for (subject_name, _english_name, description, prompts) in default_subjects {
            // 检查是否已存在
            if self.get_subject_config_by_name(subject_name)?.is_none() {
                let config = SubjectConfig {
                    id: uuid::Uuid::new_v4().to_string(),
                    subject_name: subject_name.to_string(),
                    display_name: subject_name.to_string(),
                    description: description.to_string(),
                    is_enabled: true,
                    prompts,
                    mistake_types: vec![
                        "计算错误".to_string(),
                        "概念理解".to_string(),
                        "方法应用".to_string(),
                        "知识遗忘".to_string(),
                        "审题不清".to_string(),
                    ],
                    default_tags: vec![
                        "基础知识".to_string(),
                        "重点难点".to_string(),
                        "易错点".to_string(),
                    ],
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };
                self.save_subject_config(&config)?;
            }
        }
        
        Ok(())
    }

    // 数学科目的专业提示词
    fn get_math_prompts(&self) -> SubjectPrompts {
        SubjectPrompts {
            analysis_prompt: "你是一个数学教学专家。请根据提供的{subject}题目信息，详细解答学生的问题。解答要清晰、准确，包含必要的步骤和原理解释。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            review_prompt: "你是一个{subject}学习分析专家。请分析学生的多道错题，找出共同的易错点、知识盲区和学习模式。请提供：1. 错题间的关联性分析 2. 主要易错点总结 3. 知识点掌握情况评估 4. 针对性的学习建议和复习计划。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            chat_prompt: "基于这道{subject}题目，请回答学生的问题。回答要准确、详细，包含必要的公式推导和计算步骤。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            ocr_prompt: "你是一个{subject}题目分析专家。请识别图片中的{subject}题目文字内容。\n\n【重要】OCR文本提取要求：\n1. 提取纯文本内容，不要使用LaTeX格式\n2. 数学公式用普通文字描述，如：λ = 2 的几何重数\n3. 矩阵用文字描述，如：矩阵A减去2I等于...\n4. 避免使用\\left、\\begin{array}、\\frac等LaTeX命令\n5. 保持文本简洁易读".to_string(),
            classification_prompt: "请分析这道{subject}题目的类型（如选择题、填空题、计算题、证明题等），并生成相关的{subject}标签（如代数、几何、函数、导数、概率等）。".to_string(),
            consolidated_review_prompt: "你是一个{subject}回顾分析专家。请对以下多道{subject}错题进行统一分析，找出共同的易错点、知识盲区和学习模式。请提供：1. 错题间的关联性分析 2. 主要易错点总结 3. 知识点掌握情况评估 4. 针对性的学习建议和复习计划。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            anki_generation_prompt: "请根据以下{subject}科目的学习内容，生成适合制作Anki卡片的问题和答案对。每张卡片应测试一个单一的数学概念或公式。请以JSON数组格式返回结果，每个对象必须包含 \"front\" (字符串), \"back\" (字符串), \"tags\" (字符串数组) 三个字段。front字段应包含问题或概念名称，back字段应包含答案、公式或解释，tags字段应包含相关的数学知识点标签如代数、几何、函数等。".to_string(),
        }
    }

    // 物理科目的专业提示词
    fn get_physics_prompts(&self) -> SubjectPrompts {
        SubjectPrompts {
            analysis_prompt: "你是一个{subject}教学专家。请根据提供的题目信息，详细解答学生的问题。解答要包含物理原理、公式推导和计算过程，注重物理概念的理解。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            review_prompt: "你是一个{subject}学习分析专家。请分析学生的多道{subject}错题，找出共同的易错点、知识盲区和学习模式，提供针对性的{subject}学习建议。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            chat_prompt: "基于这道{subject}题目，请回答学生的问题。回答要包含相关的物理定律、公式应用和现象解释。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            ocr_prompt: "你是一个{subject}题目分析专家。请识别图片中的{subject}题目文字内容。\n\n【重要】OCR文本提取要求：\n1. 提取纯文本内容，不要使用LaTeX格式\n2. 物理公式用普通文字描述，如：F等于m乘以a\n3. 物理量用文字描述，如：速度v等于20米每秒\n4. 避免使用\\frac、\\sqrt等LaTeX命令\n5. 保持文本简洁易读".to_string(),
            classification_prompt: "请分析这道{subject}题目的类型，并生成相关的{subject}标签（如力学、电学、光学、热学、原子物理等）。".to_string(),
            consolidated_review_prompt: "你是一个{subject}回顾分析专家。请对以下多道{subject}错题进行统一分析，找出共同的易错点、知识盲区和学习模式。请提供：1. 错题间的关联性分析 2. 主要易错点总结 3. 知识点掌握情况评估 4. 针对性的学习建议和复习计划。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            anki_generation_prompt: "请根据以下{subject}科目的学习内容，生成适合制作Anki卡片的问题和答案对。每张卡片应测试一个单一的物理原理或定律概念。请以JSON数组格式返回结果，每个对象必须包含 \"front\" (字符串), \"back\" (字符串), \"tags\" (字符串数组) 三个字段。front字段应包含问题或概念名称，back字段应包含答案或解释，tags字段应包含相关的物理知识点标签。".to_string(),
        }
    }

    // 化学科目的专业提示词
    fn get_chemistry_prompts(&self) -> SubjectPrompts {
        SubjectPrompts {
            analysis_prompt: "你是一个{subject}教学专家。请根据提供的题目信息，详细解答学生的问题。解答要包含化学原理、方程式和计算过程，注重化学反应机理的理解。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            review_prompt: "你是一个{subject}学习分析专家。请分析学生的多道{subject}错题，找出共同的易错点、知识盲区和学习模式，提供针对性的{subject}学习建议。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            chat_prompt: "基于这道{subject}题目，请回答学生的问题。回答要包含相关的化学反应、化学方程式和实验原理。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            ocr_prompt: "你是一个{subject}题目分析专家。请识别图片中的{subject}题目文字内容。\n\n【重要】OCR文本提取要求：\n1. 提取纯文本内容，不要使用LaTeX格式\n2. 化学方程式用普通文字描述，如：2H2加O2反应生成2H2O\n3. 分子式用普通文字，如：H2O、NaCl\n4. 避免使用\\rightarrow、\\text等LaTeX命令\n5. 保持文本简洁易读".to_string(),
            classification_prompt: "请分析这道{subject}题目的类型，并生成相关的{subject}标签（如有机化学、无机化学、物理化学、分析化学等）。".to_string(),
            consolidated_review_prompt: "你是一个{subject}回顾分析专家。请对以下多道{subject}错题进行统一分析，找出共同的易错点、知识盲区和学习模式。请提供：1. 错题间的关联性分析 2. 主要易错点总结 3. 知识点掌握情况评估 4. 针对性的学习建议和复习计划。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            anki_generation_prompt: "请根据以下{subject}科目的学习内容，生成适合制作Anki卡片的问题和答案对。每张卡片应测试一个单一的化学反应或概念。请以JSON数组格式返回结果，每个对象必须包含 \"front\" (字符串), \"back\" (字符串), \"tags\" (字符串数组) 三个字段。front字段应包含问题或概念名称，back字段应包含答案或解释，tags字段应包含相关的化学知识点标签。".to_string(),
        }
    }

    // 英语科目的专业提示词
    fn get_english_prompts(&self) -> SubjectPrompts {
        SubjectPrompts {
            analysis_prompt: "你是一个{subject}教学专家。请根据提供的题目信息，详细解答学生的问题。解答要包含语法解释、词汇分析和例句说明。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            review_prompt: "你是一个{subject}学习分析专家。请分析学生的多道{subject}错题，找出共同的易错点、知识盲区和学习模式，提供针对性的{subject}学习建议。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            chat_prompt: "基于这道{subject}题目，请回答学生的问题。回答要包含语法规则、词汇用法和语言表达技巧。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            ocr_prompt: "你是一个{subject}题目分析专家。请识别图片中的{subject}题目文字内容。\n\n【重要】OCR文本提取要求：\n1. 提取纯文本内容，包括题目、选项、文章等\n2. 保持英文单词和句子的完整性\n3. 注意大小写和标点符号\n4. 保持段落和换行结构\n5. 保持文本简洁易读".to_string(),
            classification_prompt: "请分析这道{subject}题目的类型，并生成相关的{subject}标签（如语法、词汇、阅读理解、写作、听力等）。".to_string(),
            consolidated_review_prompt: "你是一个{subject}回顾分析专家。请对以下多道{subject}错题进行统一分析，找出共同的易错点、知识盲区和学习模式。请提供：1. 错题间的关联性分析 2. 主要易错点总结 3. 知识点掌握情况评估 4. 针对性的学习建议和复习计划。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            anki_generation_prompt: "请根据以下{subject}科目的学习内容，生成适合制作Anki卡片的问题和答案对。每张卡片应测试一个单一的语法规则或词汇概念。请以JSON数组格式返回结果，每个对象必须包含 \"front\" (字符串), \"back\" (字符串), \"tags\" (字符串数组) 三个字段。front字段应包含问题或概念名称，back字段应包含答案或解释，tags字段应包含相关的英语知识点标签。".to_string(),
        }
    }

    // 语文科目的专业提示词
    fn get_chinese_prompts(&self) -> SubjectPrompts {
        SubjectPrompts {
            analysis_prompt: "你是一个{subject}教学专家。请根据提供的题目信息，详细解答学生的问题。解答要包含文本分析、语言表达和写作技巧。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            review_prompt: "你是一个{subject}学习分析专家。请分析学生的多道{subject}错题，找出共同的易错点、知识盲区和学习模式，提供针对性的{subject}学习建议。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            chat_prompt: "基于这道{subject}题目，请回答学生的问题。回答要包含文学理解、语言运用和表达技巧。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            ocr_prompt: "你是一个{subject}题目分析专家。请识别图片中的{subject}题目文字内容。\n\n【重要】OCR文本提取要求：\n1. 提取纯文本内容，包括文章、诗词、题目等\n2. 保持古诗词的格式和断句\n3. 注意繁体字和异体字的识别\n4. 保持标点符号的准确性\n5. 保持文本简洁易读".to_string(),
            classification_prompt: "请分析这道{subject}题目的类型，并生成相关的{subject}标签（如阅读理解、写作、古诗词、文言文等）。".to_string(),
            consolidated_review_prompt: "你是一个{subject}回顾分析专家。请对以下多道{subject}错题进行统一分析，找出共同的易错点、知识盲区和学习模式。请提供：1. 错题间的关联性分析 2. 主要易错点总结 3. 知识点掌握情况评估 4. 针对性的学习建议和复习计划。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            anki_generation_prompt: "请根据以下{subject}科目的学习内容，生成适合制作Anki卡片的问题和答案对。每张卡片应测试一个单一的文学知识或语言表达概念。请以JSON数组格式返回结果，每个对象必须包含 \"front\" (字符串), \"back\" (字符串), \"tags\" (字符串数组) 三个字段。front字段应包含问题或概念名称，back字段应包含答案或解释，tags字段应包含相关的语文知识点标签。".to_string(),
        }
    }

    // 生物科目的专业提示词
    fn get_biology_prompts(&self) -> SubjectPrompts {
        SubjectPrompts {
            analysis_prompt: "你是一个{subject}教学专家。请根据提供的题目信息，详细解答学生的问题。解答要包含生物概念、生理过程和实验原理。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            review_prompt: "你是一个{subject}学习分析专家。请分析学生的多道{subject}错题，找出共同的易错点、知识盲区和学习模式，提供针对性的{subject}学习建议。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            chat_prompt: "基于这道{subject}题目，请回答学生的问题。回答要包含相关的生物原理、生命过程和实验方法。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            ocr_prompt: "你是一个{subject}题目分析专家。请识别图片中的{subject}题目文字内容。\n\n【重要】OCR文本提取要求：\n1. 提取纯文本内容，不要使用LaTeX格式\n2. 生物化学公式用普通文字描述，如：葡萄糖C6H12O6\n3. 基因型用普通文字，如：AA、Aa、aa\n4. 避免使用LaTeX命令\n5. 保持文本简洁易读".to_string(),
            classification_prompt: "请分析这道{subject}题目的类型，并生成相关的{subject}标签（如细胞生物学、遗传学、生态学、进化论等）。".to_string(),
            consolidated_review_prompt: "你是一个{subject}回顾分析专家。请对以下多道{subject}错题进行统一分析，找出共同的易错点、知识盲区和学习模式。请提供：1. 错题间的关联性分析 2. 主要易错点总结 3. 知识点掌握情况评估 4. 针对性的学习建议和复习计划。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            anki_generation_prompt: "请根据以下{subject}科目的学习内容，生成适合制作Anki卡片的问题和答案对。每张卡片应测试一个单一的生物概念或生命过程。请以JSON数组格式返回结果，每个对象必须包含 \"front\" (字符串), \"back\" (字符串), \"tags\" (字符串数组) 三个字段。front字段应包含问题或概念名称，back字段应包含答案或解释，tags字段应包含相关的生物知识点标签。".to_string(),
        }
    }

    // 历史科目的专业提示词
    fn get_history_prompts(&self) -> SubjectPrompts {
        SubjectPrompts {
            analysis_prompt: "你是一个{subject}教学专家。请根据提供的题目信息，详细解答学生的问题。解答要包含历史背景、事件分析和影响评价。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            review_prompt: "你是一个{subject}学习分析专家。请分析学生的多道{subject}错题，找出共同的易错点、知识盲区和学习模式，提供针对性的{subject}学习建议。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            chat_prompt: "基于这道{subject}题目，请回答学生的问题。回答要包含历史事件、人物分析和时代背景。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            ocr_prompt: "你是一个{subject}题目分析专家。请识别图片中的{subject}题目文字内容。\n\n【重要】OCR文本提取要求：\n1. 提取纯文本内容，包括题目、史料、时间等\n2. 注意历史人名和地名的准确性\n3. 保持时间表述的完整性\n4. 注意朝代和年号的正确识别\n5. 保持文本简洁易读".to_string(),
            classification_prompt: "请分析这道{subject}题目的类型，并生成相关的{subject}标签（如古代史、近代史、现代史、政治史、经济史等）。".to_string(),
            consolidated_review_prompt: "你是一个{subject}回顾分析专家。请对以下多道{subject}错题进行统一分析，找出共同的易错点、知识盲区和学习模式。请提供：1. 错题间的关联性分析 2. 主要易错点总结 3. 知识点掌握情况评估 4. 针对性的学习建议和复习计划。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            anki_generation_prompt: "请根据以下{subject}科目的学习内容，生成适合制作Anki卡片的问题和答案对。每张卡片应测试一个单一的历史事件或人物概念。请以JSON数组格式返回结果，每个对象必须包含 \"front\" (字符串), \"back\" (字符串), \"tags\" (字符串数组) 三个字段。front字段应包含问题或概念名称，back字段应包含答案或解释，tags字段应包含相关的历史知识点标签。".to_string(),
        }
    }

    // 地理科目的专业提示词
    fn get_geography_prompts(&self) -> SubjectPrompts {
        SubjectPrompts {
            analysis_prompt: "你是一个{subject}教学专家。请根据提供的题目信息，详细解答学生的问题。解答要包含地理概念、空间分析和区域特征。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            review_prompt: "你是一个{subject}学习分析专家。请分析学生的多道{subject}错题，找出共同的易错点、知识盲区和学习模式，提供针对性的{subject}学习建议。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            chat_prompt: "基于这道{subject}题目，请回答学生的问题。回答要包含地理原理、空间关系和区域分析。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            ocr_prompt: "你是一个{subject}题目分析专家。请识别图片中的{subject}题目文字内容。\n\n【重要】OCR文本提取要求：\n1. 提取纯文本内容，包括题目、地名、数据等\n2. 注意地名和专业术语的准确性\n3. 保持经纬度、海拔等数据的完整性\n4. 注意图表和统计数据的正确识别\n5. 保持文本简洁易读".to_string(),
            classification_prompt: "请分析这道{subject}题目的类型，并生成相关的{subject}标签（如自然地理、人文地理、区域地理、地图分析等）。".to_string(),
            consolidated_review_prompt: "你是一个{subject}回顾分析专家。请对以下多道{subject}错题进行统一分析，找出共同的易错点、知识盲区和学习模式。请提供：1. 错题间的关联性分析 2. 主要易错点总结 3. 知识点掌握情况评估 4. 针对性的学习建议和复习计划。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            anki_generation_prompt: "请根据以下{subject}科目的学习内容，生成适合制作Anki卡片的问题和答案对。每张卡片应测试一个单一的地理概念或区域特征。请以JSON数组格式返回结果，每个对象必须包含 \"front\" (字符串), \"back\" (字符串), \"tags\" (字符串数组) 三个字段。front字段应包含问题或概念名称，back字段应包含答案或解释，tags字段应包含相关的地理知识点标签。".to_string(),
        }
    }

    // 政治科目的专业提示词
    fn get_politics_prompts(&self) -> SubjectPrompts {
        SubjectPrompts {
            analysis_prompt: "你是一个{subject}教学专家。请根据提供的题目信息，详细解答学生的问题。解答要包含政治理论、政策分析和思想原理。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            review_prompt: "你是一个{subject}学习分析专家。请分析学生的多道{subject}错题，找出共同的易错点、知识盲区和学习模式，提供针对性的{subject}学习建议。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            chat_prompt: "基于这道{subject}题目，请回答学生的问题。回答要包含政治原理、政策解读和思想分析。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            ocr_prompt: "你是一个{subject}题目分析专家。请识别图片中的{subject}题目文字内容。\n\n【重要】OCR文本提取要求：\n1. 提取纯文本内容，包括题目、理论、政策等\n2. 注意政治术语和概念的准确性\n3. 保持理论表述的完整性\n4. 注意政策名称和法规条文的正确识别\n5. 保持文本简洁易读".to_string(),
            classification_prompt: "请分析这道{subject}题目的类型，并生成相关的{subject}标签（如马克思主义、政治制度、经济政策、哲学原理等）。".to_string(),
            consolidated_review_prompt: "你是一个{subject}回顾分析专家。请对以下多道{subject}错题进行统一分析，找出共同的易错点、知识盲区和学习模式。请提供：1. 错题间的关联性分析 2. 主要易错点总结 3. 知识点掌握情况评估 4. 针对性的学习建议和复习计划。\n\n【LaTeX 数学公式输出规范】\n1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。\n2. 行内公式使用 `$...$` 包裹，例如：`$E=mc^2$`。\n3. 独立展示的公式或方程组使用 `$$...$$` 包裹。\n4. 对于矩阵，请务必使用 `bmatrix` 环境，例如：`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$`。在 `bmatrix` 环境中，使用 `&` 分隔列元素，使用 `\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 `\\\\\\\\`) 换行。\n5. 确保所有LaTeX环境（如 `bmatrix`）和括号都正确配对和闭合。\n6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。".to_string(),
            anki_generation_prompt: "请根据以下{subject}科目的学习内容，生成适合制作Anki卡片的问题和答案对。每张卡片应测试一个单一的政治理论或制度概念。请以JSON数组格式返回结果，每个对象必须包含 \"front\" (字符串), \"back\" (字符串), \"tags\" (字符串数组) 三个字段。front字段应包含问题或概念名称，back字段应包含答案或解释，tags字段应包含相关的政治知识点标签。".to_string(),
        }
    }

    /// 保存模型分配配置
    pub fn save_model_assignments(&self, assignments: &crate::models::ModelAssignments) -> Result<()> {
        let assignments_json = serde_json::to_string(assignments)?;
        self.save_setting("model_assignments", &assignments_json)
    }

    /// 获取模型分配配置
    pub fn get_model_assignments(&self) -> Result<Option<crate::models::ModelAssignments>> {
        match self.get_setting("model_assignments")? {
            Some(json_str) => {
                let assignments: crate::models::ModelAssignments = serde_json::from_str(&json_str)?;
                Ok(Some(assignments))
            }
            None => Ok(None)
        }
    }

    /// 保存API配置列表
    pub fn save_api_configs(&self, configs: &[crate::llm_manager::ApiConfig]) -> Result<()> {
        let configs_json = serde_json::to_string(configs)?;
        self.save_setting("api_configs", &configs_json)
    }

    /// 获取API配置列表
    pub fn get_api_configs(&self) -> Result<Vec<crate::llm_manager::ApiConfig>> {
        match self.get_setting("api_configs")? {
            Some(json_str) => {
                let configs: Vec<crate::llm_manager::ApiConfig> = serde_json::from_str(&json_str)?;
                Ok(configs)
            }
            None => Ok(Vec::new())
        }
    }

    /// 验证错题的JSON字段格式，防止存储损坏的数据
    fn validate_mistake_json_fields(&self, mistake: &MistakeItem) -> Result<()> {
        // 验证question_images能够正确序列化和反序列化
        let question_images_json = serde_json::to_string(&mistake.question_images)
            .map_err(|e| anyhow::Error::new(e).context("序列化question_images失败"))?;
        
        serde_json::from_str::<Vec<String>>(&question_images_json)
            .map_err(|e| anyhow::Error::new(e).context("验证question_images JSON格式失败"))?;
        
        // 验证analysis_images能够正确序列化和反序列化
        let analysis_images_json = serde_json::to_string(&mistake.analysis_images)
            .map_err(|e| anyhow::Error::new(e).context("序列化analysis_images失败"))?;
        
        serde_json::from_str::<Vec<String>>(&analysis_images_json)
            .map_err(|e| anyhow::Error::new(e).context("验证analysis_images JSON格式失败"))?;
        
        // 验证tags能够正确序列化和反序列化
        let tags_json = serde_json::to_string(&mistake.tags)
            .map_err(|e| anyhow::Error::new(e).context("序列化tags失败"))?;
        
        serde_json::from_str::<Vec<String>>(&tags_json)
            .map_err(|e| anyhow::Error::new(e).context("验证tags JSON格式失败"))?;
        
        // 额外验证：检查图片路径的有效性
        for (i, path) in mistake.question_images.iter().enumerate() {
            if path.is_empty() {
                return Err(anyhow::Error::msg(format!("question_images[{}] 路径为空", i)));
            }
            // 基本路径格式检查
            if path.contains("..") || path.starts_with('/') {
                return Err(anyhow::Error::msg(format!("question_images[{}] 路径格式不安全: {}", i, path)));
            }
        }
        
        for (i, path) in mistake.analysis_images.iter().enumerate() {
            if path.is_empty() {
                return Err(anyhow::Error::msg(format!("analysis_images[{}] 路径为空", i)));
            }
            // 基本路径格式检查
            if path.contains("..") || path.starts_with('/') {
                return Err(anyhow::Error::msg(format!("analysis_images[{}] 路径格式不安全: {}", i, path)));
            }
        }
        
        println!("错题JSON字段验证通过: question_images={}, analysis_images={}, tags={}", 
            mistake.question_images.len(), mistake.analysis_images.len(), mistake.tags.len());
        
        Ok(())
    }

    /// 解析科目提示词，自动处理缺失的anki_generation_prompt字段（向后兼容）
    fn parse_subject_prompts(&self, prompts_str: &str) -> rusqlite::Result<SubjectPrompts> {
        // 尝试直接解析现有格式
        if let Ok(prompts) = serde_json::from_str::<SubjectPrompts>(prompts_str) {
            return Ok(prompts);
        }
        
        // 如果解析失败，可能是因为缺少新字段，尝试解析为旧格式
        #[derive(serde::Deserialize)]
        struct LegacySubjectPrompts {
            analysis_prompt: String,
            review_prompt: String,
            chat_prompt: String,
            ocr_prompt: String,
            classification_prompt: String,
            consolidated_review_prompt: String,
        }
        
        match serde_json::from_str::<LegacySubjectPrompts>(prompts_str) {
            Ok(legacy) => {
                // 转换为新格式，添加默认的anki_generation_prompt
                Ok(SubjectPrompts {
                    analysis_prompt: legacy.analysis_prompt,
                    review_prompt: legacy.review_prompt,
                    chat_prompt: legacy.chat_prompt,
                    ocr_prompt: legacy.ocr_prompt,
                    classification_prompt: legacy.classification_prompt,
                    consolidated_review_prompt: legacy.consolidated_review_prompt,
                    anki_generation_prompt: "请根据以下学习内容，生成适合制作Anki卡片的问题和答案对。请以JSON数组格式返回结果，每个对象必须包含 front（字符串），back（字符串），tags（字符串数组）三个字段。".to_string(),
                })
            },
            Err(_) => {
                Err(rusqlite::Error::InvalidColumnType(5, "prompts".to_string(), rusqlite::types::Type::Text))
            }
        }
    }

    // =================== Anki Enhancement Functions ===================

    /// 插入文档任务
    pub fn insert_document_task(&self, task: &DocumentTask) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO document_tasks 
             (id, document_id, original_document_name, segment_index, content_segment, 
              status, created_at, updated_at, error_message, subject_name, anki_generation_options_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                task.id,
                task.document_id,
                task.original_document_name,
                task.segment_index,
                task.content_segment,
                task.status.to_db_string(),
                task.created_at,
                task.updated_at,
                task.error_message,
                task.subject_name,
                task.anki_generation_options_json
            ]
        )?;
        Ok(())
    }

    /// 更新文档任务状态
    pub fn update_document_task_status(&self, task_id: &str, status: TaskStatus, error_message: Option<String>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let updated_at = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE document_tasks SET status = ?1, error_message = ?2, updated_at = ?3 WHERE id = ?4",
            params![
                status.to_db_string(),
                error_message,
                updated_at,
                task_id
            ]
        )?;
        Ok(())
    }

    /// 获取单个文档任务
    pub fn get_document_task(&self, task_id: &str) -> Result<DocumentTask> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, document_id, original_document_name, segment_index, content_segment, 
                    status, created_at, updated_at, error_message, subject_name, anki_generation_options_json
             FROM document_tasks WHERE id = ?1"
        )?;
        
        let task = stmt.query_row(params![task_id], |row| {
            let status_str: String = row.get(5)?;
            let status: TaskStatus = serde_json::from_str(&status_str)
                .map_err(|_| rusqlite::Error::InvalidColumnType(5, "status".to_string(), rusqlite::types::Type::Text))?;
            
            Ok(DocumentTask {
                id: row.get(0)?,
                document_id: row.get(1)?,
                original_document_name: row.get(2)?,
                segment_index: row.get(3)?,
                content_segment: row.get(4)?,
                status,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
                error_message: row.get(8)?,
                subject_name: row.get(9)?,
                anki_generation_options_json: row.get(10)?,
            })
        })?;
        
        Ok(task)
    }

    /// 获取指定文档的所有任务
    pub fn get_tasks_for_document(&self, document_id: &str) -> Result<Vec<DocumentTask>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, document_id, original_document_name, segment_index, content_segment, 
                    status, created_at, updated_at, error_message, subject_name, anki_generation_options_json
             FROM document_tasks WHERE document_id = ?1 ORDER BY segment_index"
        )?;
        
        let task_iter = stmt.query_map(params![document_id], |row| {
            let status_str: String = row.get(5)?;
            let status: TaskStatus = serde_json::from_str(&status_str)
                .map_err(|_| rusqlite::Error::InvalidColumnType(5, "status".to_string(), rusqlite::types::Type::Text))?;
            
            Ok(DocumentTask {
                id: row.get(0)?,
                document_id: row.get(1)?,
                original_document_name: row.get(2)?,
                segment_index: row.get(3)?,
                content_segment: row.get(4)?,
                status,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
                error_message: row.get(8)?,
                subject_name: row.get(9)?,
                anki_generation_options_json: row.get(10)?,
            })
        })?;

        let mut tasks = Vec::new();
        for task in task_iter {
            tasks.push(task?);
        }
        
        Ok(tasks)
    }

    /// 插入Anki卡片
    pub fn insert_anki_card(&self, card: &AnkiCard) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO anki_cards 
             (id, task_id, front, back, text, tags_json, images_json, 
              is_error_card, error_content, card_order_in_task, created_at, updated_at,
              extra_fields_json, template_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                card.id,
                card.task_id,
                card.front,
                card.back,
                card.text,
                serde_json::to_string(&card.tags)?,
                serde_json::to_string(&card.images)?,
                if card.is_error_card { 1 } else { 0 },
                card.error_content,
                0, // card_order_in_task will be calculated
                card.created_at,
                card.updated_at,
                serde_json::to_string(&card.extra_fields)?,
                card.template_id
            ]
        )?;
        Ok(())
    }

    /// 获取指定任务的所有卡片
    pub fn get_cards_for_task(&self, task_id: &str) -> Result<Vec<AnkiCard>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, task_id, front, back, text, tags_json, images_json, 
                    is_error_card, error_content, created_at, updated_at,
                    COALESCE(extra_fields_json, '{}') as extra_fields_json,
                    template_id
             FROM anki_cards WHERE task_id = ?1 ORDER BY card_order_in_task, created_at"
        )?;
        
        let card_iter = stmt.query_map(params![task_id], |row| {
            let tags_json: String = row.get(5)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            
            let images_json: String = row.get(6)?;
            let images: Vec<String> = serde_json::from_str(&images_json).unwrap_or_default();
            
            let extra_fields_json: String = row.get(11)?;
            let extra_fields: std::collections::HashMap<String, String> = 
                serde_json::from_str(&extra_fields_json).unwrap_or_default();
            
            Ok(AnkiCard {
                id: row.get(0)?,
                task_id: row.get(1)?,
                front: row.get(2)?,
                back: row.get(3)?,
                text: row.get(4)?,
                tags,
                images,
                is_error_card: row.get::<_, i32>(7)? != 0,
                error_content: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                extra_fields,
                template_id: row.get(12)?,
            })
        })?;

        let mut cards = Vec::new();
        for card in card_iter {
            cards.push(card?);
        }
        
        Ok(cards)
    }

    /// 获取指定文档的所有卡片
    pub fn get_cards_for_document(&self, document_id: &str) -> Result<Vec<AnkiCard>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT ac.id, ac.task_id, ac.front, ac.back, ac.text, ac.tags_json, ac.images_json, 
                    ac.is_error_card, ac.error_content, ac.created_at, ac.updated_at,
                    COALESCE(ac.extra_fields_json, '{}') as extra_fields_json,
                    ac.template_id
             FROM anki_cards ac
             JOIN document_tasks dt ON ac.task_id = dt.id
             WHERE dt.document_id = ?1 
             ORDER BY dt.segment_index, ac.card_order_in_task, ac.created_at"
        )?;
        
        let card_iter = stmt.query_map(params![document_id], |row| {
            let tags_json: String = row.get(5)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            
            let images_json: String = row.get(6)?;
            let images: Vec<String> = serde_json::from_str(&images_json).unwrap_or_default();
            
            let extra_fields_json: String = row.get(11)?;
            let extra_fields: std::collections::HashMap<String, String> = 
                serde_json::from_str(&extra_fields_json).unwrap_or_default();
            
            Ok(AnkiCard {
                id: row.get(0)?,
                task_id: row.get(1)?,
                front: row.get(2)?,
                back: row.get(3)?,
                text: row.get(4)?,
                tags,
                images,
                is_error_card: row.get::<_, i32>(7)? != 0,
                error_content: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                extra_fields,
                template_id: row.get(12)?,
            })
        })?;

        let mut cards = Vec::new();
        for card in card_iter {
            cards.push(card?);
        }
        
        Ok(cards)
    }

    /// 根据ID列表获取卡片
    pub fn get_cards_by_ids(&self, card_ids: &[String]) -> Result<Vec<AnkiCard>> {
        if card_ids.is_empty() {
            return Ok(Vec::new());
        }
        
        let conn = self.conn.lock().unwrap();
        let placeholders: Vec<&str> = card_ids.iter().map(|_| "?").collect();
        let sql = format!(
            "SELECT id, task_id, front, back, text, tags_json, images_json, 
                    is_error_card, error_content, created_at, updated_at,
                    COALESCE(extra_fields_json, '{{}}') as extra_fields_json,
                    template_id
             FROM anki_cards WHERE id IN ({}) ORDER BY created_at",
            placeholders.join(",")
        );
        
        let mut stmt = conn.prepare(&sql)?;
        let card_iter = stmt.query_map(rusqlite::params_from_iter(card_ids), |row| {
            let tags_json: String = row.get(5)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            
            let images_json: String = row.get(6)?;
            let images: Vec<String> = serde_json::from_str(&images_json).unwrap_or_default();
            
            let extra_fields_json: String = row.get(11)?;
            let extra_fields: std::collections::HashMap<String, String> = 
                serde_json::from_str(&extra_fields_json).unwrap_or_default();
            
            Ok(AnkiCard {
                id: row.get(0)?,
                task_id: row.get(1)?,
                front: row.get(2)?,
                back: row.get(3)?,
                text: row.get(4)?,
                tags,
                images,
                is_error_card: row.get::<_, i32>(7)? != 0,
                error_content: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                extra_fields,
                template_id: row.get(12)?,
            })
        })?;

        let mut cards = Vec::new();
        for card in card_iter {
            cards.push(card?);
        }
        
        Ok(cards)
    }

    /// 更新Anki卡片
    pub fn update_anki_card(&self, card: &AnkiCard) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let updated_at = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE anki_cards SET 
             front = ?1, back = ?2, tags_json = ?3, images_json = ?4, 
             is_error_card = ?5, error_content = ?6, updated_at = ?7
             WHERE id = ?8",
            params![
                card.front,
                card.back,
                serde_json::to_string(&card.tags)?,
                serde_json::to_string(&card.images)?,
                if card.is_error_card { 1 } else { 0 },
                card.error_content,
                updated_at,
                card.id
            ]
        )?;
        Ok(())
    }

    /// 删除Anki卡片
    pub fn delete_anki_card(&self, card_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM anki_cards WHERE id = ?1", params![card_id])?;
        Ok(())
    }

    /// 删除文档任务及其所有卡片
    pub fn delete_document_task(&self, task_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        // 由于设置了ON DELETE CASCADE，删除任务会自动删除关联的卡片
        conn.execute("DELETE FROM document_tasks WHERE id = ?1", params![task_id])?;
        Ok(())
    }

    /// 删除整个文档会话（所有任务和卡片）
    pub fn delete_document_session(&self, document_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        // 由于设置了ON DELETE CASCADE，删除任务会自动删除关联的卡片
        conn.execute("DELETE FROM document_tasks WHERE document_id = ?1", params![document_id])?;
        Ok(())
    }

    // ==================== RAG配置管理 ====================

    /// 获取RAG配置
    pub fn get_rag_configuration(&self) -> Result<Option<crate::models::RagConfiguration>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, chunk_size, chunk_overlap, chunking_strategy, min_chunk_size, 
                    default_top_k, default_rerank_enabled, created_at, updated_at 
             FROM rag_configurations WHERE id = 'default'"
        )?;
        
        let result = stmt.query_row([], |row| {
            let created_at_str: String = row.get(7)?;
            let updated_at_str: String = row.get(8)?;
            
            let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                .map_err(|_| rusqlite::Error::InvalidColumnType(7, "created_at".to_string(), rusqlite::types::Type::Text))?
                .with_timezone(&Utc);
            let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
                .map_err(|_| rusqlite::Error::InvalidColumnType(8, "updated_at".to_string(), rusqlite::types::Type::Text))?
                .with_timezone(&Utc);
            
            Ok(crate::models::RagConfiguration {
                id: row.get(0)?,
                chunk_size: row.get(1)?,
                chunk_overlap: row.get(2)?,
                chunking_strategy: row.get(3)?,
                min_chunk_size: row.get(4)?,
                default_top_k: row.get(5)?,
                default_rerank_enabled: row.get::<_, i32>(6)? != 0,
                created_at,
                updated_at,
            })
        }).optional()?;
        
        Ok(result)
    }

    /// 更新RAG配置
    pub fn update_rag_configuration(&self, config: &crate::models::RagConfigRequest) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        
        conn.execute(
            "UPDATE rag_configurations 
             SET chunk_size = ?1, chunk_overlap = ?2, chunking_strategy = ?3, 
                 min_chunk_size = ?4, default_top_k = ?5, default_rerank_enabled = ?6, 
                 updated_at = ?7
             WHERE id = 'default'",
            params![
                config.chunk_size,
                config.chunk_overlap,
                config.chunking_strategy,
                config.min_chunk_size,
                config.default_top_k,
                if config.default_rerank_enabled { 1 } else { 0 },
                now
            ],
        )?;
        
        Ok(())
    }

    /// 重置RAG配置为默认值
    pub fn reset_rag_configuration(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        
        conn.execute(
            "UPDATE rag_configurations 
             SET chunk_size = 512, chunk_overlap = 50, chunking_strategy = 'fixed_size', 
                 min_chunk_size = 20, default_top_k = 5, default_rerank_enabled = 0, 
                 updated_at = ?1
             WHERE id = 'default'",
            params![now],
        )?;
        
        Ok(())
    }

    // ==================== RAG分库管理CRUD操作 ====================

    /// 创建新的分库
    pub fn create_sub_library(&self, request: &CreateSubLibraryRequest) -> Result<SubLibrary> {
        let conn = self.conn.lock().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        
        // 检查名称是否已存在
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM rag_sub_libraries WHERE name = ?1)",
            params![request.name],
            |row| row.get(0)
        )?;
        
        if exists {
            return Err(anyhow::anyhow!("分库名称 '{}' 已存在", request.name));
        }
        
        conn.execute(
            "INSERT INTO rag_sub_libraries (id, name, description, created_at, updated_at) 
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, request.name, request.description, now_str, now_str],
        )?;
        
        Ok(SubLibrary {
            id,
            name: request.name.clone(),
            description: request.description.clone(),
            created_at: now,
            updated_at: now,
            document_count: 0,
            chunk_count: 0,
        })
    }

    /// 获取所有分库列表
    pub fn list_sub_libraries(&self) -> Result<Vec<SubLibrary>> {
        let conn = self.conn.lock().unwrap();
        
        let mut stmt = conn.prepare(
            "SELECT sl.id, sl.name, sl.description, sl.created_at, sl.updated_at,
                    COUNT(DISTINCT rd.id) as document_count,
                    COUNT(DISTINCT rdc.id) as chunk_count
             FROM rag_sub_libraries sl
             LEFT JOIN rag_documents rd ON sl.id = rd.sub_library_id
             LEFT JOIN rag_document_chunks rdc ON rd.id = rdc.document_id
             GROUP BY sl.id, sl.name, sl.description, sl.created_at, sl.updated_at
             ORDER BY sl.name"
        )?;
        
        let library_iter = stmt.query_map([], |row| {
            let created_at = DateTime::parse_from_rfc3339(&row.get::<_, String>(3)?)
                .map_err(|_| rusqlite::Error::InvalidColumnType(3, "created_at".to_string(), rusqlite::types::Type::Text))?
                .with_timezone(&Utc);
            let updated_at = DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                .map_err(|_| rusqlite::Error::InvalidColumnType(4, "updated_at".to_string(), rusqlite::types::Type::Text))?
                .with_timezone(&Utc);
            
            Ok(SubLibrary {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                created_at,
                updated_at,
                document_count: row.get::<_, i64>(5)? as usize,
                chunk_count: row.get::<_, i64>(6)? as usize,
            })
        })?;
        
        let mut libraries = Vec::new();
        for library in library_iter {
            libraries.push(library?);
        }
        
        Ok(libraries)
    }

    /// 根据ID获取分库详情
    pub fn get_sub_library_by_id(&self, id: &str) -> Result<Option<SubLibrary>> {
        let conn = self.conn.lock().unwrap();
        
        let result = conn.query_row(
            "SELECT sl.id, sl.name, sl.description, sl.created_at, sl.updated_at,
                    COUNT(DISTINCT rd.id) as document_count,
                    COUNT(DISTINCT rdc.id) as chunk_count
             FROM rag_sub_libraries sl
             LEFT JOIN rag_documents rd ON sl.id = rd.sub_library_id
             LEFT JOIN rag_document_chunks rdc ON rd.id = rdc.document_id
             WHERE sl.id = ?1
             GROUP BY sl.id, sl.name, sl.description, sl.created_at, sl.updated_at",
            params![id],
            |row| {
                let created_at = DateTime::parse_from_rfc3339(&row.get::<_, String>(3)?)
                    .map_err(|_| rusqlite::Error::InvalidColumnType(3, "created_at".to_string(), rusqlite::types::Type::Text))?
                    .with_timezone(&Utc);
                let updated_at = DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                    .map_err(|_| rusqlite::Error::InvalidColumnType(4, "updated_at".to_string(), rusqlite::types::Type::Text))?
                    .with_timezone(&Utc);
                
                Ok(SubLibrary {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    created_at,
                    updated_at,
                    document_count: row.get::<_, i64>(5)? as usize,
                    chunk_count: row.get::<_, i64>(6)? as usize,
                })
            }
        ).optional()?;
        
        Ok(result)
    }

    /// 根据名称获取分库详情
    pub fn get_sub_library_by_name(&self, name: &str) -> Result<Option<SubLibrary>> {
        let conn = self.conn.lock().unwrap();
        
        let result = conn.query_row(
            "SELECT sl.id, sl.name, sl.description, sl.created_at, sl.updated_at,
                    COUNT(DISTINCT rd.id) as document_count,
                    COUNT(DISTINCT rdc.id) as chunk_count
             FROM rag_sub_libraries sl
             LEFT JOIN rag_documents rd ON sl.id = rd.sub_library_id
             LEFT JOIN rag_document_chunks rdc ON rd.id = rdc.document_id
             WHERE sl.name = ?1
             GROUP BY sl.id, sl.name, sl.description, sl.created_at, sl.updated_at",
            params![name],
            |row| {
                let created_at = DateTime::parse_from_rfc3339(&row.get::<_, String>(3)?)
                    .map_err(|_| rusqlite::Error::InvalidColumnType(3, "created_at".to_string(), rusqlite::types::Type::Text))?
                    .with_timezone(&Utc);
                let updated_at = DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                    .map_err(|_| rusqlite::Error::InvalidColumnType(4, "updated_at".to_string(), rusqlite::types::Type::Text))?
                    .with_timezone(&Utc);
                
                Ok(SubLibrary {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    created_at,
                    updated_at,
                    document_count: row.get::<_, i64>(5)? as usize,
                    chunk_count: row.get::<_, i64>(6)? as usize,
                })
            }
        ).optional()?;
        
        Ok(result)
    }

    /// 更新分库信息
    pub fn update_sub_library(&self, id: &str, request: &UpdateSubLibraryRequest) -> Result<SubLibrary> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        
        // 检查分库是否存在
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM rag_sub_libraries WHERE id = ?1)",
            params![id],
            |row| row.get(0)
        )?;
        
        if !exists {
            return Err(anyhow::anyhow!("分库ID '{}' 不存在", id));
        }
        
        // 如果更新名称，检查新名称是否已存在
        if let Some(new_name) = &request.name {
            let name_exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM rag_sub_libraries WHERE name = ?1 AND id != ?2)",
                params![new_name, id],
                |row| row.get(0)
            )?;
            
            if name_exists {
                return Err(anyhow::anyhow!("分库名称 '{}' 已存在", new_name));
            }
        }
        
        // 构建动态更新SQL
        let mut updates = Vec::new();
        let mut params_vec = Vec::new();
        
        if let Some(name) = &request.name {
            updates.push("name = ?");
            params_vec.push(name.as_str());
        }
        
        if let Some(description) = &request.description {
            updates.push("description = ?");
            params_vec.push(description.as_str());
        }
        
        updates.push("updated_at = ?");
        params_vec.push(&now);
        params_vec.push(id);
        
        let sql = format!(
            "UPDATE rag_sub_libraries SET {} WHERE id = ?",
            updates.join(", ")
        );
        
        conn.execute(&sql, rusqlite::params_from_iter(params_vec))?;
        
        // 释放锁，避免递归锁导致死锁
        drop(conn);
        
        // 使用单独的只读查询获取更新后的分库信息
        self.get_sub_library_by_id(id)?
            .ok_or_else(|| anyhow::anyhow!("无法获取更新后的分库信息"))
    }

    /// 删除分库
    pub fn delete_sub_library(&self, id: &str, delete_contained_documents: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        
        // 检查是否为默认分库
        if id == "default" {
            return Err(anyhow::anyhow!("不能删除默认分库"));
        }
        
        // 检查分库是否存在
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM rag_sub_libraries WHERE id = ?1)",
            params![id],
            |row| row.get(0)
        )?;
        
        if !exists {
            return Err(anyhow::anyhow!("分库ID '{}' 不存在", id));
        }
        
        let transaction = conn.unchecked_transaction()?;
        
        if delete_contained_documents {
            // 删除分库中的所有文档及其相关数据
            // 首先获取分库中的所有文档ID
            let mut stmt = transaction.prepare(
                "SELECT id FROM rag_documents WHERE sub_library_id = ?1"
            )?;
            
            let document_ids: Vec<String> = stmt.query_map(params![id], |row| {
                Ok(row.get::<_, String>(0)?)
            })?.collect::<Result<Vec<_>, _>>()?;
            
            // 删除文档关联的向量和块
            for doc_id in document_ids {
                transaction.execute(
                    "DELETE FROM rag_document_chunks WHERE document_id = ?1",
                    params![doc_id],
                )?;
            }
            
            // 删除分库中的所有文档
            transaction.execute(
                "DELETE FROM rag_documents WHERE sub_library_id = ?1",
                params![id],
            )?;
        } else {
            // 将分库中的文档移动到默认分库
            transaction.execute(
                "UPDATE rag_documents SET sub_library_id = 'default' WHERE sub_library_id = ?1",
                params![id],
            )?;
        }
        
        // 删除分库本身
        transaction.execute(
            "DELETE FROM rag_sub_libraries WHERE id = ?1",
            params![id],
        )?;
        
        transaction.commit()?;
        
        println!("✅ 成功删除分库: {}", id);
        Ok(())
    }

    /// 获取指定分库中的文档列表
    pub fn get_documents_by_sub_library(&self, sub_library_id: &str, page: Option<usize>, page_size: Option<usize>) -> Result<Vec<serde_json::Value>> {
        let conn = self.conn.lock().unwrap();
        
        let page = page.unwrap_or(1);
        let page_size = page_size.unwrap_or(50);
        let offset = (page - 1) * page_size;
        
        let mut stmt = conn.prepare(
            "SELECT id, file_name, file_path, file_size, total_chunks, sub_library_id, created_at, updated_at 
             FROM rag_documents 
             WHERE sub_library_id = ?1
             ORDER BY created_at DESC 
             LIMIT ?2 OFFSET ?3"
        )?;
        
        let rows = stmt.query_map(params![sub_library_id, page_size, offset], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "file_name": row.get::<_, String>(1)?,
                "file_path": row.get::<_, Option<String>>(2)?,
                "file_size": row.get::<_, Option<i64>>(3)?,
                "total_chunks": row.get::<_, i32>(4)?,
                "sub_library_id": row.get::<_, String>(5)?,
                "created_at": row.get::<_, String>(6)?,
                "updated_at": row.get::<_, String>(7)?
            }))
        })?;
        
        let mut documents = Vec::new();
        for row in rows {
            documents.push(row?);
        }
        
        Ok(documents)
    }

    /// 将文档移动到指定分库
    pub fn move_document_to_sub_library(&self, document_id: &str, target_sub_library_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        
        // 检查目标分库是否存在
        let library_exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM rag_sub_libraries WHERE id = ?1)",
            params![target_sub_library_id],
            |row| row.get(0)
        )?;
        
        if !library_exists {
            return Err(anyhow::anyhow!("目标分库ID '{}' 不存在", target_sub_library_id));
        }
        
        // 检查文档是否存在
        let document_exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM rag_documents WHERE id = ?1)",
            params![document_id],
            |row| row.get(0)
        )?;
        
        if !document_exists {
            return Err(anyhow::anyhow!("文档ID '{}' 不存在", document_id));
        }
        
        // 更新文档的分库归属
        let updated_at = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE rag_documents SET sub_library_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![target_sub_library_id, updated_at, document_id],
        )?;
        
        println!("✅ 成功将文档 {} 移动到分库 {}", document_id, target_sub_library_id);
        Ok(())
    }

    // =================== Migration Functions ===================

    /// 版本8到版本9的数据库迁移：添加图片遮罩卡表
    fn migrate_v8_to_v9(&self, conn: &rusqlite::Connection) -> Result<()> {
        println!("正在迁移数据库版本8到版本9：添加图片遮罩卡支持...");
        
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS image_occlusion_cards (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                image_path TEXT NOT NULL,
                image_base64 TEXT,
                image_width INTEGER NOT NULL,
                image_height INTEGER NOT NULL,
                masks_json TEXT NOT NULL,  -- JSON格式存储遮罩数组
                title TEXT NOT NULL,
                description TEXT,
                tags_json TEXT NOT NULL DEFAULT '[]',  -- JSON格式存储标签数组
                subject TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );
            CREATE INDEX IF NOT EXISTS idx_image_occlusion_cards_task_id ON image_occlusion_cards(task_id);
            CREATE INDEX IF NOT EXISTS idx_image_occlusion_cards_subject ON image_occlusion_cards(subject);
            CREATE INDEX IF NOT EXISTS idx_image_occlusion_cards_created_at ON image_occlusion_cards(created_at);"
        )?;
        
        println!("✅ 数据库版本8到版本9迁移完成");
        Ok(())
    }

    fn migrate_v9_to_v10(&self, conn: &rusqlite::Connection) -> Result<()> {
        println!("正在迁移数据库版本9到版本10：为anki_cards表添加text字段支持Cloze模板...");
        
        // 添加text字段到anki_cards表
        conn.execute(
            "ALTER TABLE anki_cards ADD COLUMN text TEXT;",
            [],
        )?;
        
        // 添加索引以优化查询性能
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_anki_cards_text ON anki_cards(text);",
            [],
        )?;
        
        println!("✅ 数据库版本9到版本10迁移完成");
        Ok(())
    }

    // =================== Image Occlusion Functions ===================

    /// 保存图片遮罩卡到数据库
    pub fn save_image_occlusion_card(&self, card: &crate::models::ImageOcclusionCard) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        
        let masks_json = serde_json::to_string(&card.masks)
            .map_err(|e| anyhow::anyhow!("序列化遮罩数据失败: {}", e))?;
        let tags_json = serde_json::to_string(&card.tags)
            .map_err(|e| anyhow::anyhow!("序列化标签数据失败: {}", e))?;
        
        conn.execute(
            "INSERT INTO image_occlusion_cards 
             (id, task_id, image_path, image_base64, image_width, image_height, masks_json, 
              title, description, tags_json, subject, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                card.id,
                card.task_id,
                card.image_path,
                card.image_base64,
                card.image_width,
                card.image_height,
                masks_json,
                card.title,
                card.description,
                tags_json,
                card.subject,
                card.created_at,
                card.updated_at
            ]
        )?;
        
        Ok(())
    }

    /// 获取所有图片遮罩卡
    pub fn get_all_image_occlusion_cards(&self) -> Result<Vec<crate::models::ImageOcclusionCard>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, task_id, image_path, image_base64, image_width, image_height, masks_json,
                    title, description, tags_json, subject, created_at, updated_at
             FROM image_occlusion_cards ORDER BY created_at DESC"
        )?;
        
        let card_iter = stmt.query_map([], |row| {
            let masks_json: String = row.get(6)?;
            let tags_json: String = row.get(9)?;
            
            let masks: Vec<crate::models::OcclusionMask> = serde_json::from_str(&masks_json)
                .map_err(|e| rusqlite::Error::InvalidColumnType(6, format!("masks_json: {}", e), rusqlite::types::Type::Text))?;
            let tags: Vec<String> = serde_json::from_str(&tags_json)
                .map_err(|e| rusqlite::Error::InvalidColumnType(9, format!("tags_json: {}", e), rusqlite::types::Type::Text))?;
            
            Ok(crate::models::ImageOcclusionCard {
                id: row.get(0)?,
                task_id: row.get(1)?,
                image_path: row.get(2)?,
                image_base64: row.get(3)?,
                image_width: row.get(4)?,
                image_height: row.get(5)?,
                masks,
                title: row.get(7)?,
                description: row.get(8)?,
                tags,
                subject: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        })?;
        
        let mut cards = Vec::new();
        for card_result in card_iter {
            cards.push(card_result?);
        }
        
        Ok(cards)
    }

    /// 根据ID获取图片遮罩卡
    pub fn get_image_occlusion_card_by_id(&self, card_id: &str) -> Result<Option<crate::models::ImageOcclusionCard>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, task_id, image_path, image_base64, image_width, image_height, masks_json,
                    title, description, tags_json, subject, created_at, updated_at
             FROM image_occlusion_cards WHERE id = ?1"
        )?;
        
        let card = stmt.query_row(params![card_id], |row| {
            let masks_json: String = row.get(6)?;
            let tags_json: String = row.get(9)?;
            
            let masks: Vec<crate::models::OcclusionMask> = serde_json::from_str(&masks_json)
                .map_err(|e| rusqlite::Error::InvalidColumnType(6, format!("masks_json: {}", e), rusqlite::types::Type::Text))?;
            let tags: Vec<String> = serde_json::from_str(&tags_json)
                .map_err(|e| rusqlite::Error::InvalidColumnType(9, format!("tags_json: {}", e), rusqlite::types::Type::Text))?;
            
            Ok(crate::models::ImageOcclusionCard {
                id: row.get(0)?,
                task_id: row.get(1)?,
                image_path: row.get(2)?,
                image_base64: row.get(3)?,
                image_width: row.get(4)?,
                image_height: row.get(5)?,
                masks,
                title: row.get(7)?,
                description: row.get(8)?,
                tags,
                subject: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        }).optional()?;
        
        Ok(card)
    }

    /// 更新图片遮罩卡
    pub fn update_image_occlusion_card(&self, card: &crate::models::ImageOcclusionCard) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        
        let masks_json = serde_json::to_string(&card.masks)
            .map_err(|e| anyhow::anyhow!("序列化遮罩数据失败: {}", e))?;
        let tags_json = serde_json::to_string(&card.tags)
            .map_err(|e| anyhow::anyhow!("序列化标签数据失败: {}", e))?;
        
        conn.execute(
            "UPDATE image_occlusion_cards SET
                task_id = ?2, image_path = ?3, image_base64 = ?4, image_width = ?5, image_height = ?6,
                masks_json = ?7, title = ?8, description = ?9, tags_json = ?10, subject = ?11, updated_at = ?12
             WHERE id = ?1",
            params![
                card.id,
                card.task_id,
                card.image_path,
                card.image_base64,
                card.image_width,
                card.image_height,
                masks_json,
                card.title,
                card.description,
                tags_json,
                card.subject,
                card.updated_at
            ]
        )?;
        
        Ok(())
    }

    /// 删除图片遮罩卡
    pub fn delete_image_occlusion_card(&self, card_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM image_occlusion_cards WHERE id = ?1",
            params![card_id]
        )?;
        Ok(())
    }

    /// 设置默认模板ID
    pub fn set_default_template(&self, template_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES ('default_template_id', ?1, ?2)",
            params![template_id, now]
        )?;
        
        Ok(())
    }

    /// 获取默认模板ID
    pub fn get_default_template(&self) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        
        match conn.query_row(
            "SELECT value FROM settings WHERE key = 'default_template_id'",
            [],
            |row| row.get::<_, String>(0)
        ) {
            Ok(template_id) => Ok(Some(template_id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}
