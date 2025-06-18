use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs::{File, self};
use std::io::Write;
use rusqlite::{Connection, Result as SqliteResult, params};
use zip::{ZipWriter, write::FileOptions};
use crate::models::AnkiCard;
use chrono::Utc;

/// 清理卡片内容中的无效模板占位符
fn clean_template_placeholders(content: &str) -> String {
    let mut cleaned = content.to_string();
    
    // 移除各种可能的占位符
    cleaned = cleaned.replace("{{.}}", "");
    cleaned = cleaned.replace("{{/}}", "");
    cleaned = cleaned.replace("{{#}}", "");
    cleaned = cleaned.replace("{{}}", "");
    
    // 移除空的Mustache标签 {{}}
    while cleaned.contains("{{}}") {
        cleaned = cleaned.replace("{{}}", "");
    }
    
    // 移除可能的空白标签
    cleaned = cleaned.replace("{{  }}", "");
    cleaned = cleaned.replace("{{ }}", "");
    
    // 清理多余的空白和换行
    cleaned.trim().to_string()
}

/// Anki的基本配置
const ANKI_COLLECTION_CONFIG: &str = r#"{
    "nextPos": 1,
    "estTimes": true,
    "activeDecks": [1],
    "sortType": "noteFld",
    "timeLim": 0,
    "sortBackwards": false,
    "addToCur": true,
    "curDeck": 1,
    "newBury": 0,
    "newSpread": 0,
    "dueCounts": true,
    "curModel": "1425279151691",
    "collapseTime": 1200
}"#;

#[derive(Serialize, Deserialize)]
struct AnkiModel {
    #[serde(rename = "vers")]
    version: Vec<i32>,
    name: String,
    #[serde(rename = "type")]
    model_type: i32,
    #[serde(rename = "mod")]
    modified: i64,
    #[serde(rename = "usn")]
    update_sequence_number: i32,
    #[serde(rename = "sortf")]
    sort_field: i32,
    #[serde(rename = "did")]
    deck_id: i64,
    #[serde(rename = "tmpls")]
    templates: Vec<AnkiTemplate>,
    #[serde(rename = "flds")]
    fields: Vec<AnkiField>,
    css: String,
    #[serde(rename = "latexPre")]
    latex_pre: String,
    #[serde(rename = "latexPost")]
    latex_post: String,
    tags: Vec<String>,
    id: String,
    req: Vec<Vec<serde_json::Value>>,
}

#[derive(Serialize, Deserialize)]
struct AnkiTemplate {
    name: String,
    ord: i32,
    qfmt: String,
    afmt: String,
    #[serde(rename = "bqfmt")]
    browser_qfmt: String,
    #[serde(rename = "bafmt")]
    browser_afmt: String,
    #[serde(rename = "did")]
    deck_id: Option<i64>,
    #[serde(rename = "bfont")]
    browser_font: String,
    #[serde(rename = "bsize")]
    browser_size: i32,
}

#[derive(Serialize, Deserialize)]
struct AnkiField {
    name: String,
    ord: i32,
    sticky: bool,
    rtl: bool,
    font: String,
    size: i32,
    #[serde(rename = "media")]
    media: Vec<String>,
    description: String,
}

