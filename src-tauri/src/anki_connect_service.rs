use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::models::AnkiCard;
use std::net::TcpStream;
use std::time::Duration;

const ANKI_CONNECT_URL: &str = "http://127.0.0.1:8765";

#[derive(Serialize)]
struct AnkiConnectRequest {
    action: String,
    version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct AnkiConnectResponse {
    result: Option<serde_json::Value>,
    error: Option<String>,
}

#[derive(Serialize)]
struct Note {
    #[serde(rename = "deckName")]
    deck_name: String,
    #[serde(rename = "modelName")]
    model_name: String,
    fields: HashMap<String, String>,
    tags: Vec<String>,
}

/// 检查AnkiConnect是否可用
pub async fn check_anki_connect_availability() -> Result<bool, String> {
    println!("🔍 正在检查AnkiConnect连接到: {}", ANKI_CONNECT_URL);
    
    // 首先检查端口8765是否开放
    println!("🔍 第0步：检查端口8765是否开放...");
    match TcpStream::connect_timeout(&"127.0.0.1:8765".parse().unwrap(), Duration::from_secs(5)) {
        Ok(_) => {
            println!("✅ 端口8765可访问");
        }
        Err(e) => {
            println!("❌ 端口8765无法访问: {}", e);
            return Err(format!("端口8765无法访问: {} \n\n这通常意味着：\n1. Anki桌面程序未运行\n2. AnkiConnect插件未安装或未启用\n3. 端口被其他程序占用\n\n解决方法：\n1. 启动Anki桌面程序\n2. 安装AnkiConnect插件（代码：2055492159）\n3. 重启Anki以激活插件", e));
        }
    }
    
    // 首先尝试简单的GET请求检查服务是否运行
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .tcp_keepalive(Some(std::time::Duration::from_secs(30)))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("创建HTTP客户端失败: {}", e))?;
    
    println!("🔍 第一步：检查AnkiConnect服务是否响应...");
    match client.get(ANKI_CONNECT_URL).send().await {
        Ok(response) => {
            println!("✅ AnkiConnect服务响应状态: {}", response.status());
            let text = response.text().await.unwrap_or_else(|_| "无法读取响应".to_string());
            println!("📥 服务响应内容: {}", text);
            
            // 检查响应内容是否包含AnkiConnect信息
            if text.contains("AnkiConnect") || text.contains("apiVersion") {
                println!("✅ AnkiConnect服务确认运行正常");
            } else {
                println!("⚠️ 服务响应异常，内容: {}", text);
            }
        }
        Err(e) => {
            println!("❌ AnkiConnect服务无响应: {}", e);
            return Err(format!("AnkiConnect服务未运行或无法访问: {} \n\n请确保：\n1. Anki桌面程序正在运行\n2. AnkiConnect插件已安装（代码：2055492159）\n3. 重启Anki以激活插件\n4. 检查端口8765是否被占用", e));
        }
    }
    
    // 如果基础连接成功，再尝试API请求
    println!("🔍 第二步：测试AnkiConnect API...");
    let request = AnkiConnectRequest {
        action: "version".to_string(),
        version: 6,
        params: None,
    };

    println!("📤 发送API请求: {}", serde_json::to_string(&request).unwrap_or_else(|_| "序列化失败".to_string()));
    
    match client
        .post(ANKI_CONNECT_URL)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("User-Agent", "ai-mistake-manager/1.0")
        .json(&request)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
    {
        Ok(response) => {
            let status_code = response.status();
            println!("📥 收到响应状态: {}", status_code);
            if status_code.is_success() {
                let response_text = response.text().await
                    .map_err(|e| format!("读取响应内容失败: {}", e))?;
                println!("📥 响应内容: {}", response_text);
                
                match serde_json::from_str::<AnkiConnectResponse>(&response_text) {
                    Ok(anki_response) => {
                        if anki_response.error.is_none() {
                            println!("✅ AnkiConnect版本检查成功");
                            Ok(true)
                        } else {
                            Err(format!("AnkiConnect错误: {}", anki_response.error.unwrap_or_default()))
                        }
                    }
                    Err(e) => Err(format!("解析AnkiConnect响应失败: {} - 响应内容: {}", e, response_text)),
                }
            } else {
                let error_text = response.text().await.unwrap_or_else(|_| "无法读取错误内容".to_string());
                Err(format!("AnkiConnect HTTP错误: {} - 内容: {}", status_code, error_text))
            }
        }
        Err(e) => {
            println!("❌ AnkiConnect连接错误详情: {:?}", e);
            if e.is_timeout() {
                Err("AnkiConnect连接超时（5秒），请确保Anki桌面程序正在运行并启用了AnkiConnect插件".to_string())
            } else if e.is_connect() {
                Err("无法连接到AnkiConnect服务器，请确保：1)Anki正在运行 2)AnkiConnect插件已安装并启用 3)端口8765未被占用".to_string())
            } else if e.to_string().contains("connection closed") {
                Err("连接被AnkiConnect服务器关闭，可能原因：1)AnkiConnect版本过旧 2)请求格式不兼容 3)需要重启Anki".to_string())
            } else {
                Err(format!("AnkiConnect连接失败: {}", e))
            }
        }
    }
}

