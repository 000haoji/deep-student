use crate::models::{ChatMessage, AppError, StandardModel1Output, StandardModel2Output, ModelAssignments, StreamChunk};
use crate::database::Database;
use crate::file_manager::FileManager;
use crate::crypto::{CryptoService, EncryptedData};
use crate::gemini_adapter;
use reqwest::{Client, ClientBuilder, header::HeaderMap};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use url::Url;
use serde::{Deserialize, Serialize};
use futures_util::StreamExt;
use tauri::{Window, Emitter};
use base64::{engine::general_purpose, Engine as _};

type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiConfig {
    pub id: String,
    pub name: String,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub is_multimodal: bool,
    pub is_reasoning: bool,
    pub enabled: bool,
    #[serde(default = "default_model_adapter")]
    pub model_adapter: String, // 新增：模型适配器类型
    #[serde(default = "default_max_output_tokens")]
    pub max_output_tokens: u32, // 新增：最大输出Token数
    #[serde(default = "default_temperature")]
    pub temperature: f32, // 新增：温度参数
}

// 默认值函数
fn default_model_adapter() -> String {
    "general".to_string()
}

fn default_max_output_tokens() -> u32 {
    8192
}

fn default_temperature() -> f32 {
    0.7
}

pub struct LLMManager {
    client: Client,
    db: Arc<Database>,
    file_manager: Arc<FileManager>,
    crypto_service: CryptoService,
}

impl LLMManager {
    pub fn new(db: Arc<Database>, file_manager: Arc<FileManager>) -> Self {
        // 创建HTTP客户端，使用渐进式回退策略确保始终有合理的配置
        let client = Self::create_http_client_with_fallback();
        
        let app_data_dir_path = file_manager.get_app_data_dir(); // Assuming this returns &Path
        let crypto_service = CryptoService::new(&app_data_dir_path.to_path_buf())
            .expect("无法初始化加密服务");
        
        Self {
            client,
            db,
            file_manager,
            crypto_service,
        }
    }

    /// 创建HTTP客户端，使用渐进式回退策略确保始终有合理的配置
    fn create_http_client_with_fallback() -> Client {
        // 创建默认请求头，显式禁用压缩，防止后端收到 gzip/deflate 数据导致乱码
        let mut headers = HeaderMap::new();
        headers.insert("Accept-Encoding", "identity".parse().unwrap());
        
        // 尝试1: 完整配置的客户端（推荐配置）
        if let Ok(client) = ClientBuilder::new()
            .timeout(std::time::Duration::from_secs(300)) // 全局超时300秒（流式请求需要更长时间）
            .connect_timeout(std::time::Duration::from_secs(30)) // 连接超时30秒
            .danger_accept_invalid_certs(false) // 保持SSL验证
            .use_rustls_tls() // 使用rustls而不是系统TLS
            .default_headers(headers.clone())
            .build() 
        {
            println!("HTTP客户端创建成功: 完整配置（超时120s，连接15s，rustls TLS）");
            return client;
        }

        // 尝试2: 简化TLS配置的客户端
        if let Ok(client) = ClientBuilder::new()
            .timeout(std::time::Duration::from_secs(300))
            .connect_timeout(std::time::Duration::from_secs(30))
            .danger_accept_invalid_certs(false)
            .default_headers(headers.clone())
            .build() 
        {
            println!("HTTP客户端创建成功: 简化TLS配置（超时120s，连接15s，系统TLS）");
            return client;
        }

        // 尝试3: 仅超时配置的客户端
        if let Ok(client) = ClientBuilder::new()
            .timeout(std::time::Duration::from_secs(300))
            .default_headers(headers.clone())
            .build() 
        {
            println!("HTTP客户端创建成功: 仅超时配置（超时120s）");
            return client;
        }

        // 尝试4: 最小配置的客户端（保证基本超时）
        if let Ok(client) = ClientBuilder::new()
            .timeout(std::time::Duration::from_secs(180)) // 最少180秒超时
            .default_headers(headers.clone())
            .build() 
        {
            println!("HTTP客户端创建成功: 最小配置（超时60s）");
            return client;
        }

        // 最后回退: 默认客户端
        println!("警告: 所有配置均失败，使用默认HTTP客户端（无超时配置）");
        println!("这可能导致网络请求挂起，建议检查系统网络和TLS配置");
        Client::new()
    }