/// 创建基本的Anki模型定义
fn create_basic_model() -> AnkiModel {
    AnkiModel {
        version: vec![],
        name: "Basic".to_string(),
        model_type: 0,
        modified: Utc::now().timestamp(),
        update_sequence_number: -1,
        sort_field: 0,
        deck_id: 1,
        templates: vec![AnkiTemplate {
            name: "Card 1".to_string(),
            ord: 0,
            qfmt: "{{Front}}".to_string(),
            afmt: "{{FrontSide}}\n\n<hr id=answer>\n\n{{Back}}".to_string(),
            browser_qfmt: "".to_string(),
            browser_afmt: "".to_string(),
            deck_id: None,
            browser_font: "Arial".to_string(),
            browser_size: 12,
        }],
        fields: vec![
            AnkiField {
                name: "Front".to_string(),
                ord: 0,
                sticky: false,
                rtl: false,
                font: "Arial".to_string(),
                size: 20,
                media: vec![],
                description: "".to_string(),
            },
            AnkiField {
                name: "Back".to_string(),
                ord: 1,
                sticky: false,
                rtl: false,
                font: "Arial".to_string(),
                size: 20,
                media: vec![],
                description: "".to_string(),
            },
        ],
        css: ".card {\n font-family: arial;\n font-size: 20px;\n text-align: center;\n color: black;\n background-color: white;\n}".to_string(),
        latex_pre: "\\documentclass[12pt]{article}\n\\special{papersize=3in,5in}\n\\usepackage[utf8]{inputenc}\n\\usepackage{amssymb,amsmath}\n\\pagestyle{empty}\n\\setlength{\\parindent}{0in}\n\\begin{document}\n".to_string(),
        latex_post: "\\end{document}".to_string(),
        tags: vec![],
        id: "1425279151691".to_string(),
        req: vec![vec![serde_json::Value::from(0), serde_json::Value::from("any"), serde_json::Value::Array(vec![serde_json::Value::from(0)])]],
    }
}

/// 根据模板创建自定义Anki模型定义
fn create_template_model(
    template_id: Option<&str>,
    template_name: &str,
    fields: &[String],
    front_template: &str,
    back_template: &str,
    css_style: &str,
    model_type: i32, // 新增参数
) -> AnkiModel {
    // 创建字段定义
    let anki_fields: Vec<AnkiField> = fields
        .iter()
        .enumerate()
        .map(|(i, field_name)| AnkiField {
            name: field_name.clone(),
            ord: i as i32,
            sticky: false,
            rtl: false,
            font: "Arial".to_string(),
            size: 20,
            media: vec![],
            description: "".to_string(),
        })
        .collect();

    let req = if model_type == 1 {
        // Cloze model requirement
        vec![vec![serde_json::Value::from(0), serde_json::Value::from("all"), serde_json::Value::Array(vec![serde_json::Value::from(0)])]]
    } else {
        // Basic model requirement
        vec![vec![serde_json::Value::from(0), serde_json::Value::from("any"), serde_json::Value::Array(vec![serde_json::Value::from(0)])]]
    };

    AnkiModel {
        version: vec![],
        name: template_name.to_string(),
        model_type, // 使用传入的model_type
        modified: Utc::now().timestamp(),
        update_sequence_number: -1,
        sort_field: 0,
        deck_id: 1,
        templates: vec![AnkiTemplate {
            name: "Card 1".to_string(),
            ord: 0,
            qfmt: front_template.to_string(),
            afmt: back_template.to_string(),
            browser_qfmt: "".to_string(),
            browser_afmt: "".to_string(),
            deck_id: None,
            browser_font: "Arial".to_string(),
            browser_size: 12,
        }],
        fields: anki_fields,
        css: css_style.to_string(),
        latex_pre: "\\documentclass[12pt]{article}\n\\special{papersize=3in,5in}\n\\usepackage[utf8]{inputenc}\n\\usepackage{amssymb,amsmath}\n\\pagestyle{empty}\n\\setlength{\\parindent}{0in}\n\\begin{document}\n".to_string(),
        latex_post: "\\end{document}".to_string(),
        tags: vec![],
        id: template_id.unwrap_or("1425279151691").to_string(),
        req,
    }
}