/// 获取所有牌组名称
pub async fn get_deck_names() -> Result<Vec<String>, String> {
    let request = AnkiConnectRequest {
        action: "deckNames".to_string(),
        version: 6,
        params: None,
    };

    let client = reqwest::Client::new();
    
    match client
        .post(ANKI_CONNECT_URL)
        .json(&request)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<AnkiConnectResponse>().await {
                    Ok(anki_response) => {
                        if let Some(error) = anki_response.error {
                            Err(format!("AnkiConnect错误: {}", error))
                        } else if let Some(result) = anki_response.result {
                            match serde_json::from_value::<Vec<String>>(result) {
                                Ok(deck_names) => Ok(deck_names),
                                Err(e) => Err(format!("解析牌组列表失败: {}", e)),
                            }
                        } else {
                            Err("AnkiConnect返回空结果".to_string())
                        }
                    }
                    Err(e) => Err(format!("解析AnkiConnect响应失败: {}", e)),
                }
            } else {
                Err(format!("AnkiConnect HTTP错误: {}", response.status()))
            }
        }
        Err(e) => Err(format!("请求牌组列表失败: {}", e)),
    }
}

/// 获取所有笔记类型名称
pub async fn get_model_names() -> Result<Vec<String>, String> {
    let request = AnkiConnectRequest {
        action: "modelNames".to_string(),
        version: 6,
        params: None,
    };

    let client = reqwest::Client::new();
    
    match client
        .post(ANKI_CONNECT_URL)
        .json(&request)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<AnkiConnectResponse>().await {
                    Ok(anki_response) => {
                        if let Some(error) = anki_response.error {
                            Err(format!("AnkiConnect错误: {}", error))
                        } else if let Some(result) = anki_response.result {
                            match serde_json::from_value::<Vec<String>>(result) {
                                Ok(model_names) => Ok(model_names),
                                Err(e) => Err(format!("解析笔记类型列表失败: {}", e)),
                            }
                        } else {
                            Err("AnkiConnect返回空结果".to_string())
                        }
                    }
                    Err(e) => Err(format!("解析AnkiConnect响应失败: {}", e)),
                }
            } else {
                Err(format!("AnkiConnect HTTP错误: {}", response.status()))
            }
        }
        Err(e) => Err(format!("请求笔记类型列表失败: {}", e)),
    }
}

/// 将AnkiCard列表添加到Anki
pub async fn add_notes_to_anki(
    cards: Vec<AnkiCard>,
    deck_name: String,
    note_type: String,
) -> Result<Vec<Option<u64>>, String> {
    // 首先检查AnkiConnect可用性
    check_anki_connect_availability().await?;

    // 构建notes数组
    let notes: Vec<Note> = cards
        .into_iter()
        .map(|card| {
            let mut fields = HashMap::new();
            
            // 根据笔记类型决定字段映射
            match note_type.as_str() {
                "Basic" => {
                    fields.insert("Front".to_string(), card.front);
                    fields.insert("Back".to_string(), card.back);
                }
                "Basic (and reversed card)" => {
                    fields.insert("Front".to_string(), card.front);
                    fields.insert("Back".to_string(), card.back);
                }
                "Basic (optional reversed card)" => {
                    fields.insert("Front".to_string(), card.front);
                    fields.insert("Back".to_string(), card.back);
                }
                "Cloze" => {
                    // 对于Cloze类型，需要将front和back合并
                    let cloze_text = if card.back.is_empty() {
                        card.front
                    } else {
                        format!("{}\n\n{}", card.front, card.back)
                    };
                    fields.insert("Text".to_string(), cloze_text);
                }
                _ => {
                    // 对于其他类型，尝试使用Front/Back字段，如果失败则使用第一个和第二个字段
                    fields.insert("Front".to_string(), card.front);
                    fields.insert("Back".to_string(), card.back);
                }
            }

            Note {
                deck_name: deck_name.clone(),
                model_name: note_type.clone(),
                fields,
                tags: card.tags,
            }
        })
        .collect();

    let params = serde_json::json!({
        "notes": notes
    });

    let request = AnkiConnectRequest {
        action: "addNotes".to_string(),
        version: 6,
        params: Some(params),
    };

    let client = reqwest::Client::new();
    
    match client
        .post(ANKI_CONNECT_URL)
        .json(&request)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<AnkiConnectResponse>().await {
                    Ok(anki_response) => {
                        if let Some(error) = anki_response.error {
                            Err(format!("AnkiConnect错误: {}", error))
                        } else if let Some(result) = anki_response.result {
                            match serde_json::from_value::<Vec<Option<u64>>>(result) {
                                Ok(note_ids) => Ok(note_ids),
                                Err(e) => Err(format!("解析笔记ID列表失败: {}", e)),
                            }
                        } else {
                            Err("AnkiConnect返回空结果".to_string())
                        }
                    }
                    Err(e) => Err(format!("解析AnkiConnect响应失败: {}", e)),
                }
            } else {
                Err(format!("AnkiConnect HTTP错误: {}", response.status()))
            }
        }
        Err(e) => Err(format!("添加笔记到Anki失败: {}", e)),
    }
}

/// 创建牌组（如果不存在）
pub async fn create_deck_if_not_exists(deck_name: &str) -> Result<(), String> {
    let params = serde_json::json!({
        "deck": deck_name
    });

    let request = AnkiConnectRequest {
        action: "createDeck".to_string(),
        version: 6,
        params: Some(params),
    };

    let client = reqwest::Client::new();
    
    match client
        .post(ANKI_CONNECT_URL)
        .json(&request)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<AnkiConnectResponse>().await {
                    Ok(anki_response) => {
                        if let Some(error) = anki_response.error {
                            // 如果牌组已存在，这不算错误
                            if error.contains("already exists") {
                                Ok(())
                            } else {
                                Err(format!("创建牌组时出错: {}", error))
                            }
                        } else {
                            Ok(())
                        }
                    }
                    Err(e) => Err(format!("解析AnkiConnect响应失败: {}", e)),
                }
            } else {
                Err(format!("AnkiConnect HTTP错误: {}", response.status()))
            }
        }
        Err(e) => Err(format!("创建牌组失败: {}", e)),
    }
}