    /// 检测Base64编码图像的真实格式
    fn detect_image_format_from_base64(base64_data: &str) -> &'static str {
        // 解码Base64获取前几个字节来判断格式
        if let Ok(decoded) = general_purpose::STANDARD.decode(base64_data.get(..100).unwrap_or(base64_data)) {
            Self::detect_image_format_from_bytes(&decoded)
        } else {
            "jpeg" // 默认格式
        }
    }

    /// 根据图像字节数据检测格式
    fn detect_image_format_from_bytes(image_data: &[u8]) -> &'static str {
        if image_data.len() < 4 {
            return "jpeg"; // 默认格式
        }

        // JPEG: FF D8 FF
        if image_data.starts_with(&[0xFF, 0xD8, 0xFF]) {
            "jpeg"
        }
        // PNG: 89 50 4E 47 0D 0A 1A 0A
        else if image_data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
            "png"
        }
        // GIF: 47 49 46 38 (GIF8)
        else if image_data.starts_with(&[0x47, 0x49, 0x46, 0x38]) {
            "gif"
        }
        // WebP: 52 49 46 46 ... 57 45 42 50 (RIFF...WEBP)
        else if image_data.len() >= 12 && 
                image_data.starts_with(&[0x52, 0x49, 0x46, 0x46]) &&
                &image_data[8..12] == &[0x57, 0x45, 0x42, 0x50] {
            "webp"
        }
        // BMP: 42 4D (BM)
        else if image_data.starts_with(&[0x42, 0x4D]) {
            "bmp"
        }
        else {
            "jpeg" // 默认格式
        }
    }

    // 获取API配置（公开方法）
    pub async fn get_api_configs(&self) -> Result<Vec<ApiConfig>> {
        let config_str = self.db.get_setting("api_configs")
            .map_err(|e| AppError::database(format!("获取API配置失败: {}", e)))?
            .unwrap_or_else(|| "[]".to_string());
        
        // 尝试反序列化为新格式
        match serde_json::from_str::<Vec<ApiConfig>>(&config_str) {
            Ok(mut configs) => {
                // 解密API密钥
                for config in &mut configs {
                    config.api_key = self.decrypt_api_key_if_needed(&config.api_key)?;
                }
                Ok(configs)
            },
            Err(_) => {
                // 如果失败，尝试作为旧格式反序列化并迁移
                println!("检测到旧版API配置格式，正在迁移...");
                self.migrate_api_configs(&config_str).await
            }
        }
    }

    // 迁移旧版API配置到新格式
    async fn migrate_api_configs(&self, old_config_str: &str) -> Result<Vec<ApiConfig>> {
        // 尝试多种旧格式的反序列化
        
        // 最新的旧格式（包含 is_reasoning 但没有 model_adapter）
        #[derive(serde::Deserialize)]
        struct OldApiConfigV2 {
            id: String,
            name: String,
            api_key: String,
            base_url: String,
            model: String,
            is_multimodal: bool,
            is_reasoning: bool,
            enabled: bool,
        }

        // 更旧的格式（没有 is_reasoning）
        #[derive(serde::Deserialize)]
        struct OldApiConfigV1 {
            id: String,
            name: String,
            api_key: String,
            base_url: String,
            model: String,
            is_multimodal: bool,
            enabled: bool,
        }

        // 首先尝试解析为 V2 格式
        if let Ok(old_configs) = serde_json::from_str::<Vec<OldApiConfigV2>>(old_config_str) {
            let new_configs: Vec<ApiConfig> = old_configs.into_iter().map(|old| {
                // 根据模型名称智能推断适配器类型
                let model_adapter = if old.model.to_lowercase().contains("deepseek") && 
                                      old.model.to_lowercase().contains("r1") {
                    "deepseek-r1".to_string()
                } else {
                    "general".to_string()
                };

                ApiConfig {
                    id: old.id,
                    name: old.name,
                    api_key: old.api_key,
                    base_url: old.base_url,
                    model: old.model,
                    is_multimodal: old.is_multimodal,
                    is_reasoning: old.is_reasoning,
                    enabled: old.enabled,
                    model_adapter,
                    max_output_tokens: default_max_output_tokens(),
                    temperature: default_temperature(),
                }
            }).collect();

            self.save_api_configurations(&new_configs).await?;
            println!("API配置迁移完成（V2->V3），添加了 {} 个配置的 model_adapter 字段", new_configs.len());
            return Ok(new_configs);
        }

        // 如果 V2 失败，尝试解析为 V1 格式
        let old_configs: Vec<OldApiConfigV1> = serde_json::from_str(old_config_str)
            .map_err(|e| AppError::configuration(format!("解析旧版API配置失败: {}", e)))?;

        let new_configs: Vec<ApiConfig> = old_configs.into_iter().map(|old| {
            // 根据模型名称智能推断适配器类型和推理能力
            let (is_reasoning, model_adapter) = if old.model.to_lowercase().contains("deepseek") && 
                                                  old.model.to_lowercase().contains("r1") {
                (true, "deepseek-r1".to_string())
            } else if old.model.to_lowercase().contains("o1") {
                (true, "general".to_string())
            } else {
                (false, "general".to_string())
            };

            ApiConfig {
                id: old.id,
                name: old.name,
                api_key: old.api_key,
                base_url: old.base_url,
                model: old.model,
                is_multimodal: old.is_multimodal,
                is_reasoning,
                enabled: old.enabled,
                model_adapter,
                max_output_tokens: default_max_output_tokens(),
                temperature: default_temperature(),
            }
        }).collect();

        // 保存迁移后的配置
        self.save_api_configurations(&new_configs).await?;
        println!("API配置迁移完成（V1->V3），添加了 {} 个配置的 is_reasoning 和 model_adapter 字段", new_configs.len());

        Ok(new_configs)
    }

    // 获取第一模型配置
    async fn get_model1_config(&self) -> Result<ApiConfig> {
        let assignments = self.get_model_assignments().await?;
        let model1_id = assignments.model1_config_id
            .ok_or_else(|| AppError::configuration("第一模型未配置"))?;
        
        println!("查找第一模型配置，ID: {}", model1_id);
        
        let configs = self.get_api_configs().await?;
        println!("可用的API配置数量: {}", configs.len());
        for (i, config) in configs.iter().enumerate() {
            println!("配置 {}: ID={}, 模型={}, 多模态={}, 启用={}", 
                    i, config.id, config.model, config.is_multimodal, config.enabled);
        }
        
        let config = configs.into_iter()
            .find(|c| c.id == model1_id && c.is_multimodal && c.enabled)
            .ok_or_else(|| AppError::configuration("找不到有效的第一模型配置"))?;
        
        println!("找到第一模型配置: 模型={}, API地址={}", config.model, config.base_url);
        Ok(config)
    }

    // 获取第二模型配置（公开方法，供 AnalysisService 使用）
    pub async fn get_model2_config(&self) -> Result<ApiConfig> {
        let assignments = self.get_model_assignments().await?;
        let model2_id = assignments.model2_config_id
            .ok_or_else(|| AppError::configuration("第二模型未配置"))?;
        
        let configs = self.get_api_configs().await?;
        let config = configs.into_iter()
            .find(|c| c.id == model2_id && c.enabled)
            .ok_or_else(|| AppError::configuration("找不到有效的第二模型配置"))?;
        
        Ok(config)
    }

    // 获取ANKI制卡模型配置
    async fn get_anki_model_config(&self) -> Result<ApiConfig> {
        let assignments = self.get_model_assignments().await?;
        let anki_model_id = assignments.anki_card_model_config_id
            .ok_or_else(|| AppError::configuration("ANKI制卡模型未配置"))?;
        
        let configs = self.get_api_configs().await?;
        let config = configs.into_iter()
            .find(|c| c.id == anki_model_id && c.enabled)
            .ok_or_else(|| AppError::configuration("找不到有效的ANKI制卡模型配置"))?;
        
        println!("找到ANKI制卡模型配置: 模型={}, API地址={}", config.model, config.base_url);
        Ok(config)
    }

    // 获取模型分配配置
    pub async fn get_model_assignments(&self) -> Result<ModelAssignments> {
        let assignments_str = self.db.get_setting("model_assignments")
            .map_err(|e| AppError::database(format!("获取模型分配配置失败: {}", e)))?
            .unwrap_or_else(|| r#"{"model1_config_id": null, "model2_config_id": null, "review_analysis_model_config_id": null, "anki_card_model_config_id": null}"#.to_string());
        
        let assignments: ModelAssignments = serde_json::from_str(&assignments_str)
            .map_err(|e| AppError::configuration(format!("解析模型分配配置失败: {}", e)))?;
        
        Ok(assignments)
    }

    // 保存模型分配配置
    pub async fn save_model_assignments(&self, assignments: &ModelAssignments) -> Result<()> {
        let assignments_str = serde_json::to_string(assignments)
            .map_err(|e| AppError::configuration(format!("序列化模型分配配置失败: {}", e)))?;
        
        self.db.save_setting("model_assignments", &assignments_str)
            .map_err(|e| AppError::database(format!("保存模型分配配置失败: {}", e)))?;
        
        Ok(())
    }

    // 保存API配置
    pub async fn save_api_configurations(&self, configs: &[ApiConfig]) -> Result<()> {
        // 创建配置副本并加密API密钥
        let mut encrypted_configs = configs.to_vec();
        for config in &mut encrypted_configs {
            config.api_key = self.encrypt_api_key(&config.api_key)?;
        }
        
        let configs_str = serde_json::to_string(&encrypted_configs)
            .map_err(|e| AppError::configuration(format!("序列化API配置失败: {}", e)))?;
        
        self.db.save_setting("api_configs", &configs_str)
            .map_err(|e| AppError::database(format!("保存API配置失败: {}", e)))?;
        
        Ok(())
    }

    // 加密API密钥
    fn encrypt_api_key(&self, api_key: &str) -> Result<String> {
        // 如果已经是加密格式，直接返回
        if CryptoService::is_encrypted_format(api_key) {
            return Ok(api_key.to_string());
        }
        
        let encrypted_data = self.crypto_service.encrypt_api_key(api_key)
            .map_err(|e| AppError::configuration(format!("加密API密钥失败: {}", e)))?;
        
        serde_json::to_string(&encrypted_data)
            .map_err(|e| AppError::configuration(format!("序列化加密数据失败: {}", e)))
    }
    
    // 解密API密钥（如果需要）
    fn decrypt_api_key_if_needed(&self, api_key: &str) -> Result<String> {
        // 检查是否为加密格式
        if CryptoService::is_encrypted_format(api_key) {
            let encrypted_data: EncryptedData = serde_json::from_str(api_key)
                .map_err(|e| AppError::configuration(format!("解析加密数据失败: {}", e)))?;
            
            self.crypto_service.decrypt_api_key(&encrypted_data)
                .map_err(|e| AppError::configuration(format!("解密API密钥失败: {}", e)))
        } else {
            // 明文格式，迁移到加密格式
            println!("检测到明文API密钥，将在下次保存时自动加密");
            Ok(api_key.to_string())
        }
    }

    // 统一AI接口层 - 模型一（OCR + 分类）
    pub async fn call_unified_model_1(
        &self,
        image_paths: Vec<String>,
        user_question: &str,
        subject: &str,
        task_context: Option<&str>,
    ) -> Result<StandardModel1Output> {
        println!("调用统一模型一接口: 图片数量={}, 科目={}", image_paths.len(), subject);
        
        // 获取模型配置
        let config = self.get_model1_config().await?;

        // *** 新增的适配器路由逻辑 ***
        if config.model_adapter == "google" {
            // 读取图片文件并转换为Base64
            let mut images_base64 = Vec::new();
            for path in &image_paths {
                let base64_content = self.file_manager.read_file_as_base64(path)?;
                images_base64.push(base64_content);
            }

            // 构建包含图片的消息
            let mut messages = Vec::new();
            let full_prompt = if let Some(context) = task_context {
                format!("科目: {}\n任务上下文: {}\n用户问题: {}\n\n请分析图片中的题目内容，提取文字内容，确定题目类型，并生成相关标签。返回JSON格式：{{\"ocr_text\": \"题目文字\", \"tags\": [\"标签1\", \"标签2\"], \"mistake_type\": \"题目类型\"}}", subject, context, user_question)
            } else {
                format!("科目: {}\n用户问题: {}\n\n请分析图片中的题目内容，提取文字内容，确定题目类型，并生成相关标签。返回JSON格式：{{\"ocr_text\": \"题目文字\", \"tags\": [\"标签1\", \"标签2\"], \"mistake_type\": \"题目类型\"}}", subject, user_question)
            };

            let mut message = ChatMessage {
                role: "user".to_string(),
                content: full_prompt,
                timestamp: chrono::Utc::now(),
                thinking_content: None,
                rag_sources: None,
                image_paths: None,
                image_base64: Some(images_base64),
            };

            messages.push(message);

            // 调用 Gemini 适配器（非流式）
            let gemini_result = gemini_adapter::non_stream_chat(
                &self.client,
                &config,
                &messages,
            ).await?;

            // 解析 Gemini 响应为 StandardModel1Output 格式
            let content_str = &gemini_result.assistant_message;
            println!("Gemini 模型一原始响应内容: {}", content_str);
            println!("Gemini 响应长度: {} 字符", content_str.len());
            
            // 添加Gemini响应分析
            if content_str.len() <= 10 {
                println!("⚠️ 警告：Gemini响应内容过短，可能存在以下问题：");
                println!("   1. API 密钥配置错误");
                println!("   2. 模型配置问题");
                println!("   3. 请求内容触发了安全限制");
                println!("   4. 图片格式或内容问题");
            }

            let parsed_json = parse_model1_json_response(content_str)?;

            return Ok(StandardModel1Output {
                ocr_text: parsed_json["ocr_text"].as_str().unwrap_or("").to_string(),
                tags: parsed_json["tags"].as_array()
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default(),
                mistake_type: parsed_json["mistake_type"].as_str().unwrap_or("").to_string(),
                raw_response: Some(content_str.to_string()),
            });
        }
        // *** 适配器逻辑结束 ***

        // 读取图片文件并转换为Base64
        let mut images_base64 = Vec::new();
        for path in &image_paths {
            let base64_content = self.file_manager.read_file_as_base64(path)?;
            images_base64.push(base64_content);
        }
        
        // 获取科目专用的Prompt
        let subject_prompt = self.get_subject_prompt(subject, "model1");
        let full_prompt = if let Some(context) = task_context {
            format!("{}\n\n任务上下文: {}\n\n用户问题: {}", subject_prompt, context, user_question)
        } else {
            format!("{}\n\n用户问题: {}", subject_prompt, user_question)
        };
        
        // 强化的JSON指令 - 明确要求纯净JSON输出
        let json_instruction = if config.model.starts_with("gpt-") {
            // GPT模型支持response_format，但仍需明确指令
            "你必须严格返回JSON格式的数据，不要添加任何解释、前缀、后缀或markdown标记。\n\n请分析图片中的题目内容，返回以下格式的JSON：\n{\n  \"ocr_text\": \"题目的完整文字内容\",\n  \"tags\": [\"相关知识点标签\"],\n  \"mistake_type\": \"题目类型分类\"\n}\n\n要求：\n1. 只返回JSON数据，不要任何其他文字\n2. 字符串值必须用双引号包围\n3. 特殊字符必须正确转义"
        } else {
            // 其他模型需要更严格的指令
            "**重要：你必须只返回纯净的JSON数据，不要添加任何解释文字、markdown标记或其他内容。**\n\n请分析图片中的题目内容，严格按照以下JSON格式返回：\n\n{\n  \"ocr_text\": \"题目的完整文字内容\",\n  \"tags\": [\"相关知识点标签1\", \"相关知识点标签2\"],\n  \"mistake_type\": \"题目类型分类\"\n}\n\n**格式要求（必须严格遵守）：**\n1. 响应必须以 { 开始，以 } 结束\n2. 不要添加 ```json 或任何markdown标记\n3. 不要添加任何解释文字或说明\n4. 字符串值必须用双引号包围\n5. 如果文字中包含引号，使用 \\\" 转义\n6. 如果文字中包含反斜杠，使用 \\\\\\ 转义\n7. 换行符使用 \\n 表示\n8. 确保JSON语法完全正确\n\n**示例输出：**\n{\"ocr_text\": \"求解方程 x² + 2x - 3 = 0\", \"tags\": [\"二次方程\", \"代数\"], \"mistake_type\": \"计算题\"}"
        };
        
        let image_context = if images_base64.len() > 1 {
            format!("我将为您提供{}张图片，它们可能是同一道题目的不同部分或多道相关题目。请仔细分析所有图片内容，综合提取完整的题目信息。\n\n", images_base64.len())
        } else {
            "我将为您提供一张图片，请分析其中的题目内容。\n\n".to_string()
        };

        let mut request_content_parts = vec![
            json!({
                "type": "text",
                "text": format!("{}{}\n\n请分析这道题目，提取文字内容，确定题目类型，并生成相关标签。\n\n{}", image_context, full_prompt, json_instruction)
            })
        ];

        // 添加图片
        for image_base64 in &images_base64 {
            let image_format = Self::detect_image_format_from_base64(image_base64);
            println!("🖼️ 模型一检测到图像格式: {}", image_format);
            request_content_parts.push(json!({
                "type": "image_url",
                "image_url": {
                    "url": format!("data:image/{};base64,{}", image_format, image_base64)
                }
            }));
        }

        let mut request_body = json!({
            "model": config.model,
            "messages": [
                {
                    "role": "user",
                    "content": request_content_parts
                }
            ],
            "max_tokens": config.max_output_tokens,
            "stream": false,  // 第一模型不使用流式，因为需要结构化JSON输出
            "temperature": config.temperature,
            "top_p": 0.9,
            "frequency_penalty": 0.0,
            "presence_penalty": 0.0
        });
        
        println!("模型一使用 max_tokens: {} (模型: {})", config.max_output_tokens, config.model);
        
        // 调试：打印请求体（不包含图片内容）
        let debug_body = {
            let mut debug = request_body.clone();
            if let Some(messages) = debug["messages"].as_array_mut() {
                for message in messages {
                    if let Some(content) = message["content"].as_array_mut() {
                        for part in content {
                            if part["type"] == "image_url" {
                                part["image_url"]["url"] = json!("data:image/*;base64,[图片数据已隐藏]");
                            }
                        }
                    }
                }
            }
            debug
        };
        println!("📤 请求体: {}", serde_json::to_string_pretty(&debug_body).unwrap_or_default());

        // 只有支持JSON模式的模型才添加response_format
        // 目前已知支持的模型：gpt-3.5-turbo, gpt-4, gpt-4-turbo等
        if config.model.starts_with("gpt-") {
            request_body["response_format"] = json!({"type": "json_object"});
        }

        // 发送请求
        println!("发送请求到模型一: {}/chat/completions", config.base_url);
        let mut request_builder = self.client
            .post(&format!("{}/chat/completions", config.base_url))
            .header("Authorization", format!("Bearer {}", config.api_key))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/plain, */*")
            .header("Accept-Encoding", "identity")  // 禁用压缩，避免二进制响应
            .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .header("Connection", "keep-alive")
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36");

        if let Ok(parsed_url) = Url::parse(&config.base_url) {
            if (parsed_url.scheme() == "http" || parsed_url.scheme() == "https") && parsed_url.host_str().is_some() {
                let origin_val = format!("{}://{}", parsed_url.scheme(), parsed_url.host_str().unwrap_or_default());
                let referer_val = format!("{}://{}/", parsed_url.scheme(), parsed_url.host_str().unwrap_or_default());
                 request_builder = request_builder
                    .header("Origin", origin_val)
                    .header("Referer", referer_val);
            }
        }
        
        let response = request_builder
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                let error_msg = if e.to_string().contains("timed out") {
                    format!("模型一API请求超时，请检查网络连接或稍后重试: {}", e)
                } else if e.to_string().contains("connect") {
                    format!("无法连接到模型一API服务器，请检查网络和API地址: {}", e)
                } else {
                    format!("模型一API请求失败: {}", e)
                };
                AppError::network(error_msg)
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(AppError::llm(format!("模型一API请求失败: {} - {}", status, error_text)));
        }

        // 先获取原始响应文本进行调试
        let response_text = response.text().await
            .map_err(|e| AppError::llm(format!("获取模型一响应文本失败: {}", e)))?;
        
        // 打印原始响应以供调试（安全处理UTF-8）
        let preview_text = if response_text.len() > 200 {
            // 安全地截取前200个字节，避免UTF-8字符边界问题
            let mut end = 200;
            while end > 0 && !response_text.is_char_boundary(end) {
                end -= 1;
            }
            format!("{}...", &response_text[..end])
        } else {
            response_text.clone()
        };
        
        // 检查是否为二进制数据
        let is_binary = response_text.chars().any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t');
        
        println!("📄 模型一原始响应文本 (前200字符): {}", preview_text);
        if is_binary {
            println!("⚠️ 检测到二进制响应数据，这不是有效的JSON文本");
            println!("📄 响应长度: {} 字节", response_text.len());
            println!("📄 响应开头字节: {:?}", response_text.as_bytes().get(..10).unwrap_or(&[]));
        }
        
        // 检查响应是否为空
        if response_text.trim().is_empty() {
            return Err(AppError::llm("模型一API返回空响应".to_string()));
        }
        
        // 检查是否为流式响应（SSE格式）
        if response_text.contains("data: ") || response_text.starts_with("data:") {
            println!("⚠️ 检测到流式响应，但期望非流式响应");
            return Err(AppError::llm("API返回了流式响应，但模型一需要非流式响应。请检查API配置或切换到支持非流式的模型。".to_string()));
        }
        
        // 清理可能的额外字符（一些API可能在JSON前后添加额外内容）
        let cleaned_response = response_text.trim();
        let cleaned_response = if let Some(start) = cleaned_response.find('{') {
            if let Some(end) = cleaned_response.rfind('}') {
                &cleaned_response[start..=end]
            } else {
                cleaned_response
            }
        } else {
            cleaned_response
        };
        
        // 尝试解析为JSON
        let response_json: Value = serde_json::from_str(cleaned_response)
            .map_err(|e| {
                println!("📄 JSON解析失败的完整响应: {}", response_text);
                println!("📄 清理后的响应: {}", cleaned_response);
                {
                    // 安全地截取响应文本用于错误报告
                    let error_preview = if response_text.len() > 500 {
                        let mut end = 500;
                        while end > 0 && !response_text.is_char_boundary(end) {
                            end -= 1;
                        }
                        format!("{}...", &response_text[..end])
                    } else {
                        response_text.clone()
                    };
                    
                    if is_binary {
                        AppError::llm(format!("API返回了二进制数据而非JSON文本。可能的原因：\n1. 响应被压缩（gzip/deflate）\n2. API配置错误\n3. 网络传输问题\n错误: {}", e))
                    } else {
                        AppError::llm(format!("解析模型一响应失败: {} \n原始响应: {}", e, error_preview))
                    }
                }
            })?;
        
        let content_str = response_json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| AppError::llm("无法解析模型一API响应"))?;

        println!("模型一原始响应内容: {}", content_str);
        println!("响应长度: {} 字符", content_str.len());
        
        // 添加响应分析
        if content_str.len() <= 10 {
            println!("⚠️ 警告：模型响应内容过短，可能存在以下问题：");
            println!("   1. max_tokens 设置过低（当前：{}）", config.max_output_tokens);
            println!("   2. API 密钥权限不足");
            println!("   3. 模型配置错误");
            println!("   4. 请求内容触发了安全限制");
            println!("完整响应JSON: {}", serde_json::to_string_pretty(&response_json).unwrap_or_default());
        }

        // 强化的JSON解析逻辑 - 多层次解析策略
        let parsed_json = parse_model1_json_response(content_str)?;
        
        Ok(StandardModel1Output {
            ocr_text: parsed_json["ocr_text"].as_str().unwrap_or("").to_string(),
            tags: parsed_json["tags"].as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default(),
            mistake_type: parsed_json["mistake_type"].as_str().unwrap_or("").to_string(),
            raw_response: Some(content_str.to_string()),
        })
    }

    // 统一AI接口层 - 模型二（核心解析/对话）- 流式版本
    pub async fn call_unified_model_2_stream(
        &self,
        context: &HashMap<String, Value>,
        chat_history: &[ChatMessage],
        subject: &str,
        enable_chain_of_thought: bool,
        image_paths: Option<Vec<String>>,
        task_context: Option<&str>,
        window: Window,
        stream_event: &str,
    ) -> Result<StandardModel2Output> {
        println!("调用统一模型二接口(流式): 科目={}, 思维链={}, 图片数量={}", 
                subject, enable_chain_of_thought, image_paths.as_ref().map(|p| p.len()).unwrap_or(0));
        
        // 获取模型配置
        let config = self.get_model2_config().await?;
        
        // 处理图片（如果模型支持多模态且提供了图片）
        let images_base64 = if config.is_multimodal && image_paths.is_some() {
            let mut base64_images = Vec::new();
            for path in image_paths.unwrap() {
                let base64_content = self.file_manager.read_file_as_base64(&path)?;
                base64_images.push(base64_content);
            }
            Some(base64_images)
        } else {
            None
        };
        
        let mut messages = vec![];
        
        // 获取科目专用的Prompt
        let mut subject_prompt = self.get_subject_prompt(subject, "model2");
        
        // 添加任务上下文
        if let Some(context_str) = task_context {
            subject_prompt = format!("{}\n\n任务上下文: {}", subject_prompt, context_str);
        }
        
        // 构建系统消息，包含RAG增强内容
        let mut system_content = format!("{}\n\n题目信息:\nOCR文本: {}\n标签: {:?}\n题目类型: {}\n用户原问题: {}",
            subject_prompt,
            context.get("ocr_text").and_then(|v| v.as_str()).unwrap_or(""),
            context.get("tags").and_then(|v| v.as_array()).unwrap_or(&vec![]),
            context.get("mistake_type").and_then(|v| v.as_str()).unwrap_or(""),
            context.get("user_question").and_then(|v| v.as_str()).unwrap_or("")
        );
        
        // 如果有RAG上下文，添加到系统消息中
        if let Some(rag_context) = context.get("rag_context").and_then(|v| v.as_str()) {
            system_content.push_str(&format!("\n\n--- 知识库参考信息 ---\n{}", rag_context));
        }
        
        // 如果有最新用户查询（继续对话时），添加到系统消息中
        if let Some(latest_query) = context.get("latest_user_query").and_then(|v| v.as_str()) {
            system_content.push_str(&format!("\n\n用户最新问题: {}", latest_query));
        }
        
        // 对于推理模型，系统消息需要合并到用户消息中
        if config.is_reasoning {
            // 推理模型不支持系统消息，需要将系统提示合并到用户消息中
            let combined_content = format!("{}\n\n请基于上述信息，提供详细的解答。", system_content);
            
            if config.is_multimodal && images_base64.is_some() && chat_history.is_empty() {
                let mut content = vec![
                    json!({
                        "type": "text",
                        "text": combined_content
                    })
                ];
                
                if let Some(images) = &images_base64 {
                    for image_base64 in images {
                        let image_format = Self::detect_image_format_from_base64(image_base64);
                        println!("🖼️ 检测到图像格式: {}", image_format);
                        content.push(json!({
                            "type": "image_url",
                            "image_url": {
                                "url": format!("data:image/{};base64,{}", image_format, image_base64)
                            }
                        }));
                    }
                }
                
                messages.push(json!({
                    "role": "user",
                    "content": content
                }));
            } else if chat_history.is_empty() {
                messages.push(json!({
                    "role": "user",
                    "content": combined_content
                }));
            } else {
                // 如果有聊天历史，将系统提示添加到第一条用户消息前
                messages.push(json!({
                    "role": "user",
                    "content": format!("{}请基于前面的信息，回答我的新问题。", system_content)
                }));
            }
        } else {
            // 非推理模型使用标准的系统消息
            messages.push(json!({
                "role": "system",
                "content": system_content
            }));

            // 如果是多模态模型且提供了图片，添加图片到第一条用户消息
            if config.is_multimodal && images_base64.is_some() && chat_history.is_empty() {
                let mut content = vec![
                    json!({
                        "type": "text",
                        "text": "请基于上述信息和图片，提供详细的解答。"
                    })
                ];

                if let Some(images) = &images_base64 {
                    for image_base64 in images {
                        let image_format = Self::detect_image_format_from_base64(image_base64);
                        println!("🖼️ 检测到图像格式: {}", image_format);
                        content.push(json!({
                            "type": "image_url",
                            "image_url": {
                                "url": format!("data:image/{};base64,{}", image_format, image_base64)
                            }
                        }));
                    }
                }

                messages.push(json!({
                    "role": "user",
                    "content": content
                }));
            } else if chat_history.is_empty() {
                // 纯文本模型或没有提供图片
                messages.push(json!({
                    "role": "user",
                    "content": "请基于上述信息，提供详细的解答。"
                }));
            }
        }

        // 添加聊天历史
        for (index, msg) in chat_history.iter().enumerate() {
            // 🎯 修复：如果是多模态模型且有图片，在最后一条用户消息中添加图片
            if msg.role == "user" && index == chat_history.len() - 1 && config.is_multimodal && images_base64.is_some() {
                let mut content = vec![
                    json!({
                        "type": "text",
                        "text": msg.content.clone()
                    })
                ];

                if let Some(images) = &images_base64 {
                    for image_base64 in images {
                        let image_format = Self::detect_image_format_from_base64(image_base64);
                        println!("🖼️ 检测到图像格式: {}", image_format);
                        content.push(json!({
                            "type": "image_url",
                            "image_url": {
                                "url": format!("data:image/{};base64,{}", image_format, image_base64)
                            }
                        }));
                    }
                }

                messages.push(json!({
                    "role": msg.role,
                    "content": content
                }));
            } else {
                messages.push(json!({
                    "role": msg.role,
                    "content": msg.content
                }));
            }
        }

        let mut request_body = json!({
            "model": config.model,
            "messages": messages,
            "stream": true
        });

        // 根据模型适配器类型和是否为推理模型设置不同的参数
        println!("模型适配器类型: {}, 推理模型: {}, 前端请求思维链: {}", 
                 config.model_adapter, config.is_reasoning, enable_chain_of_thought);
        
        let is_deepseek_model_family = config.model_adapter == "deepseek-r1" || 
                                     config.model.to_lowercase().contains("deepseek");

        if config.is_reasoning { // 目标是推理模型
            match config.model_adapter.as_str() {
                "deepseek-r1" => {
                    request_body["max_tokens"] = json!(config.max_output_tokens);
                    // 只有当明确是 deepseek-r1 且前端请求了思维链时，才添加 stream_options
                    // 或者，如果 deepseek-r1 总是需要这个选项来保证流式稳定，则无条件添加
                    if enable_chain_of_thought { // 或者无条件添加 if deepseek-r1 needs it
                         request_body["stream_options"] = json!({"include_usage": true});
                         println!("应用 DeepSeek-R1 特殊参数 (启用思维链): max_tokens={}, stream_options=include_usage", config.max_output_tokens);
                    } else {
                         println!("应用 DeepSeek-R1 特殊参数 (未启用思维链): max_tokens={}", config.max_output_tokens);
                    }
                },
                _ => { // 其他推理模型
                    request_body["max_completion_tokens"] = json!(config.max_output_tokens);
                    println!("应用通用推理模型参数: max_completion_tokens={}", config.max_output_tokens);
                }
            }
        } else { // 目标是非推理模型 (例如 deepseek-v3 可能落入此分支)
            if is_deepseek_model_family {
                request_body["max_tokens"] = json!(config.max_output_tokens);
                request_body["temperature"] = json!(config.temperature);
                // 对于非推理的DeepSeek模型，如果它们不支持或不需要 stream_options，则不应添加
                // 如果它们也需要 stream_options 来稳定流式输出，则可以考虑添加
                // request_body["stream_options"] = json!({"include_usage": true}); 
                println!("应用 DeepSeek 普通模型参数: max_tokens={}, temperature={}", config.max_output_tokens, config.temperature);
            } else { // 其他通用非推理模型
                request_body["max_tokens"] = json!(config.max_output_tokens);
                request_body["temperature"] = json!(config.temperature);
                println!("应用普通模型参数: max_tokens={}, temperature={}", config.max_output_tokens, config.temperature);
            }
            
            // 关键：如果模型是非推理模型，即使前端请求了思维链，
            // 也不要向API发送特定于思维链的参数，除非该模型明确支持。
            // 对于通用模型，通常不需要为"思维链"传递特殊参数，模型会自然地按指令回复。
            // 如果 enable_chain_of_thought 对非推理模型意味着不同的处理（例如，更详细的回复），
            // 这里的逻辑可能需要调整，但通常是Prompt工程的一部分，而不是API参数。
            if enable_chain_of_thought {
                println!("警告: 前端为非推理模型 {} 请求了思维链。通常这由Prompt控制，而非特定API参数。", config.model);
            }
        }

        let mut request_builder = self.client
            .post(&format!("{}/chat/completions", config.base_url))
            .header("Authorization", format!("Bearer {}", config.api_key))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/plain, */*")
            .header("Accept-Encoding", "identity")  // 禁用压缩，避免二进制响应
            .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .header("Connection", "keep-alive")
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36");

        if let Ok(parsed_url) = Url::parse(&config.base_url) {
            if (parsed_url.scheme() == "http" || parsed_url.scheme() == "https") && parsed_url.host_str().is_some() {
                let origin_val = format!("{}://{}", parsed_url.scheme(), parsed_url.host_str().unwrap_or_default());
                let referer_val = format!("{}://{}/", parsed_url.scheme(), parsed_url.host_str().unwrap_or_default());
                 request_builder = request_builder
                    .header("Origin", origin_val)
                    .header("Referer", referer_val);
            }
        }
        
        let response = request_builder
            .json(&request_body)
            .send()
            .await
            .map_err(|e| AppError::network(format!("模型二API请求失败: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(AppError::llm(format!("模型二API请求失败: {} - {}", status, error_text)));
        }

        let mut stream = response.bytes_stream();
        let mut full_content = String::new();
        let mut reasoning_content = String::new(); // 收集思维链内容
        let mut chunk_counter = 0;

        let mut stream_ended = false;
        
        // 添加调试日志输出
        println!("🚀 发送给模型二的请求:");
        println!("URL: {}/chat/completions", config.base_url);
        println!("Model: {}", config.model);
        println!("请求内容: {}", serde_json::to_string_pretty(&request_body).unwrap_or_else(|_| "序列化失败".to_string()));
        println!("---请求结束---");
        
        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    let chunk_str = String::from_utf8_lossy(&chunk);
                    
                    // 输出原始响应块（仅输出前几个块避免日志过多）
                    if chunk_counter < 5 || stream_ended {
                        println!("📥 原始响应块 #{}: {}", chunk_counter, chunk_str);
                    }
                    
                    // 处理SSE格式的数据
                    for line in chunk_str.lines() {
                        if line.starts_with("data: ") {
                            let data = &line[6..]; // 移除 "data: " 前缀
                            
                            if data == "[DONE]" {
                                // 流结束，标记但不立即发送完成信号
                                stream_ended = true;
                                println!("📍 收到流结束标记 [DONE]");
                                break;
                            }
                            
                            // 特殊处理DeepSeek错误格式
                            if is_deepseek_model_family && (data.contains("升级") || data.contains("关闭") || data.contains("日志")) {
                                println!("❌ 检测到DeepSeek特定错误: {}", data);
                                let error_message = format!("DeepSeek API错误: {}", data);
                                
                                // 发送错误事件到前端
                                if let Err(e) = window.emit("stream_error", &serde_json::json!({"error": error_message})) {
                                    println!("发送DeepSeek错误事件失败: {}", e);
                                }
                                
                                return Err(AppError::llm(error_message));
                            }
                            
                            // 尝试解析JSON
                            if let Ok(json_data) = serde_json::from_str::<Value>(data) {
                                // 输出解析后的JSON结构（仅输出前几个）
                                if chunk_counter < 5 {
                                    println!("🔍 解析后的JSON数据: {}", serde_json::to_string_pretty(&json_data).unwrap_or_else(|_| "序列化失败".to_string()));
                                }
                                
                                if let Some(choices) = json_data["choices"].as_array() {
                                    if let Some(choice) = choices.first() {
                                        if let Some(delta) = choice["delta"].as_object() {
                                            // DeepSeek-R1 推理模型的思维链内容在 reasoning_content 字段
                                            if let Some(reasoning_content_chunk) = delta.get("reasoning_content").and_then(|v| v.as_str()) {
                                                // 这是思维链内容
                                                reasoning_content.push_str(reasoning_content_chunk); // 收集思维链
                                                chunk_counter += 1;
                                                
                                                if chunk_counter < 10 {
                                                    println!("🧠 收到思维链块: {}", reasoning_content_chunk);
                                                }
                                                
                                                let stream_chunk = StreamChunk {
                                                    content: reasoning_content_chunk.to_string(),
                                                    is_complete: false,
                                                    chunk_id: format!("reasoning_chunk_{}", chunk_counter),
                                                };
                                                
                                                // 发送思维链流事件到前端
                                                if let Err(e) = window.emit(&format!("{}_reasoning", stream_event), &stream_chunk) {
                                                    println!("发送思维链流事件失败: {}", e);
                                                }
                                            }
                                            
                                            // 普通内容仍在 content 字段
                                            if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                                                full_content.push_str(content);
                                                chunk_counter += 1;
                                                
                                                if chunk_counter < 10 {
                                                    println!("💬 收到主内容块: {}", content);
                                                }
                                                
                                                let stream_chunk = StreamChunk {
                                                    content: content.to_string(),
                                                    is_complete: false,
                                                    chunk_id: format!("chunk_{}", chunk_counter),
                                                };
                                                
                                                // 发送流事件到前端
                                                if let Err(e) = window.emit(stream_event, &stream_chunk) {
                                                    println!("发送流事件失败: {}", e);
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                // JSON解析失败，输出原始数据以便调试
                                if chunk_counter < 10 {
                                    println!("⚠️ JSON解析失败，原始数据: {}", data);
                                }
                                
                                // 检查是否是错误信息
                                if data.contains("error") || data.contains("Error") || data.contains("升级") {
                                    println!("❌ 检测到流式请求错误: {}", data);
                                    
                                    // 尝试解析错误信息
                                    let error_message = if let Ok(error_json) = serde_json::from_str::<serde_json::Value>(data) {
                                        if let Some(error_msg) = error_json.get("error").and_then(|e| e.as_str()) {
                                            error_msg.to_string()
                                        } else if let Some(message) = error_json.get("message").and_then(|m| m.as_str()) {
                                            message.to_string()
                                        } else {
                                            format!("流式请求错误: {}", data)
                                        }
                                    } else {
                                        format!("流式请求解析错误: {}", data)
                                    };
                                    
                                    // 发送错误事件到前端
                                    if let Err(e) = window.emit("stream_error", &serde_json::json!({"error": error_message})) {
                                        println!("发送错误事件失败: {}", e);
                                    }
                                    
                                    return Err(AppError::llm(error_message));
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    println!("❌ 流读取错误: {}", e);
                    println!("已处理块数: {}, 主内容长度: {}, 思维链长度: {}", 
                        chunk_counter, full_content.len(), reasoning_content.len());
                    
                    // 如果已经有内容，不把这当作完全失败
                    if !full_content.is_empty() || !reasoning_content.is_empty() {
                        println!("⚠️ 部分内容已接收，标记为部分成功");
                        break;
                    } else {
                        println!("💥 没有接收到任何内容，这是完全失败");
                        return Err(AppError::network(format!("流式请求失败: {}", e)));
                    }
                }
            }
            
            // 如果流已结束，退出循环
            if stream_ended {
                break;
            }
        }

        // 输出最终收集的内容长度
        println!("📊 流式响应完成统计:");
        println!("  - 主内容总长度: {} 字符", full_content.len());
        println!("  - 思维链总长度: {} 字符", reasoning_content.len());
        println!("  - 主内容预览:");
        if full_content.chars().count() > 50 {
            let preview: String = full_content.chars().take(50).collect();
            println!("{}...", preview);
        } else {
            println!("{}", full_content);
        }
        println!("  - 思维链预览:");
        if reasoning_content.chars().count() > 50 {
            let preview: String = reasoning_content.chars().take(50).collect();
            println!("{}...", preview);
        } else {
            println!("{}", reasoning_content);
        }

        // 发送最终完成信号到主内容流
        let final_chunk = StreamChunk {
            content: full_content.clone(), // 发送完整内容而不是空字符串
            is_complete: true,
            chunk_id: format!("final_chunk_{}", chunk_counter + 1),
        };
        
        if let Err(e) = window.emit(stream_event, &final_chunk) {
            println!("发送最终主内容完成信号失败: {}", e);
        } else {
            println!("✅ 发送主内容完成信号成功，内容长度: {}", full_content.len());
        }

        // 如果有思维链内容，也发送思维链完成信号
        if enable_chain_of_thought && !reasoning_content.is_empty() {
            let reasoning_final_chunk = StreamChunk {
                content: reasoning_content.clone(), // 也发送完整的思维链内容
                is_complete: true,
                chunk_id: format!("reasoning_final_chunk_{}", chunk_counter + 1),
            };
            
            if let Err(e) = window.emit(&format!("{}_reasoning", stream_event), &reasoning_final_chunk) {
                println!("发送思维链完成信号失败: {}", e);
            } else {
                println!("✅ 发送思维链完成信号成功，内容长度: {}", reasoning_content.len());
            }
        }

        // 如果启用了思维链，尝试提取思维链详情
        let is_deepseek_r1 = config.model_adapter == "deepseek-r1";
        let chain_of_thought_details = if enable_chain_of_thought || is_deepseek_r1 {
            if config.is_reasoning || is_deepseek_r1 {
                // 推理模型自动包含思维链
                let details = if is_deepseek_r1 && !reasoning_content.is_empty() {
                    // DeepSeek-R1 的思维链在 reasoning_content 中
                    json!({
                        "full_response": full_content,
                        "reasoning_content": reasoning_content,
                        "enabled": true,
                        "is_reasoning_model": true,
                        "model_type": "deepseek-r1",
                        "model_adapter": config.model_adapter,
                        "parsed_sections": extract_reasoning_sections(&reasoning_content)
                    })
                } else {
                    // 其他推理模型的思维链在主内容中
                    json!({
                        "full_response": full_content,
                        "enabled": true,
                        "is_reasoning_model": true,
                        "model_adapter": config.model_adapter,
                        "parsed_sections": extract_reasoning_sections(&full_content)
                    })
                };
                Some(details)
            } else {
                // 普通模型的思维链处理
                Some(json!({
                    "full_response": full_content,
                    "enabled": true,
                    "is_reasoning_model": false,
                    "model_adapter": config.model_adapter
                }))
            }
        } else {
            None
        };

        Ok(StandardModel2Output {
            assistant_message: full_content,
            raw_response: Some("stream_response".to_string()),
            chain_of_thought_details,
        })
    }

    // 🎯 新增：通用流式接口，支持自定义模型配置（用于总结请求等特殊场景）
    pub async fn call_unified_model_stream_with_config(
        &self,
        config: &ApiConfig,
        context: &HashMap<String, Value>,
        chat_history: &[ChatMessage],
        subject: &str,
        enable_chain_of_thought: bool,
        image_paths: Option<Vec<String>>,
        task_context: Option<&str>,
        window: Window,
        stream_event: &str,
    ) -> Result<StandardModel2Output> {
        println!("调用通用流式接口: 模型={}, 科目={}, 思维链={}, 图片数量={}",
                config.model, subject, enable_chain_of_thought, image_paths.as_ref().map(|p| p.len()).unwrap_or(0));

        // *** 新增的适配器路由逻辑 ***
        if config.model_adapter == "google" {
            // 处理图片（如果模型支持多模态且提供了图片）
            let images_base64 = if config.is_multimodal && image_paths.is_some() {
                let mut base64_images = Vec::new();
                for path in image_paths.as_ref().unwrap() {
                    let base64_content = self.file_manager.read_file_as_base64(path)?;
                    base64_images.push(base64_content);
                }
                Some(base64_images)
            } else {
                None
            };

            // 构建 ChatMessage 格式的消息历史
            let mut messages_with_images = chat_history.to_vec();

            // 如果有图片且是多模态模型，将图片添加到最后一条用户消息
            if let Some(images) = images_base64 {
                if let Some(last_msg) = messages_with_images.last_mut() {
                    if last_msg.role == "user" {
                        last_msg.image_base64 = Some(images);
                    }
                } else {
                    // 如果没有聊天历史，创建一个包含图片的用户消息
                    let mut system_content = String::new();

                    // 构建系统提示词
                    if !context.is_empty() {
                        for (key, value) in context {
                            match key.as_str() {
                                "ocr_text" => system_content.push_str(&format!("题目内容: {}\n", value.as_str().unwrap_or(""))),
                                "user_question" => system_content.push_str(&format!("学生问题: {}\n", value.as_str().unwrap_or(""))),
                                "tags" => {
                                    if let Some(tags_array) = value.as_array() {
                                        let tags: Vec<String> = tags_array.iter()
                                            .filter_map(|v| v.as_str())
                                            .map(|s| s.to_string())
                                            .collect();
                                        if !tags.is_empty() {
                                            system_content.push_str(&format!("相关标签: {}\n", tags.join(", ")));
                                        }
                                    }
                                },
                                "mistake_type" => system_content.push_str(&format!("题目类型: {}\n", value.as_str().unwrap_or(""))),
                                _ => {}
                            }
                        }
                    }

                    system_content.push_str("请基于上述信息和图片，提供详细的解答。");

                    let message = ChatMessage {
                        role: "user".to_string(),
                        content: system_content,
                        timestamp: chrono::Utc::now(),
                        thinking_content: None,
                        rag_sources: None,
                        image_paths: None,
                        image_base64: Some(images),
                    };

                    messages_with_images.push(message);
                }
            }

            // 调用 Gemini 适配器（流式）
            return gemini_adapter::stream_chat(
                &self.client,
                config,
                &messages_with_images,
                window,
                stream_event,
            ).await;
        }
        // *** 适配器逻辑结束 ***

        // 处理图片（如果模型支持多模态且提供了图片）
        let images_base64 = if config.is_multimodal && image_paths.is_some() {
            let mut base64_images = Vec::new();
            for path in image_paths.unwrap() {
                let base64_content = self.file_manager.read_file_as_base64(&path)?;
                base64_images.push(base64_content);
            }
            Some(base64_images)
        } else {
            None
        };
        
        let mut messages = vec![];
        
        // 获取科目专用的Prompt
        let subject_prompt = self.db.get_subject_config_by_name(subject)
            .unwrap_or(None)
            .map(|config| {
                match task_context {
                    Some(task) if task.contains("总结") || task.contains("summary") => {
                        format!("【科目专用指导 - {}】\n{}\n\n", subject, config.prompts.analysis_prompt)
                    },
                    _ => {
                        format!("【科目专用指导 - {}】\n{}\n\n", subject, config.prompts.chat_prompt)
                    }
                }
            })
            .unwrap_or_else(|| format!("请基于{}科目的特点进行分析。\n\n", subject));

        // 构建系统提示词（使用与call_unified_model_2_stream相同的逻辑）
        if !context.is_empty() {
            let mut system_content = subject_prompt;
            
            if let Some(task_ctx) = task_context {
                system_content.push_str(&format!("【任务背景】\n{}\n\n", task_ctx));
            }
            
            for (key, value) in context {
                match key.as_str() {
                    "ocr_text" => system_content.push_str(&format!("【题目内容】\n{}\n\n", value.as_str().unwrap_or(""))),
                    "user_question" => system_content.push_str(&format!("【学生问题】\n{}\n\n", value.as_str().unwrap_or(""))),
                    "tags" => {
                        if let Some(tags_array) = value.as_array() {
                            let tags: Vec<String> = tags_array.iter()
                                .filter_map(|v| v.as_str())
                                .map(|s| s.to_string())
                                .collect();
                            if !tags.is_empty() {
                                system_content.push_str(&format!("【相关标签】\n{}\n\n", tags.join(", ")));
                            }
                        }
                    },
                    "mistake_type" => system_content.push_str(&format!("【题目类型】\n{}\n\n", value.as_str().unwrap_or(""))),
                    _ => {}
                }
            }

            messages.push(json!({
                "role": "system",
                "content": system_content
            }));

            // 如果是多模态模型且提供了图片，添加图片到第一条用户消息
            if config.is_multimodal && images_base64.is_some() && chat_history.is_empty() {
                let mut content = vec![
                    json!({
                        "type": "text",
                        "text": "请基于上述信息和图片，提供详细的解答。"
                    })
                ];
                
                if let Some(images) = &images_base64 {
                    for image_base64 in images {
                        let image_format = Self::detect_image_format_from_base64(image_base64);
                        println!("🖼️ 检测到图像格式: {}", image_format);
                        content.push(json!({
                            "type": "image_url",
                            "image_url": {
                                "url": format!("data:image/{};base64,{}", image_format, image_base64)
                            }
                        }));
                    }
                }
                
                messages.push(json!({
                    "role": "user",
                    "content": content
                }));
            } else if chat_history.is_empty() {
                // 纯文本模型或没有提供图片
                messages.push(json!({
                    "role": "user",
                    "content": "请基于上述信息，提供详细的解答。"
                }));
            }
        }

        // 添加聊天历史
        for (index, msg) in chat_history.iter().enumerate() {
            // 🎯 修复：如果是多模态模型且有图片，在最后一条用户消息中添加图片
            if msg.role == "user" && index == chat_history.len() - 1 && config.is_multimodal && images_base64.is_some() {
                let mut content = vec![
                    json!({
                        "type": "text",
                        "text": msg.content.clone()
                    })
                ];

                if let Some(images) = &images_base64 {
                    for image_base64 in images {
                        let image_format = Self::detect_image_format_from_base64(image_base64);
                        println!("🖼️ 检测到图像格式: {}", image_format);
                        content.push(json!({
                            "type": "image_url",
                            "image_url": {
                                "url": format!("data:image/{};base64,{}", image_format, image_base64)
                            }
                        }));
                    }
                }

                messages.push(json!({
                    "role": msg.role,
                    "content": content
                }));
            } else {
                messages.push(json!({
                    "role": msg.role,
                    "content": msg.content
                }));
            }
        }

        let mut request_body = json!({
            "model": config.model,
            "messages": messages,
            "stream": true
        });

        // 根据模型适配器添加特定参数
        match config.model_adapter.as_str() {
            "deepseek-r1" => {
                request_body["max_reasoning_tokens"] = json!(config.max_output_tokens);
                request_body["max_completion_tokens"] = json!(4096);
                request_body["temperature"] = json!(config.temperature);
            },
            _ => {
                request_body["max_tokens"] = json!(config.max_output_tokens);
                request_body["temperature"] = json!(config.temperature);
            }
        }

        println!("发送请求到: {}", config.base_url);
        let mut request_builder = self.client
            .post(&format!("{}/chat/completions", config.base_url))
            .header("Authorization", format!("Bearer {}", config.api_key))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/plain, */*")
            .header("Accept-Encoding", "identity")  // 禁用压缩，避免二进制响应
            .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .header("Connection", "keep-alive")
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36");

        if let Ok(parsed_url) = Url::parse(&config.base_url) { // config is a parameter here
            if (parsed_url.scheme() == "http" || parsed_url.scheme() == "https") && parsed_url.host_str().is_some() {
                let origin_val = format!("{}://{}", parsed_url.scheme(), parsed_url.host_str().unwrap_or_default());
                let referer_val = format!("{}://{}/", parsed_url.scheme(), parsed_url.host_str().unwrap_or_default());
                 request_builder = request_builder
                    .header("Origin", origin_val)
                    .header("Referer", referer_val);
            }
        }
        
        let response = request_builder
            .json(&request_body)
            .send()
            .await
            .map_err(|e| AppError::network(format!("请求失败: {}", e)))?;

        // 流式处理响应（使用与call_unified_model_2_stream相同的逻辑）
        let mut stream = response.bytes_stream();
        let mut full_content = String::new();
        let mut reasoning_content = String::new();
        let mut chunk_counter = 0;

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    let chunk_str = String::from_utf8_lossy(&chunk);
                    
                    for line in chunk_str.lines() {
                        if line.starts_with("data: ") {
                            let data = &line[6..];
                            if data == "[DONE]" {
                                break;
                            }
                            
                            if let Ok(json_data) = serde_json::from_str::<Value>(data) {
                                if let Some(choices) = json_data["choices"].as_array() {
                                    if let Some(choice) = choices.first() {
                                        // 处理主内容
                                        if let Some(delta) = choice["delta"].as_object() {
                                            if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                                                full_content.push_str(content);
                                                
                                                let chunk = StreamChunk {
                                                    content: content.to_string(),
                                                    is_complete: false,
                                                    chunk_id: format!("chunk_{}", chunk_counter),
                                                };
                                                
                                                if let Err(e) = window.emit(stream_event, &chunk) {
                                                    println!("发送流式数据失败: {}", e);
                                                }
                                                
                                                chunk_counter += 1;
                                            }
                                        }
                                        
                                        // 处理思维链内容（DeepSeek-R1）
                                        if config.model_adapter == "deepseek-r1" {
                                            if let Some(delta) = choice["delta"].as_object() {
                                                if let Some(reasoning) = delta.get("reasoning_content").and_then(|v| v.as_str()) {
                                                    reasoning_content.push_str(reasoning);
                                                    
                                                    let reasoning_chunk = StreamChunk {
                                                        content: reasoning.to_string(),
                                                        is_complete: false,
                                                        chunk_id: format!("reasoning_chunk_{}", chunk_counter),
                                                    };
                                                    
                                                    if let Err(e) = window.emit(&format!("{}_reasoning", stream_event), &reasoning_chunk) {
                                                        println!("发送思维链流式数据失败: {}", e);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    println!("流式响应错误: {}", e);
                    break;
                }
            }
        }

        // 发送完成信号
        let final_chunk = StreamChunk {
            content: full_content.clone(),
            is_complete: true,
            chunk_id: format!("final_chunk_{}", chunk_counter),
        };
        
        if let Err(e) = window.emit(stream_event, &final_chunk) {
            println!("发送最终完成信号失败: {}", e);
        }

        // 如果有思维链内容，也发送思维链完成信号
        if enable_chain_of_thought && !reasoning_content.is_empty() {
            let reasoning_final_chunk = StreamChunk {
                content: reasoning_content.clone(),
                is_complete: true,
                chunk_id: format!("reasoning_final_chunk_{}", chunk_counter + 1),
            };
            
            if let Err(e) = window.emit(&format!("{}_reasoning", stream_event), &reasoning_final_chunk) {
                println!("发送思维链完成信号失败: {}", e);
            }
        }

        // 构建思维链详情
        let is_deepseek_r1 = config.model_adapter == "deepseek-r1";
        let chain_of_thought_details = if enable_chain_of_thought || is_deepseek_r1 {
            if config.is_reasoning || is_deepseek_r1 {
                let details = if is_deepseek_r1 && !reasoning_content.is_empty() {
                    json!({
                        "full_response": full_content,
                        "reasoning_content": reasoning_content,
                        "enabled": true,
                        "is_reasoning_model": true,
                        "model_type": "deepseek-r1",
                        "model_adapter": config.model_adapter
                    })
                } else {
                    json!({
                        "full_response": full_content,
                        "enabled": true,
                        "is_reasoning_model": true,
                        "model_adapter": config.model_adapter
                    })
                };
                Some(details)
            } else {
                None
            }
        } else {
            None
        };

        Ok(StandardModel2Output {
            assistant_message: full_content,
            raw_response: Some("stream_response".to_string()),
            chain_of_thought_details,
        })
    }

    // 统一AI接口层 - 模型二（核心解析/对话）- 非流式版本（保持向后兼容）
    pub async fn call_unified_model_2(
        &self,
        context: &HashMap<String, Value>,
        chat_history: &[ChatMessage],
        subject: &str,
        enable_chain_of_thought: bool,
        image_paths: Option<Vec<String>>,
        task_context: Option<&str>,
    ) -> Result<StandardModel2Output> {
        println!("调用统一模型二接口: 科目={}, 思维链={}, 图片数量={}", 
                subject, enable_chain_of_thought, image_paths.as_ref().map(|p| p.len()).unwrap_or(0));
        
        // 获取模型配置
        let config = self.get_model2_config().await?;
        
        // 处理图片（如果模型支持多模态且提供了图片）
        let images_base64 = if config.is_multimodal && image_paths.is_some() {
            let mut base64_images = Vec::new();
            for path in image_paths.unwrap() {
                let base64_content = self.file_manager.read_file_as_base64(&path)?;
                base64_images.push(base64_content);
            }
            Some(base64_images)
        } else {
            None
        };
        
        let mut messages = vec![];
        
        // 获取科目专用的Prompt
        let mut subject_prompt = self.get_subject_prompt(subject, "model2");
        
        // 添加任务上下文
        if let Some(context_str) = task_context {
            subject_prompt = format!("{}\n\n任务上下文: {}", subject_prompt, context_str);
        }
        
        // 构建系统消息，包含RAG增强内容
        let mut system_content = format!("{}\n\n题目信息:\nOCR文本: {}\n标签: {:?}\n题目类型: {}\n用户原问题: {}",
            subject_prompt,
            context.get("ocr_text").and_then(|v| v.as_str()).unwrap_or(""),
            context.get("tags").and_then(|v| v.as_array()).unwrap_or(&vec![]),
            context.get("mistake_type").and_then(|v| v.as_str()).unwrap_or(""),
            context.get("user_question").and_then(|v| v.as_str()).unwrap_or("")
        );
        
        // 如果有RAG上下文，添加到系统消息中
        if let Some(rag_context) = context.get("rag_context").and_then(|v| v.as_str()) {
            system_content.push_str(&format!("\n\n--- 知识库参考信息 ---\n{}", rag_context));
        }
        
        // 如果有最新用户查询（继续对话时），添加到系统消息中
        if let Some(latest_query) = context.get("latest_user_query").and_then(|v| v.as_str()) {
            system_content.push_str(&format!("\n\n用户最新问题: {}", latest_query));
        }
        
        // 对于推理模型，系统消息需要合并到用户消息中
        if config.is_reasoning {
            // 推理模型不支持系统消息，需要将系统提示合并到用户消息中
            let combined_content = format!("{}\n\n请基于上述信息，提供详细的解答。", system_content);
            
            if config.is_multimodal && images_base64.is_some() && chat_history.is_empty() {
                let mut content = vec![
                    json!({
                        "type": "text",
                        "text": combined_content
                    })
                ];
                
                if let Some(images) = &images_base64 {
                    for image_base64 in images {
                        let image_format = Self::detect_image_format_from_base64(image_base64);
                        println!("🖼️ 检测到图像格式: {}", image_format);
                        content.push(json!({
                            "type": "image_url",
                            "image_url": {
                                "url": format!("data:image/{};base64,{}", image_format, image_base64)
                            }
                        }));
                    }
                }
                
                messages.push(json!({
                    "role": "user",
                    "content": content
                }));
            } else if chat_history.is_empty() {
                messages.push(json!({
                    "role": "user",
                    "content": combined_content
                }));
            } else {
                // 如果有聊天历史，将系统提示添加到第一条用户消息前
                messages.push(json!({
                    "role": "user",
                    "content": format!("{}请基于前面的信息，回答我的新问题。", system_content)
                }));
            }
        } else {
            // 非推理模型使用标准的系统消息
            messages.push(json!({
                "role": "system",
                "content": system_content
            }));

            // 如果是多模态模型且提供了图片，添加图片到第一条用户消息
            if config.is_multimodal && images_base64.is_some() && chat_history.is_empty() {
                let mut content = vec![
                    json!({
                        "type": "text",
                        "text": "请基于上述信息和图片，提供详细的解答。"
                    })
                ];
                
                if let Some(images) = &images_base64 {
                    for image_base64 in images {
                        let image_format = Self::detect_image_format_from_base64(image_base64);
                        println!("🖼️ 检测到图像格式: {}", image_format);
                        content.push(json!({
                            "type": "image_url",
                            "image_url": {
                                "url": format!("data:image/{};base64,{}", image_format, image_base64)
                            }
                        }));
                    }
                }
                
                messages.push(json!({
                    "role": "user",
                    "content": content
                }));
            } else if chat_history.is_empty() {
                // 纯文本模型或没有提供图片
                messages.push(json!({
                    "role": "user",
                    "content": "请基于上述信息，提供详细的解答。"
                }));
            }
        }

        // 添加聊天历史
        for msg in chat_history {
            messages.push(json!({
                "role": msg.role,
                "content": msg.content
            }));
        }

        let mut request_body = json!({
            "model": config.model,
            "messages": messages,
            "stream": false  // 非流式版本
        });

        // 根据模型适配器类型设置不同的参数
        println!("模型适配器类型: {}, 推理模型: {}", config.model_adapter, config.is_reasoning);
        
        if config.is_reasoning {
            // 推理模型的特殊参数
            match config.model_adapter.as_str() {
                "deepseek-r1" => {
                    request_body["max_tokens"] = json!(config.max_output_tokens);
                    println!("应用 DeepSeek-R1 特殊参数: max_tokens={}", config.max_output_tokens);
                },
                _ => {
                    // 其他推理模型（如o1系列）
                    request_body["max_completion_tokens"] = json!(config.max_output_tokens);
                    println!("应用通用推理模型参数: max_completion_tokens={}", config.max_output_tokens);
                }
            }
        } else {
            // 普通模型的标准参数
            request_body["max_tokens"] = json!(config.max_output_tokens);
            request_body["temperature"] = json!(config.temperature);
            println!("应用普通模型参数: max_tokens={}, temperature={}", config.max_output_tokens, config.temperature);
        }

        let mut request_builder = self.client
            .post(&format!("{}/chat/completions", config.base_url))
            .header("Authorization", format!("Bearer {}", config.api_key))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/plain, */*")
            .header("Accept-Encoding", "identity")  // 禁用压缩，避免二进制响应
            .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .header("Connection", "keep-alive")
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36");

        if let Ok(parsed_url) = Url::parse(&config.base_url) {
            if (parsed_url.scheme() == "http" || parsed_url.scheme() == "https") && parsed_url.host_str().is_some() {
                let origin_val = format!("{}://{}", parsed_url.scheme(), parsed_url.host_str().unwrap_or_default());
                let referer_val = format!("{}://{}/", parsed_url.scheme(), parsed_url.host_str().unwrap_or_default());
                 request_builder = request_builder
                    .header("Origin", origin_val)
                    .header("Referer", referer_val);
            }
        }
        
        let response = request_builder
            .json(&request_body)
            .send()
            .await
            .map_err(|e| AppError::network(format!("模型二API请求失败: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(AppError::llm(format!("模型二API请求失败: {} - {}", status, error_text)));
        }

        let response_json: Value = response.json().await
            .map_err(|e| AppError::llm(format!("解析模型二响应失败: {}", e)))?;
        
        let content = response_json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| AppError::llm("无法解析模型二API响应"))?;

        // 如果启用了思维链，尝试提取思维链详情
        let is_deepseek_r1 = config.model_adapter == "deepseek-r1";
        let chain_of_thought_details = if enable_chain_of_thought || is_deepseek_r1 {
            // 这里可以根据响应内容解析思维链步骤
            // 暂时将完整响应作为思维链详情
            Some(json!({
                "full_response": content,
                "enabled": true,
                "is_reasoning_model": config.is_reasoning,
                "model_adapter": config.model_adapter
            }))
        } else {
            None
        };

        Ok(StandardModel2Output {
            assistant_message: content.to_string(),
            raw_response: Some(response_json.to_string()),
            chain_of_thought_details,
        })
    }

    // 测试API连接 - 支持指定模型名称
    pub async fn test_connection(&self, api_key: &str, base_url: &str) -> Result<bool> {
        self.test_connection_with_model(api_key, base_url, None).await
    }

    // 测试API连接 - 可以指定具体模型
    pub async fn test_connection_with_model(&self, api_key: &str, base_url: &str, model_name: Option<&str>) -> Result<bool> {
        println!("测试API连接: {} (密钥长度: {})", base_url, api_key.len());
        
        // 确保base_url格式正确
        let normalized_url = if base_url.ends_with('/') {
            base_url.trim_end_matches('/').to_string()
        } else {
            base_url.to_string()
        };
        
        // 如果指定了模型名称，优先使用指定的模型
        let test_models = if let Some(specified_model) = model_name {
            vec![specified_model.to_string()]
        } else {
            // 使用通用的测试模型名称，不同API提供商可能支持不同的模型
            vec![
                "gpt-3.5-turbo".to_string(),           // OpenAI
                "deepseek-chat".to_string(),           // DeepSeek
                "Qwen/Qwen2-7B-Instruct".to_string(),  // SiliconFlow
                "meta-llama/Llama-2-7b-chat-hf".to_string(), // 其他
            ]
        };
        
        // 尝试不同的模型进行测试
        for model in test_models {
            let request_body = json!({
                "model": model,
                "messages": [
                    {
                        "role": "user",
                        "content": "Hi"
                    }
                ],
                "max_tokens": 5,
                "temperature": 0.1
            });

            println!("尝试模型: {}", model);
            
            // 使用tokio的timeout包装整个请求
            let timeout_duration = std::time::Duration::from_secs(15);
            let mut request_builder = self.client
                .post(&format!("{}/chat/completions", normalized_url))
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .header("Accept", "application/json, text/plain, */*")
                .header("Accept-Encoding", "identity")  // 禁用压缩，避免二进制响应
                .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .header("Connection", "keep-alive")
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36");

            if let Ok(parsed_url) = Url::parse(&normalized_url) { // Use normalized_url here
                if (parsed_url.scheme() == "http" || parsed_url.scheme() == "https") && parsed_url.host_str().is_some() {
                    let origin_val = format!("{}://{}", parsed_url.scheme(), parsed_url.host_str().unwrap_or_default());
                    let referer_val = format!("{}://{}/", parsed_url.scheme(), parsed_url.host_str().unwrap_or_default());
                    request_builder = request_builder
                        .header("Origin", origin_val)
                        .header("Referer", referer_val);
                }
            }
            
            let request_future = request_builder
                .json(&request_body)
                .send();

            // 使用tokio::time::timeout
            match tokio::time::timeout(timeout_duration, request_future).await {
                Ok(Ok(response)) => {
                    let status = response.status();
                    println!("API连接测试响应状态: {} (模型: {})", status, model);
                    
                    if status.is_success() {
                        println!("API连接测试成功！使用模型: {}", model);
                        return Ok(true);
                    } else if status == 400 {
                        // 400错误可能是模型不支持，尝试下一个
                        let error_text = response.text().await.unwrap_or_default();
                        println!("模型 {} 不支持，错误: {}", model, error_text);
                        // 如果是用户指定的模型，直接返回失败
                        if model_name.is_some() {
                            return Ok(false);
                        }
                        continue;
                    } else if status == 401 {
                        // 401是认证错误，不需要尝试其他模型
                        println!("API密钥认证失败: {}", status);
                        return Ok(false);
                    } else {
                        // 其他错误
                        let error_text = response.text().await.unwrap_or_default();
                        println!("API请求失败: {} - {}", status, error_text);
                        // 如果是用户指定的模型，直接返回失败
                        if model_name.is_some() {
                            return Ok(false);
                        }
                        continue;
                    }
                },
                Ok(Err(e)) => {
                    println!("API连接测试请求错误 (模型: {}): {}", model, e);
                    // 如果是连接错误，不需要尝试其他模型
                    if e.to_string().contains("handshake") || e.to_string().contains("connect") {
                        return Err(AppError::network(format!("连接失败: {}", e)));
                    }
                    // 如果是用户指定的模型，直接返回失败
                    if model_name.is_some() {
                        return Err(AppError::network(format!("请求失败: {}", e)));
                    }
                    continue;
                },
                Err(_) => {
                    println!("API连接测试超时 (模型: {})", model);
                    // 如果是用户指定的模型，直接返回失败
                    if model_name.is_some() {
                        return Err(AppError::network("请求超时"));
                    }
                    continue;
                }
            }
        }
        
        println!("所有测试模型都失败了");
        Ok(false)
    }
}

// 获取科目配置的Prompt模板（从数据库读取）
impl LLMManager {
    pub fn get_subject_prompt(&self, subject: &str, task_type: &str) -> String {
        // 尝试从数据库获取科目配置
        match self.db.get_subject_config_by_name(subject) {
            Ok(Some(config)) => {
                let base_prompt = match task_type {
                    "model1" | "ocr" | "classification" => {
                        // OCR和分类任务使用OCR提示词+分类提示词
                        let mut prompt = config.prompts.ocr_prompt.clone();
                        if !config.prompts.classification_prompt.is_empty() {
                            prompt.push_str("\n\n");
                            prompt.push_str(&config.prompts.classification_prompt);
                        }
                        // 添加JSON格式要求
                        prompt.push_str("\n\n请以JSON格式返回结果：{\"ocr_text\": \"题目文字\", \"tags\": [\"标签1\", \"标签2\"], \"mistake_type\": \"题目类型\"}");
                        prompt
                    },
                    "model2" | "analysis" => {
                        config.prompts.analysis_prompt.clone()
                    },
                    "review" => {
                        config.prompts.review_prompt.clone()
                    },
                    "chat" => {
                        config.prompts.chat_prompt.clone()
                    },
                    "consolidated_review" | "consolidated_review_chat" => {
                        config.prompts.consolidated_review_prompt.clone()
                    },
                    "anki_generation" => {
                        config.prompts.anki_generation_prompt.clone()
                    },
                    _ => {
                        config.prompts.analysis_prompt.clone() // 默认为分析
                    }
                };

                // 替换占位符
                base_prompt.replace("{subject}", subject)
            },
            _ => {
                // 如果无法从数据库获取配置，使用默认提示词
                self.get_fallback_prompt(subject, task_type)
            }
        }
    }

    // 备用提示词（当数据库配置不可用时使用）
    fn get_fallback_prompt(&self, subject: &str, task_type: &str) -> String {
        match task_type {
            "model1" | "ocr" | "classification" => {
                format!("你是一个{}题目分析专家。请识别图片中的{}题目文字内容，并分析题目类型和相关知识点标签。\n\n【重要】OCR文本提取要求：\n1. 提取纯文本内容，不要使用LaTeX格式\n2. 数学公式用普通文字描述\n3. 保持文本简洁易读\n4. 避免使用特殊LaTeX命令\n\n请以JSON格式返回结果：{{\"ocr_text\": \"题目文字\", \"tags\": [\"标签1\", \"标签2\"], \"mistake_type\": \"题目类型\"}}", subject, subject)
            },
            "model2" | "analysis" => {
                if subject == "数学" || subject == "物理" || subject == "化学" || subject == "生物" {
                    format!("你是一个{}教学专家。请仔细分析这道{}错题，提供详细的解题思路和知识点讲解。\n\n【重要】公式格式要求：\n1. 行内公式请使用 $\\text{{公式}}$ 格式\n2. 独立公式请使用 $$\\text{{公式}}$$ 格式\n3. 分数请使用 \\frac{{分子}}{{分母}} 格式\n4. 积分请使用 \\int 格式\n5. 求和请使用 \\sum 格式\n6. 根号请使用 \\sqrt{{}} 格式\n7. 幂次请使用 ^ 符号，如 x^2\n8. 下标请使用 _ 符号，如 x_1\n9. 希腊字母请使用对应的LaTeX命令，如 \\alpha, \\beta\n10. 请确保所有数学表达式都严格按照LaTeX格式书写，避免使用纯文本表示数学公式", subject, subject)
                } else {
                    format!("你是一个{}教学专家。请仔细分析这道{}错题，提供详细的解题思路和知识点讲解。", subject, subject)
                }
            },
            "review" => {
                format!("你是一个{}学习分析专家。请分析这些{}错题的共同问题和改进建议。", subject, subject)
            },
            "consolidated_review" => {
                format!("你是一个{}学习分析专家。请对提供的{}错题进行综合复习分析，包括知识点总结、常见错误模式识别和学习建议。", subject, subject)
            },
            "chat" => {
                if subject == "数学" || subject == "物理" || subject == "化学" || subject == "生物" {
                    format!("基于这道{}题目，请回答学生的问题。\n\n【重要】公式格式要求：\n1. 行内公式请使用 $公式$ 格式\n2. 独立公式请使用 $$公式$$ 格式\n3. 分数请使用 \\frac{{分子}}{{分母}} 格式\n4. 积分请使用 \\int 格式\n5. 求和请使用 \\sum 格式\n6. 根号请使用 \\sqrt{{}} 格式\n7. 请确保所有数学表达式都严格按照LaTeX格式书写", subject)
                } else {
                    format!("基于这道{}题目，请回答学生的问题。", subject)
                }
            },
            "anki_generation" => {
                format!("请根据以下{}科目的学习内容，生成适合制作Anki卡片的问题和答案对。每张卡片应测试一个单一的概念。请以JSON数组格式返回结果，每个对象必须包含 \"front\" (字符串), \"back\" (字符串), \"tags\" (字符串数组) 三个字段。", subject)
            },
            _ => {
                format!("请根据提供的{}题目信息，详细解答问题。", subject)
            }
        }
    }

    /// 生成ANKI卡片 - 核心功能
    pub async fn generate_anki_cards_from_document(
        &self,
        document_content: &str,
        subject_name: &str,
        options: Option<&crate::models::AnkiGenerationOptions>,
    ) -> Result<Vec<crate::models::AnkiCard>> {
        println!("开始生成ANKI卡片: 科目={}, 文档长度={}", subject_name, document_content.len());
        
        // 1. 获取ANKI制卡模型配置
        let config = self.get_anki_model_config().await?;
        
        // 2. 获取科目特定的ANKI制卡Prompt
        let subject_prompt = self.get_subject_prompt(subject_name, "anki_generation");
        
        // 3. 构建最终的AI指令
        let final_prompt = format!("{}\n\n文档内容：\n{}", subject_prompt, document_content);
        
        // 4. 准备AI模型请求
        let max_tokens = options.as_ref()
            .and_then(|opt| opt.max_tokens)
            .unwrap_or(config.max_output_tokens); // 使用配置中的max_output_tokens
        let temperature = options.as_ref()
            .and_then(|opt| opt.temperature)
            .unwrap_or(0.3);
            
        let mut request_body = json!({
            "model": config.model,
            "messages": [
                {
                    "role": "user",
                    "content": final_prompt
                }
            ],
            "max_tokens": max_tokens,
            "temperature": temperature
        });

        // 如果支持JSON模式，添加response_format
        if config.model.starts_with("gpt-") {
            request_body["response_format"] = json!({"type": "json_object"});
        }
        
        println!("发送ANKI制卡请求到: {}/chat/completions", config.base_url);
        
        // 5. 发送HTTP请求至AI模型
        let mut request_builder = self.client
            .post(&format!("{}/chat/completions", config.base_url))
            .header("Authorization", format!("Bearer {}", config.api_key))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/plain, */*")
            .header("Accept-Encoding", "identity")  // 禁用压缩，避免二进制响应
            .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .header("Connection", "keep-alive")
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36");

        if let Ok(parsed_url) = Url::parse(&config.base_url) {
            if (parsed_url.scheme() == "http" || parsed_url.scheme() == "https") && parsed_url.host_str().is_some() {
                let origin_val = format!("{}://{}", parsed_url.scheme(), parsed_url.host_str().unwrap_or_default());
                let referer_val = format!("{}://{}/", parsed_url.scheme(), parsed_url.host_str().unwrap_or_default());
                 request_builder = request_builder
                    .header("Origin", origin_val)
                    .header("Referer", referer_val);
            }
        }
        
        let response = request_builder
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                let error_msg = if e.to_string().contains("timed out") {
                    format!("ANKI制卡API请求超时: {}", e)
                } else if e.to_string().contains("connect") {
                    format!("无法连接到ANKI制卡API服务器: {}", e)
                } else {
                    format!("ANKI制卡API请求失败: {}", e)
                };
                AppError::network(error_msg)
            })?;

        // 6. 处理HTTP响应
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(AppError::llm(format!("ANKI制卡API请求失败: {} - {}", status, error_text)));
        }

        let response_json: Value = response.json().await
            .map_err(|e| AppError::llm(format!("解析ANKI制卡响应失败: {}", e)))?;
        
        // 7. 提取AI生成的内容
        let content_str = response_json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| AppError::llm("无法解析ANKI制卡API响应"))?;

        println!("ANKI制卡原始响应: {}", content_str);
        
        // 8. 清理和解析AI返回的JSON数据
        let cleaned_content = self.clean_anki_json_response(content_str)?;
        println!("清理后的JSON: {}", cleaned_content);
        
        // 9. 反序列化为AnkiCard向量（带容错处理）
        let cards: Vec<crate::models::AnkiCard> = self.parse_anki_cards_with_fallback(&cleaned_content, content_str)?;
        
        println!("成功生成 {} 张ANKI卡片", cards.len());
        Ok(cards)
    }

    /// 清理AI返回的ANKI卡片JSON响应
    fn clean_anki_json_response(&self, content: &str) -> Result<String> {
        let mut cleaned = content.trim().to_string();
        
        // 移除markdown代码块
        cleaned = regex::Regex::new(r"```(?:json)?\s*")
            .unwrap()
            .replace_all(&cleaned, "")
            .to_string();
        cleaned = regex::Regex::new(r"```\s*$")
            .unwrap()
            .replace_all(&cleaned, "")
            .to_string();
        
        // 移除常见前缀
        let prefixes = [
            "以下是生成的Anki卡片：", "Anki卡片：", "JSON结果：", "卡片数据：",
            "Here are the Anki cards:", "Cards:", "JSON:", "Result:",
        ];
        
        for prefix in &prefixes {
            if cleaned.starts_with(prefix) {
                cleaned = cleaned.strip_prefix(prefix).unwrap_or(&cleaned).trim().to_string();
                break;
            }
        }
        
        // 确保是有效的JSON数组格式
        if !cleaned.starts_with('[') {
            // 尝试找到第一个'['
            if let Some(start) = cleaned.find('[') {
                cleaned = cleaned[start..].to_string();
            } else {
                return Err(AppError::llm("无法找到JSON数组开始标记"));
            }
        }
        
        if !cleaned.ends_with(']') {
            // 尝试找到最后一个']'
            if let Some(end) = cleaned.rfind(']') {
                cleaned = cleaned[..=end].to_string();
            } else {
                return Err(AppError::llm("无法找到JSON数组结束标记"));
            }
        }
        
        Ok(cleaned)
    }

    /// 解析ANKI卡片JSON，带容错处理（自动补充缺失的images字段和兼容question字段）
    fn parse_anki_cards_with_fallback(&self, json_str: &str, original_content: &str) -> Result<Vec<crate::models::AnkiCard>> {
        // 尝试将JSON字符串解析为通用的Value数组
        let mut card_values: Vec<Value> = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(e) => {
                // 如果连基本JSON都解析不了，直接返回错误
                return Err(AppError::llm(format!("解析ANKI卡片JSON失败: {} - 原始内容: {}", e, original_content)));
            }
        };

        // 遍历每个卡片对象，进行字段兼容性处理
        for card_value in &mut card_values {
            if let Some(obj) = card_value.as_object_mut() {
                // 兼容 "question" 字段 -> "front"
                if obj.contains_key("question") && !obj.contains_key("front") {
                    if let Some(question_val) = obj.remove("question") {
                        obj.insert("front".to_string(), question_val);
                    }
                }
                // 自动补充缺失的 "images" 字段
                if !obj.contains_key("images") {
                    obj.insert("images".to_string(), json!([]));
                }
            }
        }

        // 将处理过的Value转换回JSON字符串
        let processed_json_str = match serde_json::to_string(&card_values) {
            Ok(s) => s,
            Err(e) => return Err(AppError::llm(format!("重新序列化卡片数据失败: {}", e))),
        };

        // 使用处理过的JSON字符串进行最终的反序列化
        match serde_json::from_str::<Vec<crate::models::AnkiCard>>(&processed_json_str) {
            Ok(cards) => Ok(cards),
            Err(e) => {
                // 如果仍然失败，说明有其他结构问题
                Err(AppError::llm(format!("最终解析ANKI卡片失败: {} - 处理后JSON: {}", e, processed_json_str)))
            }
        }
    }
}

// 提取推理模型的思维链段落
/// 改进的思维链内容提取方法，提供多种策略以提高可靠性
fn extract_reasoning_sections(content: &str) -> Vec<serde_json::Value> {
    // 策略1: 尝试标准化的思维链格式提取
    if let Some(sections) = extract_standard_cot_format(content) {
        return sections;
    }
    
    // 策略2: 尝试数字列表格式提取
    if let Some(sections) = extract_numbered_list_format(content) {
        return sections;
    }
    
    // 策略3: 尝试关键词段落格式提取
    if let Some(sections) = extract_keyword_sections(content) {
        return sections;
    }
    
    // 策略4: 尝试markdown格式提取
    if let Some(sections) = extract_markdown_sections(content) {
        return sections;
    }
    
    // 策略5: 回退到语义分割
    extract_semantic_sections(content)
}

/// 策略1: 提取标准化的思维链格式（如 "## 步骤1:", "### 分析:", 等）
fn extract_standard_cot_format(content: &str) -> Option<Vec<serde_json::Value>> {
    use regex::Regex;
    
    // 匹配标准思维链格式的标题
    let cot_patterns = [
        r"(?i)^#{1,4}\s*(步骤\s*\d+|问题理解|知识点分析|解题思路|具体步骤|结论总结)[:：]?\s*(.*)$",
        r"(?i)^(\d+\.\s*(?:问题理解|知识点分析|解题思路|具体步骤|结论总结))[:：]?\s*(.*)$",
        r"(?i)^(思考过程\s*\d*|分析\s*\d*|推理\s*\d*)[:：]\s*(.*)$"
    ];
    
    for pattern in &cot_patterns {
        if let Ok(re) = Regex::new(pattern) {
            if re.is_match(content) {
                return Some(extract_sections_by_regex(content, &re));
            }
        }
    }
    
    None
}

/// 策略2: 提取数字列表格式（如 "1. 分析", "2. 推理"）
fn extract_numbered_list_format(content: &str) -> Option<Vec<serde_json::Value>> {
    use regex::Regex;
    
    if let Ok(re) = Regex::new(r"(?m)^(\d+\.\s+.+?)(?=^\d+\.\s|\z)") {
        let sections: Vec<_> = re.captures_iter(content)
            .enumerate()
            .map(|(i, cap)| {
                let full_match = cap.get(0).unwrap().as_str();
                let lines: Vec<&str> = full_match.lines().collect();
                let title = lines.first().unwrap_or(&"").trim();
                let content_lines = &lines[1..];
                
                json!({
                    "title": title,
                    "content": content_lines.join("\n").trim(),
                    "section_index": i,
                    "extraction_method": "numbered_list"
                })
            })
            .collect();
        
        if !sections.is_empty() {
            return Some(sections);
        }
    }
    
    None
}

/// 策略3: 提取关键词段落格式
fn extract_keyword_sections(content: &str) -> Option<Vec<serde_json::Value>> {
    let mut sections = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut current_section: Option<(String, Vec<String>)> = None;
    
    // 增强的关键词列表
    let section_keywords = [
        "思考过程", "分析过程", "推理过程", "解题思路", "问题理解", 
        "知识点分析", "具体步骤", "结论总结", "答案推导", "计算过程",
        "观察", "假设", "验证", "解法", "方法", "策略", "思维链",
        "第一步", "第二步", "第三步", "最后", "因此", "所以", "综上"
    ];
    
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        
        // 检查是否是段落标题 - 更严格的匹配
        let is_section_title = section_keywords.iter().any(|&keyword| {
            trimmed.starts_with(keyword) || 
            (trimmed.contains(keyword) && (trimmed.ends_with("：") || trimmed.ends_with(":"))) ||
            (trimmed.len() < 50 && trimmed.contains(keyword) && 
             (trimmed.contains("分析") || trimmed.contains("思考") || trimmed.contains("步骤")))
        });
        
        if is_section_title {
            // 保存上一个段落
            if let Some((title, content_lines)) = current_section.take() {
                if !content_lines.is_empty() {
                    sections.push(json!({
                        "title": title,
                        "content": content_lines.join("\n"),
                        "extraction_method": "keyword_sections"
                    }));
                }
            }
            
            // 开始新段落
            let title = trimmed.trim_end_matches(['：', ':']).to_string();
            current_section = Some((title, Vec::new()));
        } else if let Some((_, ref mut content_lines)) = current_section {
            // 添加到当前段落内容
            content_lines.push(trimmed.to_string());
        }
    }
    
    // 保存最后一个段落
    if let Some((title, content_lines)) = current_section {
        if !content_lines.is_empty() {
            sections.push(json!({
                "title": title,
                "content": content_lines.join("\n"),
                "extraction_method": "keyword_sections"
            }));
        }
    }
    
    if !sections.is_empty() {
        Some(sections)
    } else {
        None
    }
}

/// 策略4: 提取markdown格式
fn extract_markdown_sections(content: &str) -> Option<Vec<serde_json::Value>> {
    use regex::Regex;
    
    if let Ok(re) = Regex::new(r"(?m)^(#{1,6}\s+.+?)$((?:(?!^#{1,6}\s).)*?)") {
        let sections: Vec<_> = re.captures_iter(content)
            .enumerate()
            .map(|(i, cap)| {
                let title = cap.get(1).unwrap().as_str().trim();
                let section_content = cap.get(2).unwrap().as_str().trim();
                
                json!({
                    "title": title.trim_start_matches('#').trim(),
                    "content": section_content,
                    "section_index": i,
                    "extraction_method": "markdown"
                })
            })
            .collect();
        
        if !sections.is_empty() {
            return Some(sections);
        }
    }
    
    None
}

/// 策略5: 语义分割回退方案
fn extract_semantic_sections(content: &str) -> Vec<serde_json::Value> {
    let trimmed_content = content.trim();
    
    if trimmed_content.is_empty() {
        return vec![];
    }
    
    // 尝试按句号或换行符分割
    let sentences: Vec<&str> = trimmed_content
        .split(|c| c == '。' || c == '.' || c == '\n')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && s.len() > 10) // 过滤太短的内容
        .collect();
    
    if sentences.len() > 1 {
        // 如果能分割出多个句子，按句子分组
        let sections: Vec<_> = sentences
            .chunks(2) // 每两个句子一组
            .enumerate()
            .map(|(i, chunk)| {
                json!({
                    "title": format!("思维片段 {}", i + 1),
                    "content": chunk.join("。"),
                    "section_index": i,
                    "extraction_method": "semantic_fallback"
                })
            })
            .collect();
        sections
    } else {
        // 无法分割，返回整个内容
        vec![json!({
            "title": "完整推理内容",
            "content": trimmed_content,
            "section_index": 0,
            "extraction_method": "full_content_fallback"
        })]
    }
}

/// 通用的正则表达式段落提取器
fn extract_sections_by_regex(content: &str, re: &regex::Regex) -> Vec<serde_json::Value> {
    let sections: Vec<_> = re.captures_iter(content)
        .enumerate()
        .map(|(i, cap)| {
            let title = cap.get(1).map(|m| m.as_str()).unwrap_or("未知段落");
            let section_content = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            
            json!({
                "title": title.trim(),
                "content": section_content.trim(),
                "section_index": i,
                "extraction_method": "regex"
            })
        })
        .collect();
    
    sections
}

// 清理JSON响应内容
fn clean_json_response(content: &str) -> String {
    // 移除常见的包装文本
    let mut cleaned = content.trim();
    
    // 移除markdown代码块标记
    if cleaned.starts_with("```json") {
        cleaned = cleaned.strip_prefix("```json").unwrap_or(cleaned).trim();
    }
    if cleaned.starts_with("```") {
        cleaned = cleaned.strip_prefix("```").unwrap_or(cleaned).trim();
    }
    if cleaned.ends_with("```") {
        cleaned = cleaned.strip_suffix("```").unwrap_or(cleaned).trim();
    }
    
    // 移除常见的前缀文本
    let prefixes_to_remove = [
        "以下是JSON格式的结果：",
        "JSON结果：",
        "结果：",
        "答案：",
        "这是分析结果：",
    ];
    
    for prefix in &prefixes_to_remove {
        if cleaned.starts_with(prefix) {
            cleaned = cleaned.strip_prefix(prefix).unwrap_or(cleaned).trim();
            break;
        }
    }
    
    cleaned.to_string()
}

// 从文本中提取JSON
fn extract_json_from_text(text: &str) -> Option<String> {
    // 方法1：查找第一个{到最后一个}
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            if end > start {
                let json_candidate = &text[start..=end];
                // 验证这是一个合理的JSON结构
                if json_candidate.contains("ocr_text") || json_candidate.contains("tags") || json_candidate.contains("mistake_type") {
                    return Some(json_candidate.to_string());
                }
            }
        }
    }
    
    // 方法2：查找包含所需字段的JSON对象
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.trim().starts_with('{') {
            // 从这一行开始，找到匹配的}
            let mut brace_count = 0;
            let mut json_lines = Vec::new();
            
            for j in i..lines.len() {
                let current_line = lines[j];
                json_lines.push(current_line);
                
                for ch in current_line.chars() {
                    match ch {
                        '{' => brace_count += 1,
                        '}' => brace_count -= 1,
                        _ => {}
                    }
                }
                
                if brace_count == 0 {
                    let json_candidate = json_lines.join("\n");
                    if json_candidate.contains("ocr_text") || json_candidate.contains("tags") || json_candidate.contains("mistake_type") {
                        return Some(json_candidate);
                    }
                    break;
                }
            }
        }
    }
    
    None
}

// 修复常见的JSON错误
fn fix_common_json_errors(json_str: &str) -> String {
    let mut fixed = json_str.to_string();
    
    // 修复常见的转义问题
    // 1. 修复单引号为双引号（但要小心不要破坏字符串内容）
    // 这个比较复杂，暂时跳过
    
    // 2. 修复未转义的引号
    // 在字符串值中查找未转义的引号并转义它们
    // 这需要更复杂的解析，暂时使用简单的替换
    
    // 3. 修复尾随逗号
    fixed = fixed.replace(",}", "}");
    fixed = fixed.replace(",]", "]");
    
    // 4. 修复多余的空白字符
    fixed = fixed.replace("\n", " ");
    fixed = fixed.replace("\r", " ");
    
    // 5. 修复常见的字段名问题
    fixed = fixed.replace("'ocr_text'", "\"ocr_text\"");
    fixed = fixed.replace("'tags'", "\"tags\"");
    fixed = fixed.replace("'mistake_type'", "\"mistake_type\"");
    
    // 6. 确保字符串值被正确引用
    // 这需要更复杂的逻辑，暂时跳过
    
    fixed
}

/// 强化的模型一JSON响应解析函数
/// 使用多层次解析策略，提高稳定性和成功率
fn parse_model1_json_response(content: &str) -> Result<Value> {
    println!("开始强化JSON解析，内容长度: {} 字符", content.len());
    
    // 第一层：直接解析（针对格式良好的响应）
    if let Ok(json_value) = serde_json::from_str::<Value>(content.trim()) {
        if validate_model1_json(&json_value) {
            println!("✅ 第一层解析成功：直接解析");
            return Ok(json_value);
        } else {
            println!("⚠️ 第一层解析成功但数据验证失败");
        }
    }
    
    // 第二层：预处理后解析（清理常见包装和格式问题）
    let cleaned_content = enhanced_clean_json_response(content);
    println!("第二层：清理后内容: {}", cleaned_content);
    
    if let Ok(json_value) = serde_json::from_str::<Value>(&cleaned_content) {
        if validate_model1_json(&json_value) {
            println!("✅ 第二层解析成功：预处理后解析");
            return Ok(json_value);
        }
    }
    
    // 第三层：智能提取（从文本中提取JSON结构）
    if let Some(extracted_json) = smart_extract_json_from_text(content) {
        println!("第三层：提取的JSON: {}", extracted_json);
        
        if let Ok(json_value) = serde_json::from_str::<Value>(&extracted_json) {
            if validate_model1_json(&json_value) {
                println!("✅ 第三层解析成功：智能提取");
                return Ok(json_value);
            }
        }
    }
    
    // 第四层：模式匹配重构（从非结构化内容中重构JSON）
    if let Some(reconstructed_json) = reconstruct_json_from_content(content) {
        println!("第四层：重构的JSON: {}", reconstructed_json);
        
        if let Ok(json_value) = serde_json::from_str::<Value>(&reconstructed_json) {
            if validate_model1_json(&json_value) {
                println!("✅ 第四层解析成功：模式匹配重构");
                return Ok(json_value);
            }
        }
    }
    
    // 第五层：降级处理（创建带默认值的最小可用JSON）
    let fallback_json = create_fallback_json(content);
    println!("第五层：降级处理JSON: {}", fallback_json);
    
    if let Ok(json_value) = serde_json::from_str::<Value>(&fallback_json) {
        println!("⚠️ 使用降级处理结果");
        return Ok(json_value);
    }
    
    // 所有解析策略都失败
    Err(AppError::llm(format!(
        "所有JSON解析策略都失败。原始内容: {}",
        content
    )))
}

/// 验证模型一JSON响应的数据结构
fn validate_model1_json(json: &Value) -> bool {
    // 检查必需字段存在且类型正确
    let has_ocr_text = json.get("ocr_text").and_then(|v| v.as_str()).is_some();
    let has_tags = json.get("tags").and_then(|v| v.as_array()).is_some();
    let has_mistake_type = json.get("mistake_type").and_then(|v| v.as_str()).is_some();
    
    let is_valid = has_ocr_text && has_tags && has_mistake_type;
    
    if !is_valid {
        println!("JSON验证失败: ocr_text={}, tags={}, mistake_type={}", 
                 has_ocr_text, has_tags, has_mistake_type);
    }
    
    is_valid
}

/// 增强的JSON清理函数
fn enhanced_clean_json_response(content: &str) -> String {
    let mut cleaned = content.trim().to_string();
    
    // 移除markdown代码块
    cleaned = regex::Regex::new(r"```(?:json)?\s*").unwrap().replace_all(&cleaned, "").to_string();
    cleaned = regex::Regex::new(r"```\s*$").unwrap().replace_all(&cleaned, "").to_string();
    
    // 移除常见前缀和后缀
    let prefixes = [
        "以下是JSON格式的结果：", "JSON结果：", "结果：", "答案：", "分析结果：",
        "Here is the JSON:", "JSON:", "Result:", "Output:", "Analysis:",
        "根据分析，JSON格式结果如下：", "JSON格式输出：",
    ];
    
    for prefix in &prefixes {
        if cleaned.starts_with(prefix) {
            cleaned = cleaned.strip_prefix(prefix).unwrap_or(&cleaned).trim().to_string();
        }
    }
    
    // 移除常见后缀
    let suffixes = [
        "以上就是分析结果。", "分析完成。", "希望对您有帮助。",
        "That's the analysis.", "Analysis complete.",
    ];
    
    for suffix in &suffixes {
        if cleaned.ends_with(suffix) {
            cleaned = cleaned.strip_suffix(suffix).unwrap_or(&cleaned).trim().to_string();
        }
    }
    
    cleaned
}

/// 智能JSON提取函数
fn smart_extract_json_from_text(text: &str) -> Option<String> {
    // 使用正则表达式查找JSON对象
    let json_pattern = regex::Regex::new(r#"\{[^{}]*"ocr_text"[^{}]*"tags"[^{}]*"mistake_type"[^{}]*\}"#).unwrap();
    
    if let Some(captures) = json_pattern.find(text) {
        return Some(captures.as_str().to_string());
    }
    
    // 备用方法：查找花括号包围的内容
    let mut brace_depth = 0;
    let mut start_pos = None;
    let mut end_pos = None;
    
    for (i, ch) in text.char_indices() {
        match ch {
            '{' => {
                if brace_depth == 0 {
                    start_pos = Some(i);
                }
                brace_depth += 1;
            }
            '}' => {
                brace_depth -= 1;
                if brace_depth == 0 && start_pos.is_some() {
                    end_pos = Some(i + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    
    if let (Some(start), Some(end)) = (start_pos, end_pos) {
        let json_candidate = &text[start..end];
        // 验证是否包含必需字段
        if json_candidate.contains("ocr_text") && 
           json_candidate.contains("tags") && 
           json_candidate.contains("mistake_type") {
            return Some(json_candidate.to_string());
        }
    }
    
    None
}

/// 从内容中重构JSON（当结构化JSON无法提取时）
fn reconstruct_json_from_content(content: &str) -> Option<String> {
    // 使用正则表达式提取各个字段的值
    let ocr_pattern = regex::Regex::new(r#"(?i)(?:ocr_text|题目内容|文字内容|题目文字)[":\s]*["']?([^"'\n\r}]+)["']?"#).unwrap();
    let tags_pattern = regex::Regex::new(r#"(?i)(?:tags|标签|知识点)[":\s]*\[([^\]]+)\]"#).unwrap();
    let type_pattern = regex::Regex::new(r#"(?i)(?:mistake_type|题目类型|类型)[":\s]*["']?([^"'\n\r}]+)["']?"#).unwrap();
    
    let mut ocr_text = "";
    let mut tags_text = "";
    let mut mistake_type = "";
    
    // 提取OCR文本
    if let Some(captures) = ocr_pattern.captures(content) {
        ocr_text = captures.get(1).map(|m| m.as_str().trim()).unwrap_or("");
    }
    
    // 提取标签
    if let Some(captures) = tags_pattern.captures(content) {
        tags_text = captures.get(1).map(|m| m.as_str().trim()).unwrap_or("");
    }
    
    // 提取错误类型
    if let Some(captures) = type_pattern.captures(content) {
        mistake_type = captures.get(1).map(|m| m.as_str().trim()).unwrap_or("");
    }
    
    // 如果至少提取到一些内容，构建JSON
    if !ocr_text.is_empty() || !tags_text.is_empty() || !mistake_type.is_empty() {
        // 处理标签字符串
        let tags_array = if tags_text.is_empty() {
            "[]".to_string()
        } else {
            let tags: Vec<String> = tags_text
                .split(',')
                .map(|tag| format!("\"{}\"", tag.trim().trim_matches('"').trim_matches('\'')))
                .collect();
            format!("[{}]", tags.join(", "))
        };
        
        let json = format!(
            r#"{{"ocr_text": "{}", "tags": {}, "mistake_type": "{}"}}"#,
            ocr_text.replace('"', "\\\""),
            tags_array,
            mistake_type.replace('"', "\\\"")
        );
        
        return Some(json);
    }
    
    None
}

/// 创建降级JSON（最后的兜底方案）
fn create_fallback_json(content: &str) -> String {
    println!("📋 创建降级JSON，原始内容: '{}'", content);
    
    // 特殊处理空响应或只有符号的响应
    if content.trim().is_empty() || content.trim() == "{}" || content.trim() == "[]" {
        println!("⚠️ 检测到空响应，生成默认内容");
        return format!(
            r#"{{"ocr_text": "模型响应为空，无法识别题目内容", "tags": ["API响应异常", "需要人工处理"], "mistake_type": "系统错误"}}"#
        );
    }
    
    // 尝试从内容中提取一些有用信息作为OCR文本
    let mut ocr_content = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter(|line| !line.contains("JSON") && !line.contains("格式"))
        .take(3)
        .collect::<Vec<_>>()
        .join(" ");
    
    // 清理内容
    ocr_content = ocr_content.replace('"', "\\\"");
    if ocr_content.chars().count() > 200 {
        ocr_content = format!("{}...", ocr_content.chars().take(200).collect::<String>());
    }
    
    if ocr_content.is_empty() || ocr_content == "{}" {
        ocr_content = "无法识别题目内容，模型响应异常".to_string();
    }
    
    format!(
        r#"{{"ocr_text": "{}", "tags": ["需要人工标注"], "mistake_type": "未分类"}}"#,
        ocr_content
    )
}

// ==================== RAG相关扩展方法 ====================

impl LLMManager {
    /// 获取嵌入模型配置
    pub async fn get_embedding_model_config(&self) -> Result<ApiConfig> {
        let assignments = self.get_model_assignments().await?;
        let embedding_model_id = assignments.embedding_model_config_id
            .ok_or_else(|| AppError::configuration("未配置嵌入模型"))?;
        
        let configs = self.get_api_configs().await?;
        configs.into_iter()
            .find(|config| config.id == embedding_model_id)
            .ok_or_else(|| AppError::configuration("找不到嵌入模型配置"))
    }
    
    /// 获取重排序模型配置
    pub async fn get_reranker_model_config(&self) -> Result<ApiConfig> {
        let assignments = self.get_model_assignments().await?;
        let reranker_model_id = assignments.reranker_model_config_id
            .ok_or_else(|| AppError::configuration("未配置重排序模型"))?;
        
        let configs = self.get_api_configs().await?;
        configs.into_iter()
            .find(|config| config.id == reranker_model_id)
            .ok_or_else(|| AppError::configuration("找不到重排序模型配置"))
    }
    
    /// 调用嵌入API生成向量
    pub async fn call_embedding_api(&self, texts: Vec<String>, model_config_id: &str) -> Result<Vec<Vec<f32>>> {
        println!("🧠 调用嵌入API，文本数量: {}", texts.len());
        
        // 获取API配置
        let configs = self.get_api_configs().await?;
        let config = configs.iter()
            .find(|c| c.id == model_config_id)
            .ok_or_else(|| AppError::configuration("找不到嵌入模型配置"))?;
        
        // 解密API密钥
        let api_key = self.decrypt_api_key_if_needed(&config.api_key)?;
        
        // 构造请求
        let request_body = match config.model_adapter.as_str() {
            "openai" | "general" => {
                json!({
                    "model": config.model,
                    "input": texts,
                    "encoding_format": "float"
                })
            }
            "claude" => {
                // Claude目前不直接支持嵌入，这里返回错误
                return Err(AppError::configuration("Claude模型不支持嵌入API"));
            }
            "deepseek" => {
                json!({
                    "model": config.model,
                    "input": texts,
                    "encoding_format": "float"
                })
            }
            _ => {
                // 默认使用OpenAI格式
                json!({
                    "model": config.model,
                    "input": texts,
                    "encoding_format": "float"
                })
            }
        };
        
        // 发送请求
        let url = format!("{}/embeddings", config.base_url.trim_end_matches('/'));
        let mut request_builder = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/plain, */*")
            .header("Accept-Encoding", "identity")  // 禁用压缩，避免二进制响应
            .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .header("Connection", "keep-alive")
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36");

        if let Ok(parsed_url) = Url::parse(&config.base_url) { // config is the specific model config here
            if (parsed_url.scheme() == "http" || parsed_url.scheme() == "https") && parsed_url.host_str().is_some() {
                let origin_val = format!("{}://{}", parsed_url.scheme(), parsed_url.host_str().unwrap_or_default());
                let referer_val = format!("{}://{}/", parsed_url.scheme(), parsed_url.host_str().unwrap_or_default());
                 request_builder = request_builder
                    .header("Origin", origin_val)
                    .header("Referer", referer_val);
            }
        }
        
        let response = request_builder
            .json(&request_body)
            .send()
            .await
            .map_err(|e| AppError::network(format!("嵌入API请求失败: {}", e)))?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(AppError::llm(format!("嵌入API返回错误 {}: {}", status, error_text)));
        }
        
        let response_json: Value = response.json().await
            .map_err(|e| AppError::llm(format!("解析嵌入API响应失败: {}", e)))?;
        
        // 解析嵌入向量
        let data = response_json["data"].as_array()
            .ok_or_else(|| AppError::llm("嵌入API响应格式无效：缺少data字段"))?;
        
        let mut embeddings = Vec::new();
        for item in data {
            let embedding = item["embedding"].as_array()
                .ok_or_else(|| AppError::llm("嵌入API响应格式无效：缺少embedding字段"))?;
            
            let vector: Result<Vec<f32>> = embedding.iter()
                .map(|v| v.as_f64()
                    .map(|f| f as f32)
                    .ok_or_else(|| AppError::llm("嵌入向量包含无效数值")))
                .collect();
            
            embeddings.push(vector?);
        }
        
        if embeddings.len() != texts.len() {
            return Err(AppError::llm("嵌入向量数量与输入文本数量不匹配"));
        }
        
        println!("✅ 嵌入API调用成功，返回 {} 个向量", embeddings.len());
        Ok(embeddings)
    }
    
    /// 调用重排序API
    pub async fn call_reranker_api(
        &self, 
        query: String, 
        chunks: Vec<crate::models::RetrievedChunk>, 
        model_config_id: &str
    ) -> Result<Vec<crate::models::RetrievedChunk>> {
        println!("🔄 调用重排序API，候选文档数量: {}", chunks.len());
        
        // 获取API配置
        let configs = self.get_api_configs().await?;
        let config = configs.iter()
            .find(|c| c.id == model_config_id)
            .ok_or_else(|| AppError::configuration("找不到重排序模型配置"))?;
        
        // 解密API密钥
        let api_key = self.decrypt_api_key_if_needed(&config.api_key)?;
        
        // 构造重排序请求
        let documents: Vec<String> = chunks.iter()
            .map(|chunk| chunk.chunk.text.clone())
            .collect();
        
        let request_body = json!({
            "model": config.model,
            "query": query,
            "documents": documents,
            "top_k": chunks.len(),
            "return_documents": true
        });
        
        // 发送请求
        let url = format!("{}/rerank", config.base_url.trim_end_matches('/'));
        let mut request_builder = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/plain, */*")
            .header("Accept-Encoding", "identity")  // 禁用压缩，避免二进制响应
            .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .header("Connection", "keep-alive")
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36");

        if let Ok(parsed_url) = Url::parse(&config.base_url) { // config is the specific model config here
            if (parsed_url.scheme() == "http" || parsed_url.scheme() == "https") && parsed_url.host_str().is_some() {
                let origin_val = format!("{}://{}", parsed_url.scheme(), parsed_url.host_str().unwrap_or_default());
                let referer_val = format!("{}://{}/", parsed_url.scheme(), parsed_url.host_str().unwrap_or_default());
                 request_builder = request_builder
                    .header("Origin", origin_val)
                    .header("Referer", referer_val);
            }
        }
        
        let response = request_builder
            .json(&request_body)
            .send()
            .await
            .map_err(|e| AppError::network(format!("重排序API请求失败: {}", e)))?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(AppError::llm(format!("重排序API返回错误 {}: {}", status, error_text)));
        }
        
        let response_json: Value = response.json().await
            .map_err(|e| AppError::llm(format!("解析重排序API响应失败: {}", e)))?;
        
        // 解析重排序结果
        let results = response_json["results"].as_array()
            .ok_or_else(|| AppError::llm("重排序API响应格式无效：缺少results字段"))?;
        
        let mut reranked_chunks = Vec::new();
        for result in results {
            let index = result["index"].as_u64()
                .ok_or_else(|| AppError::llm("重排序结果缺少index字段"))? as usize;
            let relevance_score = result["relevance_score"].as_f64()
                .ok_or_else(|| AppError::llm("重排序结果缺少relevance_score字段"))? as f32;
            
            if index < chunks.len() {
                let mut reranked_chunk = chunks[index].clone();
                reranked_chunk.score = relevance_score;
                reranked_chunks.push(reranked_chunk);
            }
        }
        
        println!("✅ 重排序API调用成功，返回 {} 个重排序结果", reranked_chunks.len());
        Ok(reranked_chunks)
    }
    
    /// RAG增强的流式模型调用
    pub async fn call_rag_enhanced_model_stream(
        &self,
        analysis_context: &std::collections::HashMap<String, serde_json::Value>,
        retrieved_context: Vec<crate::models::RetrievedChunk>,
        chat_history: &[ChatMessage],
        subject: &str,
        enable_chain_of_thought: bool,
        image_paths: Option<Vec<String>>, // 🎯 修复：添加图片路径参数
        window: Window,
        stream_event: &str,
    ) -> Result<StandardModel2Output> {
        println!("🚀 开始RAG增强的流式模型调用");
        
        // 构建增强的上下文
        let mut enhanced_context = analysis_context.clone();
        
        // 添加检索到的上下文
        if !retrieved_context.is_empty() {
            let context_text = retrieved_context.iter()
                .enumerate()
                .map(|(i, chunk)| {
                    format!(
                        "--- 参考信息{} (来源: {}) ---\n{}",
                        i + 1,
                        chunk.chunk.metadata.get("file_name").unwrap_or(&"unknown".to_string()),
                        chunk.chunk.text
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n");
            
            enhanced_context.insert("rag_context".to_string(), serde_json::Value::String(context_text));
        }
        
        enhanced_context.insert("has_rag_context".to_string(), serde_json::Value::Bool(!retrieved_context.is_empty()));
        
        // 发送RAG来源信息事件到前端
        if !retrieved_context.is_empty() {
            let rag_sources: Vec<serde_json::Value> = retrieved_context.iter().map(|chunk| {
                serde_json::json!({
                    "document_id": chunk.chunk.document_id,
                    "file_name": chunk.chunk.metadata.get("file_name").unwrap_or(&"unknown".to_string()),
                    "chunk_text": chunk.chunk.text,
                    "score": chunk.score,
                    "chunk_index": chunk.chunk.chunk_index
                })
            }).collect();
            
            let rag_sources_event = format!("{}_rag_sources", stream_event);
            let rag_payload = serde_json::json!({
                "sources": rag_sources
            });
            
            // 发送RAG来源信息事件
            if let Err(e) = window.emit(&rag_sources_event, &rag_payload) {
                println!("⚠️ 发送RAG来源信息事件失败: {}", e);
            } else {
                println!("✅ RAG来源信息事件已发送: {} 个来源", rag_sources.len());
            }
        }
        
        // 调用原有的流式模型方法，传入增强的上下文
        self.call_unified_model_2_stream(
            &enhanced_context,
            chat_history,
            subject,
            enable_chain_of_thought,
            image_paths, // 🎯 修复：传递图片路径参数
            Some("rag_enhanced_analysis"), // 任务类型标识
            window,
            stream_event,
        ).await
    }
}