/// 创建Cloze模型定义
fn create_cloze_model() -> AnkiModel {
    AnkiModel {
        version: vec![],
        name: "Cloze".to_string(),
        model_type: 1, // Cloze类型
        modified: Utc::now().timestamp(),
        update_sequence_number: -1,
        sort_field: 0,
        deck_id: 1,
        templates: vec![AnkiTemplate {
            name: "Cloze".to_string(),
            ord: 0,
            qfmt: "{{cloze:Text}}".to_string(),
            afmt: "{{cloze:Text}}<br>{{Extra}}".to_string(),
            browser_qfmt: "".to_string(),
            browser_afmt: "".to_string(),
            deck_id: None,
            browser_font: "Arial".to_string(),
            browser_size: 12,
        }],
        fields: vec![
            AnkiField {
                name: "Text".to_string(),
                ord: 0,
                sticky: false,
                rtl: false,
                font: "Arial".to_string(),
                size: 20,
                media: vec![],
                description: "".to_string(),
            },
            AnkiField {
                name: "Extra".to_string(),
                ord: 1,
                sticky: false,
                rtl: false,
                font: "Arial".to_string(),
                size: 20,
                media: vec![],
                description: "".to_string(),
            },
        ],
        css: ".card {\n font-family: arial;\n font-size: 20px;\n text-align: center;\n color: black;\n background-color: white;\n}\n.cloze {\n font-weight: bold;\n color: blue;\n}".to_string(),
        latex_pre: "\\documentclass[12pt]{article}\n\\special{papersize=3in,5in}\n\\usepackage[utf8]{inputenc}\n\\usepackage{amssymb,amsmath}\n\\pagestyle{empty}\n\\setlength{\\parindent}{0in}\n\\begin{document}\n".to_string(),
        latex_post: "\\end{document}".to_string(),
        tags: vec![],
        id: "1425279151692".to_string(),
        req: vec![vec![serde_json::Value::from(0), serde_json::Value::from("all"), serde_json::Value::Array(vec![serde_json::Value::from(0)])]],
    }
}

/// 初始化Anki数据库结构
fn initialize_anki_database(conn: &Connection, deck_name: &str, model_name: &str) -> SqliteResult<(i64, i64)> {
    initialize_anki_database_with_template(conn, deck_name, model_name, None)
}

fn initialize_anki_database_with_template(
    conn: &Connection, 
    deck_name: &str, 
    model_name: &str,
    template_config: Option<(String, Vec<String>, String, String, String)>
) -> SqliteResult<(i64, i64)> {
    // 创建基本表结构
    conn.execute_batch(r#"
        PRAGMA journal_mode = WAL;
        
        CREATE TABLE col (
            id              integer primary key,
            crt             integer not null,
            mod             integer not null,
            scm             integer not null,
            ver             integer not null,
            dty             integer not null,
            usn             integer not null,
            ls              integer not null,
            conf            text not null,
            models          text not null,
            decks           text not null,
            dconf           text not null,
            tags            text not null
        );
        
        CREATE TABLE notes (
            id              integer primary key,
            guid            text not null unique,
            mid             integer not null,
            mod             integer not null,
            usn             integer not null,
            tags            text not null,
            flds            text not null,
            sfld            text not null,
            csum            integer not null,
            flags           integer not null,
            data            text not null
        );
        
        CREATE TABLE cards (
            id              integer primary key,
            nid             integer not null,
            did             integer not null,
            ord             integer not null,
            mod             integer not null,
            usn             integer not null,
            type            integer not null,
            queue           integer not null,
            due             integer not null,
            ivl             integer not null,
            factor          integer not null,
            reps            integer not null,
            lapses          integer not null,
            left            integer not null,
            odue            integer not null,
            odid            integer not null,
            flags           integer not null,
            data            text not null
        );
        
        CREATE TABLE revlog (
            id              integer primary key,
            cid             integer not null,
            usn             integer not null,
            ease            integer not null,
            ivl             integer not null,
            lastIvl         integer not null,
            factor          integer not null,
            time            integer not null,
            type            integer not null
        );
        
        CREATE TABLE graves (
            usn             integer not null,
            oid             integer not null,
            type            integer not null
        );
        
        CREATE INDEX ix_cards_nid on cards (nid);
        CREATE INDEX ix_cards_sched on cards (did, queue, due);
        CREATE INDEX ix_cards_usn on cards (usn);
        CREATE INDEX ix_notes_usn on notes (usn);
        CREATE INDEX ix_notes_csum on notes (csum);
        CREATE INDEX ix_revlog_usn on revlog (usn);
        CREATE INDEX ix_revlog_cid on revlog (cid);
    "#)?;

    let now = Utc::now().timestamp();
    let deck_id = 1i64;
    let model_id = if model_name == "Cloze" { 1425279151692i64 } else { 1425279151691i64 };

    // 创建牌组配置
    let decks = serde_json::json!({
        "1": {
            "id": 1,
            "name": deck_name,
            "extendRev": 50,
            "usn": 0,
            "collapsed": false,
            "newToday": [0, 0],
            "revToday": [0, 0],
            "lrnToday": [0, 0],
            "timeToday": [0, 0],
            "dyn": 0,
            "extendNew": 10,
            "conf": 1,
            "desc": "",
            "browserCollapsed": true,
            "mod": now
        }
    });

    // 创建模型配置
    let model = if let Some((template_name, fields, front_template, back_template, css_style)) = template_config {
        // 判断是否为Cloze类型
        let model_type = if model_name == "Cloze" { 1 } else { 0 };
        
        // 🎯 关键修复：清理模板HTML中的占位符
        let cleaned_front_template = clean_template_placeholders(&front_template);
        let cleaned_back_template = clean_template_placeholders(&back_template);
        let cleaned_css_style = clean_template_placeholders(&css_style);
        
        create_template_model(
            Some(&model_id.to_string()),
            &template_name,
            &fields,
            &cleaned_front_template,
            &cleaned_back_template,
            &cleaned_css_style,
            model_type,
        )
    } else if model_name == "Cloze" {
        create_cloze_model()
    } else {
        create_basic_model()
    };
    
    let model_id_clone = model.id.clone();
    let models = serde_json::json!({
        model_id_clone: model
    });

    // 创建牌组配置
    let dconf = serde_json::json!({
        "1": {
            "id": 1,
            "name": "Default",
            "replayq": true,
            "lapse": {
                "leechFails": 8,
                "minInt": 1,
                "leechAction": 0,
                "delays": [10],
                "mult": 0.0
            },
            "rev": {
                "perDay": 200,
                "ivlFct": 1.0,
                "maxIvl": 36500,
                "ease4": 1.3,
                "bury": true,
                "minSpace": 1
            },
            "timer": 0,
            "maxTaken": 60,
            "usn": 0,
            "new": {
                "perDay": 20,
                "delays": [1, 10],
                "separate": true,
                "ints": [1, 4, 7],
                "initialFactor": 2500,
                "bury": true,
                "order": 1
            },
            "mod": now,
            "autoplay": true
        }
    });

    // 插入集合配置
    conn.execute(
        "INSERT INTO col (id, crt, mod, scm, ver, dty, usn, ls, conf, models, decks, dconf, tags) VALUES (1, ?, ?, ?, 11, 0, 0, 0, ?, ?, ?, ?, '{}')",
        params![
            now,
            now,
            now,
            ANKI_COLLECTION_CONFIG,
            models.to_string(),
            decks.to_string(),
            dconf.to_string()
        ]
    )?;

    Ok((deck_id, model_id))
}

/// 生成字段校验和
fn field_checksum(text: &str) -> i64 {
    if text.is_empty() {
        return 0;
    }
    
    let mut sum = 0i64;
    for (i, ch) in text.chars().enumerate() {
        sum += (ch as u32 as i64) * (i as i64 + 1);
    }
    sum
}

/// 将AnkiCard转换为Anki数据库记录
fn convert_cards_to_anki_records(
    cards: Vec<AnkiCard>,
    _deck_id: i64,
    _model_id: i64,
    model_name: &str,
) -> Result<Vec<(String, String, String, i64, String)>, String> {
    convert_cards_to_anki_records_with_fields(cards, _deck_id, _model_id, model_name, None)
}

fn convert_cards_to_anki_records_with_fields(
    cards: Vec<AnkiCard>,
    _deck_id: i64,
    _model_id: i64,
    model_name: &str,
    template_fields: Option<&[String]>,
) -> Result<Vec<(String, String, String, i64, String)>, String> {
    let mut records = Vec::new();
    let now = Utc::now().timestamp();

    for card in cards {
        let note_id = now * 1000 + records.len() as i64; // 生成唯一ID
        let guid = format!("{}", uuid::Uuid::new_v4().to_string().replace("-", ""));
        
        // 根据模板字段或模型类型处理字段
        let (fields, sort_field) = if let Some(field_names) = template_fields {
            // 使用模板字段
            let mut field_values = Vec::new();
            
            // 🐛 调试日志：打印字段处理信息
            if field_names.len() > 4 { // 学术模板有6个字段
                println!("🎯 DEBUG: 处理学术模板，字段数量: {}", field_names.len());
                println!("🎯 DEBUG: 模板字段: {:?}", field_names);
                println!("🎯 DEBUG: 卡片extra_fields: {:?}", card.extra_fields.keys().collect::<Vec<_>>());
                println!("🎯 DEBUG: 卡片tags字段: {:?}", card.tags);
            }
            
            for field_name in field_names {
                let value = match field_name.to_lowercase().as_str() {
                    "front" => clean_template_placeholders(&card.front),
                    "back" => clean_template_placeholders(&card.back),
                    "text" => clean_template_placeholders(&card.text.clone().unwrap_or_default()),
                    "tags" => {
                        // 处理标签字段：将Vec<String>转换为逗号分隔的字符串
                        if card.tags.is_empty() {
                            String::new()
                        } else {
                            clean_template_placeholders(&card.tags.join(", "))
                        }
                    }
                    _ => {
                        // 从扩展字段中获取 (大小写不敏感)
                        let field_key = field_name.to_lowercase();
                        let raw_value = card.extra_fields.get(&field_key)
                            .or_else(|| card.extra_fields.get(field_name))
                            .cloned()
                            .unwrap_or_else(|| {
                                // 🐛 调试：记录缺失的字段
                                println!("⚠️ DEBUG: 字段 '{}' 未找到，使用空值", field_name);
                                String::new()
                            });
                        clean_template_placeholders(&raw_value)
                    }
                };
                
                // 🐛 调试：打印每个字段的值 (UTF-8安全截断)
                if field_names.len() > 4 {
                    println!("🎯 DEBUG: 字段 '{}' -> '{}'", field_name, 
                        if value.chars().count() > 50 { 
                            format!("{}...", value.chars().take(50).collect::<String>())
                        } else { 
                            value.clone() 
                        });
                }
                
                field_values.push(value);
            }
            let fields_str = field_values.join("\x1f");
            let sort_field = field_values.first().cloned().unwrap_or_default();
            (fields_str, sort_field)
        } else {
            // 传统处理方式
            match model_name {
                "Cloze" => {
                    let clean_front = clean_template_placeholders(&card.front);
                    let clean_back = clean_template_placeholders(&card.back);
                    let cloze_text = if clean_back.is_empty() {
                        clean_front
                    } else {
                        format!("{{{{c1::{}}}}}\n\n{}", clean_front, clean_back)
                    };
                    let fields = format!("{}\x1f\x1f", cloze_text);
                    (fields, cloze_text)
                }
                _ => {
                    let clean_front = clean_template_placeholders(&card.front);
                    let clean_back = clean_template_placeholders(&card.back);
                    let fields = format!("{}\x1f{}", clean_front, clean_back);
                    (fields, clean_front)
                }
            }
        };

        // 清理tags中的模板占位符
        let cleaned_tags: Vec<String> = card.tags.iter()
            .map(|tag| clean_template_placeholders(tag))
            .filter(|tag| !tag.is_empty()) // 过滤掉空标签
            .collect();
        let tags = cleaned_tags.join(" ");
        let csum = field_checksum(&sort_field);

        records.push((
            note_id.to_string(),
            guid,
            fields,
            csum,
            tags,
        ));
    }

    Ok(records)
}

/// 导出卡片为.apkg文件
pub async fn export_cards_to_apkg(
    cards: Vec<AnkiCard>,
    deck_name: String,
    note_type: String,
    output_path: PathBuf,
) -> Result<(), String> {
    export_cards_to_apkg_with_template(cards, deck_name, note_type, output_path, None).await
}

/// 导出卡片为.apkg文件（支持模板）
pub async fn export_cards_to_apkg_with_template(
    cards: Vec<AnkiCard>,
    deck_name: String,
    note_type: String,
    output_path: PathBuf,
    template_config: Option<(String, Vec<String>, String, String, String)>, // (name, fields, front, back, css)
) -> Result<(), String> {
    if cards.is_empty() {
        return Err("没有卡片可以导出".to_string());
    }

    // 创建临时目录
    let temp_dir = std::env::temp_dir().join(format!("anki_export_{}", Utc::now().timestamp()));
    fs::create_dir_all(&temp_dir).map_err(|e| format!("创建临时目录失败: {}", e))?;

    let db_path = temp_dir.join("collection.anki2");
    
    // 确保输出目录存在
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建输出目录失败: {}", e))?;
    }

    let result = async move {
        // 创建并初始化数据库
        let conn = Connection::open(&db_path)
            .map_err(|e| format!("创建数据库失败: {}", e))?;
        
        let (deck_id, model_id) = initialize_anki_database_with_template(&conn, &deck_name, &note_type, template_config.clone())
            .map_err(|e| format!("初始化数据库失败: {}", e))?;

        // 转换卡片数据
        let template_fields_ref = template_config.as_ref().map(|(_, fields, _, _, _)| fields.as_slice());
        let records = convert_cards_to_anki_records_with_fields(cards, deck_id, model_id, &note_type, template_fields_ref)?;
        
        let now = Utc::now().timestamp();

        // 插入笔记和卡片
        for (i, (note_id, guid, fields, csum, tags)) in records.iter().enumerate() {
            // 插入笔记
            conn.execute(
                "INSERT INTO notes (id, guid, mid, mod, usn, tags, flds, sfld, csum, flags, data) VALUES (?, ?, ?, ?, -1, ?, ?, ?, ?, 0, '')",
                params![
                    note_id.parse::<i64>().unwrap(),
                    guid,
                    model_id,
                    now,
                    tags,
                    fields,
                    "", // sfld 会在后面更新
                    csum
                ]
            ).map_err(|e| format!("插入笔记失败: {}", e))?;

            // 为每个笔记创建卡片（Basic类型通常只有一张卡片）
            let card_id = note_id.parse::<i64>().unwrap() * 100 + i as i64;
            conn.execute(
                "INSERT INTO cards (id, nid, did, ord, mod, usn, type, queue, due, ivl, factor, reps, lapses, left, odue, odid, flags, data) VALUES (?, ?, ?, 0, ?, -1, 0, 0, ?, 0, 2500, 0, 0, 0, 0, 0, 0, '')",
                params![
                    card_id,
                    note_id.parse::<i64>().unwrap(),
                    deck_id,
                    now,
                    i as i64 + 1 // due date
                ]
            ).map_err(|e| format!("插入卡片失败: {}", e))?;
        }

        conn.close().map_err(|e| format!("关闭数据库失败: {:?}", e))?;

        // 创建.apkg文件（实际上是一个zip文件）
        let output_file = File::create(&output_path)
            .map_err(|e| format!("创建输出文件失败: {}", e))?;
        
        let mut zip = ZipWriter::new(output_file);
        
        // 添加数据库文件到zip
        let db_content = fs::read(&db_path)
            .map_err(|e| format!("读取数据库文件失败: {}", e))?;
        
        zip.start_file("collection.anki2", FileOptions::default())
            .map_err(|e| format!("创建zip文件条目失败: {}", e))?;
        zip.write_all(&db_content)
            .map_err(|e| format!("写入数据库到zip失败: {}", e))?;

        // 创建媒体文件列表（空的）
        zip.start_file("media", FileOptions::default())
            .map_err(|e| format!("创建媒体文件条目失败: {}", e))?;
        zip.write_all(b"{}")
            .map_err(|e| format!("写入媒体文件列表失败: {}", e))?;

        zip.finish()
            .map_err(|e| format!("完成zip文件失败: {}", e))?;

        Ok(())
    }.await;

    // 清理临时文件
    if temp_dir.exists() {
        if let Err(e) = fs::remove_dir_all(&temp_dir) {
            println!("警告：清理临时目录失败: {}", e);
        }
    }

    result
}
