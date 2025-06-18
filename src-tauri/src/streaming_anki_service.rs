use crate::models::{
    DocumentTask, TaskStatus, AnkiCard, AnkiGenerationOptions, AppError, StreamedCardPayload, 
    StreamEvent, SubjectConfig, FieldType, FieldExtractionRule
};
use crate::llm_manager::ApiConfig;
use crate::database::Database;
use crate::llm_manager::LLMManager;
use std::sync::Arc;
use uuid::Uuid;
use chrono::Utc;
use reqwest::Client;
use serde_json::{json, Value};
use futures_util::StreamExt;
use tauri::{Window, Emitter};
use std::time::Duration;
use tokio::time::timeout;

#[derive(Clone)]
pub struct StreamingAnkiService {
    db: Arc<Database>,
    llm_manager: Arc<LLMManager>,
    client: Client,
}

impl StreamingAnkiService {
    pub fn new(db: Arc<Database>, llm_manager: Arc<LLMManager>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(600)) // 10分钟超时，适合流式处理
            .build()
            .expect("创建HTTP客户端失败");
            
        Self {
            db,
            llm_manager,
            client,
        }
    }

    /// 处理任务并流式生成卡片
    pub async fn process_task_and_generate_cards_stream(
        &self,
        task: DocumentTask,
        window: Window,
    ) -> Result<(), AppError> {
        let task_id = task.id.clone();
        
        
        // 更新任务状态为处理中
        self.update_task_status(&task_id, TaskStatus::Processing, None, Some(task.segment_index), &window).await?;
        
        // 获取配置
        let (api_config, subject_config) = self.get_configurations(&task.subject_name).await?;
        
        // 解析生成选项
        let options: AnkiGenerationOptions = serde_json::from_str(&task.anki_generation_options_json)
            .map_err(|e| AppError::validation(format!("解析生成选项失败: {}", e)))?;
        
        // 构建prompt
        let prompt = self.build_prompt(subject_config.as_ref(), &task.content_segment, &options)?;
        
        // 确定API参数
        let max_tokens = options.max_output_tokens_override
            .or(options.max_tokens.map(|t| t as u32))
            .unwrap_or(api_config.max_output_tokens);
        let temperature = options.temperature_override
            .or(options.temperature)
            .unwrap_or(api_config.temperature);
        
        // 开始流式处理
        self.update_task_status(&task_id, TaskStatus::Streaming, None, Some(task.segment_index), &window).await?;
        
        let result = self.stream_cards_from_ai(
            &api_config,
            &prompt,
            max_tokens,
            temperature,
            &task_id,
            &window,
            &options,
        ).await;
        
        match result {
            Ok(card_count) => {
                self.complete_task_successfully(&task_id, card_count, &window).await?;
            }
            Err(e) => {
                self.handle_task_error(&task_id, &e, &window).await?;
            }
        }
        
        Ok(())
    }

    /// 获取API配置和科目配置（科目配置可选）
    async fn get_configurations(&self, subject_name: &str) -> Result<(ApiConfig, Option<SubjectConfig>), AppError> {
        // 获取模型分配
        let model_assignments = self.llm_manager.get_model_assignments().await
            .map_err(|e| AppError::configuration(format!("获取模型分配失败: {}", e)))?;
        
        // 获取Anki制卡模型配置
        let anki_model_id = model_assignments.anki_card_model_config_id
            .ok_or_else(|| AppError::configuration("Anki制卡模型在模型分配中未配置 (anki_card_model_config_id is None)"))?;
        println!("[ANKI_CONFIG_DEBUG] Anki Model ID from assignments: {}", anki_model_id);
        
        let api_configs = self.llm_manager.get_api_configs().await
            .map_err(|e| AppError::configuration(format!("获取API配置失败: {}", e)))?;
        
        let config_count = api_configs.len();
        let api_config = api_configs.into_iter()
            .find(|config| config.id == anki_model_id && config.enabled)
            .ok_or_else(|| AppError::configuration(format!("找不到有效的Anki制卡模型配置. Tried to find ID: {} in {} available configs.", anki_model_id, config_count)))?;
        
        println!("[ANKI_CONFIG_DEBUG] Found ApiConfig for ANKI: ID='{}', Name='{}', BaseURL='{}', Model='{}', Enabled='{}'",
            api_config.id,
            api_config.name.as_str(), // Assuming name is String, not Option<String>
            api_config.base_url,
            api_config.model,
            api_config.enabled
        );

        // 尝试获取科目配置，但不再要求必须存在
        let subject_config = match self.db.get_subject_config_by_name(subject_name) {
            Ok(Some(config)) => {
                println!("✅ 找到科目配置: {}", subject_name);
                Some(config)
            }
            Ok(None) => {
                println!("ℹ️ 未找到科目配置: {}，将使用默认配置", subject_name);
                None
            }
            Err(e) => {
                println!("⚠️ 获取科目配置失败，将使用默认配置: {}", e);
                None
            }
        };
        
        Ok((api_config, subject_config))
    }

    /// 构建AI提示词
    fn build_prompt(
        &self,
        subject_config: Option<&SubjectConfig>,
        content: &str,
        options: &AnkiGenerationOptions,
    ) -> Result<String, AppError> {
        // 优先级：用户自定义system_prompt > 模板prompt > 科目配置prompt > 默认prompt
        let base_prompt = if let Some(system_prompt) = &options.system_prompt {
            if !system_prompt.trim().is_empty() {
                system_prompt.clone()
            } else {
                // 如果system_prompt为空，则继续使用原有逻辑
                if let Some(custom_prompt) = &options.custom_anki_prompt {
                    custom_prompt.clone()
                } else if let Some(config) = subject_config {
                    config.prompts.anki_generation_prompt.replace("{subject}", &config.subject_name)
                } else {
                    // 默认ANKI制卡prompt
                    "你是一个专业的ANKI学习卡片制作助手。请根据提供的学习内容，生成高质量的ANKI学习卡片。\n\n要求：\n1. 卡片应该有助于记忆和理解\n2. 问题要简洁明确\n3. 答案要准确完整\n4. 适当添加相关标签\n5. 确保卡片的逻辑性和实用性".to_string()
                }
            }
        } else {
            // 如果没有设置system_prompt，使用原有逻辑
            if let Some(custom_prompt) = &options.custom_anki_prompt {
                custom_prompt.clone()
            } else if let Some(config) = subject_config {
                config.prompts.anki_generation_prompt.replace("{subject}", &config.subject_name)
            } else {
                // 默认ANKI制卡prompt
                "你是一个专业的ANKI学习卡片制作助手。请根据提供的学习内容，生成高质量的ANKI学习卡片。\n\n要求：\n1. 卡片应该有助于记忆和理解\n2. 问题要简洁明确\n3. 答案要准确完整\n4. 适当添加相关标签\n5. 确保卡片的逻辑性和实用性".to_string()
            }
        };
        
        // 获取模板字段，默认为基础字段
        let template_fields = options.template_fields.as_ref()
            .map(|fields| fields.clone())
            .unwrap_or_else(|| vec!["front".to_string(), "back".to_string(), "tags".to_string()]);
        
        // 动态构建字段要求
        let fields_requirement = template_fields.iter()
            .map(|field| {
                match field.as_str() {
                    "front" => "front（字符串）：问题或概念".to_string(),
                    "back" => "back（字符串）：答案或解释".to_string(), 
                    "tags" => "tags（字符串数组）：相关标签".to_string(),
                    "example" => "example（字符串，可选）：具体示例".to_string(),
                    "source" => "source（字符串，可选）：来源信息".to_string(),
                    "code" => "code（字符串，可选）：代码示例".to_string(),
                    "notes" => "notes（字符串，可选）：补充注释".to_string(),
                    _ => format!("{}（字符串，可选）：{}", field, field),
                }
            })
            .collect::<Vec<_>>()
            .join("、");
        
        // 构建示例JSON
        let example_json = {
            let mut example_fields = vec![];
            for field in &template_fields {
                match field.as_str() {
                    "front" => example_fields.push("\"front\": \"问题内容\"".to_string()),
                    "back" => example_fields.push("\"back\": \"答案内容\"".to_string()),
                    "tags" => example_fields.push("\"tags\": [\"标签1\", \"标签2\"]".to_string()),
                    "example" => example_fields.push("\"example\": \"示例内容\"".to_string()),
                    "source" => example_fields.push("\"source\": \"来源信息\"".to_string()),
                    "code" => example_fields.push("\"code\": \"代码示例\"".to_string()),
                    "notes" => example_fields.push("\"notes\": \"注释内容\"".to_string()),
                    _ => example_fields.push(format!("\"{}\": \"{}内容\"", field, field)),
                }
            }
            format!("{{{}}}", example_fields.join(", "))
        };
        
        // 添加自定义要求部分
        let custom_requirements_text = if let Some(requirements) = &options.custom_requirements {
            if !requirements.trim().is_empty() {
                format!("\n\n📋 特殊制卡要求：\n{}\n请严格按照以上要求进行制卡。", requirements.trim())
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        // 构建卡片数量要求
        let card_count_instruction = if options.max_cards_per_mistake > 0 {
            format!("🎯 重要提醒：你必须根据提供内容的具体情况来生成卡片：\n\
            - 如果内容是选择题格式：请为每一道选择题生成一张对应的卡片，绝不要遗漏任何题目\n\
            - 如果内容是其他格式：建议生成{}张高质量卡片，充分覆盖所有知识点\n\
            \n\
            ❗ 特别强调：不要只生成几张卡片就停止，要确保充分利用提供的内容！\n\n", options.max_cards_per_mistake)
        } else {
            "🎯 重要提醒：你必须根据提供内容的具体情况来生成卡片：\n\
            - 如果内容是选择题格式：请为每一道选择题生成一张对应的卡片，绝不要遗漏任何题目\n\
            - 如果内容是其他格式：请生成尽可能多的高质量Anki卡片，充分覆盖所有知识点\n\
            \n\
            ❗ 特别强调：不要只生成几张卡片就停止，要确保充分利用提供的内容！\n\n".to_string()
        };

        // 增强prompt以支持流式输出和动态字段
        let enhanced_prompt = format!(
            "{}{}\n\n{}\
            重要指令：\n\
            1. 请逐个生成卡片，每个卡片必须是完整的JSON格式\n\
            2. 每生成一个完整的卡片JSON后，立即输出分隔符：<<<ANKI_CARD_JSON_END>>>\n\
            3. JSON格式必须包含以下字段：{}\n\
            4. 不要使用Markdown代码块，直接输出JSON\n\
            5. 示例输出格式：\n\
            {}\n\
            <<<ANKI_CARD_JSON_END>>>\n\n\
            请根据以下内容生成Anki卡片：\n\n{}",
            base_prompt, custom_requirements_text, card_count_instruction, fields_requirement, example_json, content
        );
        
        Ok(enhanced_prompt)
    }

    /// 流式处理AI响应并生成卡片
    async fn stream_cards_from_ai(
        &self,
        api_config: &ApiConfig,
        prompt: &str,
        max_tokens: u32,
        temperature: f32,
        task_id: &str,
        window: &Window,
        options: &AnkiGenerationOptions,
    ) -> Result<u32, AppError> {
        let request_body = json!({
            "model": api_config.model,
            "messages": [
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "max_tokens": max_tokens,
            "temperature": temperature,
            "stream": true
        });

        let request_url = format!("{}/chat/completions", api_config.base_url.trim_end_matches('/'));
        println!("[ANKI_REQUEST_DEBUG] Attempting to POST to URL: {}", request_url);
        println!("[ANKI_REQUEST_DEBUG] Request Body Model: {}", api_config.model);
        println!("[ANKI_REQUEST_DEBUG] Prompt length: {}", prompt.len());
        println!("[ANKI_REQUEST_DEBUG] Max Tokens: {}, Temperature: {}", max_tokens, temperature);
        println!("[ANKI_REQUEST_DEBUG] Max Cards Per Mistake: {}", options.max_cards_per_mistake);
        println!("[ANKI_REQUEST_DEBUG] System Prompt: {}", 
                if let Some(sp) = &options.system_prompt { 
                    if sp.trim().is_empty() { "未设置" } else { "已自定义" }
                } else { "使用默认" });
        
        // 输出完整的 prompt 内容
        println!("[ANKI_PROMPT_DEBUG] ==> 完整Prompt内容开始 <==");
        println!("{}", prompt);
        println!("[ANKI_PROMPT_DEBUG] ==> 完整Prompt内容结束 <==");
        
        // 输出完整的请求体
        println!("[ANKI_REQUEST_DEBUG] ==> 完整请求体开始 <==");
        println!("{}", serde_json::to_string_pretty(&request_body).unwrap_or_default());
        println!("[ANKI_REQUEST_DEBUG] ==> 完整请求体结束 <==");

        let response = self.client
            .post(&request_url)
            .header("Authorization", format!("Bearer {}", api_config.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| AppError::network(format!("AI请求失败: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AppError::llm(format!("AI API错误: {}", error_text)));
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut card_count = 0u32;
        let mut _last_activity = std::time::Instant::now(); // Prefixed to silence warning
        const IDLE_TIMEOUT: Duration = Duration::from_secs(30); // 30秒无响应超时
        let mut all_received_content = String::new(); // 用于记录所有接收到的内容

        while let Some(chunk_result) = timeout(IDLE_TIMEOUT, stream.next()).await
            .map_err(|_| AppError::network("AI响应超时"))? 
        {

            let chunk = chunk_result
                .map_err(|e| AppError::network(format!("读取AI响应流失败: {}", e)))?;
            
            _last_activity = std::time::Instant::now(); // Prefixed to silence warning
            
            let chunk_str = String::from_utf8_lossy(&chunk);
            
            // 处理SSE格式
            for line in chunk_str.lines() {
                if line.starts_with("data: ") {
                    let data = &line[6..]; // 去掉 "data: " 前缀
                    
                    if data == "[DONE]" {
                        break;
                    }
                    
                    // 解析SSE数据
                    if let Ok(json_data) = serde_json::from_str::<Value>(data) {
                        if let Some(content) = json_data["choices"][0]["delta"]["content"].as_str() {
                            buffer.push_str(content);
                            all_received_content.push_str(content); // 记录所有内容
                            
                            // 检查是否有完整的卡片
                            while let Some(card_result) = self.extract_card_from_buffer(&mut buffer) {
                                match card_result {
                                    Ok(card_json) => {
                                        match self.parse_and_save_card(&card_json, task_id, options.template_id.as_deref(), &options.field_extraction_rules).await {
                                            Ok(card) => {
                                                card_count += 1;
                                                println!("[ANKI_CARD_DEBUG] 已生成第{}张卡片 (目标: {}张)", card_count, options.max_cards_per_mistake);
                                                self.emit_new_card(card, window).await;
                                            }
                                            Err(e) => {
                                                println!("解析卡片失败: {} - 原始JSON: {}", e, card_json);
                                                // 继续处理，不中断整个流程
                                            }
                                        }
                                    }
                                    Err(truncated_content) => {
                                        // 处理截断内容
                                        if let Ok(error_card) = self.create_error_card(&truncated_content, task_id).await {
                                            self.emit_error_card(error_card, window).await;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 处理剩余缓冲区内容
        if !buffer.trim().is_empty() {
            if let Ok(error_card) = self.create_error_card(&buffer, task_id).await {
                self.emit_error_card(error_card, window).await;
            }
        }

        // 输出完整的AI响应内容
        println!("[ANKI_RESPONSE_DEBUG] ==> 完整AI响应内容开始 <==");
        println!("{}", all_received_content);
        println!("[ANKI_RESPONSE_DEBUG] ==> 完整AI响应内容结束 <==");
        println!("[ANKI_RESPONSE_DEBUG] 总共生成卡片数量: {}", card_count);
        println!("[ANKI_RESPONSE_DEBUG] 剩余缓冲区内容: '{}'", buffer);

        Ok(card_count)
    }

    /// 从缓冲区提取卡片
    fn extract_card_from_buffer(&self, buffer: &mut String) -> Option<Result<String, String>> {
        const DELIMITER: &str = "<<<ANKI_CARD_JSON_END>>>";
        
        if let Some(delimiter_pos) = buffer.find(DELIMITER) {
            let card_content = buffer[..delimiter_pos].trim().to_string();
            let remaining = buffer[delimiter_pos + DELIMITER.len()..].to_string();
            *buffer = remaining;
            
            if !card_content.is_empty() {
                Some(Ok(card_content))
            } else {
                None
            }
        } else if buffer.len() > 10000 { // 如果缓冲区过大，可能是截断
            let truncated = buffer.clone();
            buffer.clear();
            Some(Err(truncated))
        } else {
            None
        }
    }

    /// 解析并保存卡片 - 支持动态字段提取规则
    async fn parse_and_save_card(&self, card_json: &str, task_id: &str, template_id: Option<&str>, extraction_rules: &Option<std::collections::HashMap<String, FieldExtractionRule>>) -> Result<AnkiCard, AppError> {
        // 清理JSON字符串
        let cleaned_json = self.clean_json_string(card_json);
        
        // 解析JSON
        let json_value: Value = serde_json::from_str(&cleaned_json)
            .map_err(|e| AppError::validation(format!("JSON解析失败: {} - 原始内容: {}", e, card_json)))?;
        
        // 动态字段提取 - 使用模板的字段提取规则
        let (front, back, tags, extra_fields) = if let Some(rules) = extraction_rules {
            self.extract_fields_with_rules(&json_value, rules)?
        } else {
            // 回退到旧的硬编码逻辑
            self.extract_fields_legacy(&json_value)?
        };
        
        // 清理所有字段中的模板占位符
        let cleaned_front = self.clean_template_placeholders(&front);
        let cleaned_back = self.clean_template_placeholders(&back);
        let cleaned_tags: Vec<String> = tags.iter()
            .map(|tag| self.clean_template_placeholders(tag))
            .filter(|tag| !tag.is_empty())
            .collect();
        let cleaned_extra_fields: std::collections::HashMap<String, String> = extra_fields.iter()
            .map(|(k, v)| (k.clone(), self.clean_template_placeholders(v)))
            .collect();
        
        // 创建卡片
        let now = Utc::now().to_rfc3339();
        let card = AnkiCard {
            id: Uuid::new_v4().to_string(),
            task_id: task_id.to_string(),
            front: cleaned_front,
            back: cleaned_back,
            text: cleaned_extra_fields.get("text").cloned(), // 从清理后的extra_fields中提取text字段
            tags: cleaned_tags,
            images: Vec::new(),
            is_error_card: false,
            error_content: None,
            created_at: now.clone(),
            updated_at: now,
            extra_fields: cleaned_extra_fields,
            template_id: template_id.map(|id| id.to_string()),
        };
        
        // 检查是否存在重复卡片 - 支持不同卡片类型的重复检测
        if let Ok(existing_cards) = self.db.get_cards_for_task(task_id) {
            let is_duplicate = existing_cards.iter().any(|existing| {
                // 对于Cloze类型，比较text字段；对于其他类型，比较front和back字段
                if card.text.is_some() && existing.text.is_some() {
                    // 两张卡片都有text字段，按Cloze类型处理
                    card.text == existing.text && card.text.as_ref().unwrap().len() > 0
                } else {
                    // 按传统方式比较front和back字段
                    existing.front == card.front && existing.back == card.back
                }
            });
            if is_duplicate {
                let preview = card.text.as_ref().unwrap_or(&card.front).chars().take(50).collect::<String>();
                println!("⚠️ 发现重复卡片，跳过保存: {}", preview);
                return Err(AppError::validation("重复卡片已跳过".to_string()));
            }
        }

        // 保存到数据库
        self.db.insert_anki_card(&card)
            .map_err(|e| AppError::database(format!("保存卡片失败: {}", e)))?;
        
        Ok(card)
    }

    /// 清理JSON字符串
    fn clean_json_string(&self, json_str: &str) -> String {
        let mut cleaned = json_str.trim();
        
        // 移除Markdown代码块标记
        if cleaned.starts_with("```json") {
            cleaned = &cleaned[7..];
        }
        if cleaned.starts_with("```") {
            cleaned = &cleaned[3..];
        }
        if cleaned.ends_with("```") {
            cleaned = &cleaned[..cleaned.len() - 3];
        }
        
        cleaned.trim().to_string()
    }

    /// 清理模板占位符
    fn clean_template_placeholders(&self, content: &str) -> String {
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

    /// 使用模板字段提取规则动态解析字段
    fn extract_fields_with_rules(
        &self, 
        json_value: &Value, 
        rules: &std::collections::HashMap<String, FieldExtractionRule>
    ) -> Result<(String, String, Vec<String>, std::collections::HashMap<String, String>), AppError> {
        
        let mut front = String::new();
        let mut back = String::new();
        let mut tags = Vec::new();
        let mut extra_fields = std::collections::HashMap::new();
        
        // 遍历所有定义的字段规则
        for (field_name, rule) in rules {
            let field_value = self.extract_field_value(json_value, field_name);
            
            match (field_value, rule.is_required) {
                (Some(value), _) => {
                    // 字段存在，根据类型和字段名称处理
                    match field_name.to_lowercase().as_str() {
                        "front" => {
                            front = self.process_field_value(&value, &rule.field_type)?;
                        }
                        "back" => {
                            back = self.process_field_value(&value, &rule.field_type)?;
                        }
                        "tags" => {
                            tags = self.process_tags_field(&value, &rule.field_type)?;
                        }
                        "explanation" => {
                            // 选择题的答案需要组合多个字段
                            let explanation_text = self.process_field_value(&value, &rule.field_type)?;
                            // 先保存explanation，稍后组合完整答案
                            extra_fields.insert("explanation".to_string(), explanation_text);
                        }
                        // 填空题模板字段映射
                        "text" => {
                            // 对于填空题，Text字段应该保存到extra_fields中，用于Cloze模板
                            let processed_value = self.process_field_value(&value, &rule.field_type)?;
                            extra_fields.insert("text".to_string(), processed_value.clone());
                            // 同时设置front字段以确保基础验证通过
                            front = processed_value.clone();
                            back = format!("填空题：{}", processed_value); // 为back字段提供有意义的内容
                        }
                        _ => {
                            // 扩展字段
                            let processed_value = self.process_field_value(&value, &rule.field_type)?;
                            extra_fields.insert(field_name.to_lowercase(), processed_value);
                        }
                    }
                }
                (None, true) => {
                    // 必需字段缺失
                    if let Some(default) = &rule.default_value {
                        match field_name.to_lowercase().as_str() {
                            "front" => front = default.clone(),
                            "back" => back = default.clone(),
                            "tags" => tags = serde_json::from_str(default).unwrap_or_default(),
                            _ => {
                                extra_fields.insert(field_name.to_lowercase(), default.clone());
                            }
                        }
                    } else {
                        return Err(AppError::validation(format!("缺少必需字段: {}", field_name)));
                    }
                }
                (None, false) => {
                    // 可选字段缺失，使用默认值
                    if let Some(default) = &rule.default_value {
                        match field_name.to_lowercase().as_str() {
                            "front" => front = default.clone(),
                            "back" => back = default.clone(),
                            "tags" => tags = serde_json::from_str(default).unwrap_or_default(),
                            _ => {
                                extra_fields.insert(field_name.to_lowercase(), default.clone());
                            }
                        }
                    }
                    // 如果没有默认值，就不设置该字段
                }
            }
        }
        
        // 特殊处理选择题模板的back字段组合
        if extra_fields.contains_key("optiona") {
            // 这是选择题模板，需要组合答案
            let mut choice_back = String::new();
            
            // 添加选项
            if let Some(option_a) = extra_fields.get("optiona") {
                choice_back.push_str(&format!("A. {}\n", option_a));
            }
            if let Some(option_b) = extra_fields.get("optionb") {
                choice_back.push_str(&format!("B. {}\n", option_b));
            }
            if let Some(option_c) = extra_fields.get("optionc") {
                choice_back.push_str(&format!("C. {}\n", option_c));
            }
            if let Some(option_d) = extra_fields.get("optiond") {
                choice_back.push_str(&format!("D. {}\n", option_d));
            }
            
            // 添加正确答案
            if let Some(correct) = extra_fields.get("correct") {
                choice_back.push_str(&format!("\n正确答案：{}\n", correct));
            }
            
            // 添加解析
            if let Some(explanation) = extra_fields.get("explanation") {
                choice_back.push_str(&format!("\n解析：{}", explanation));
            }
            
            back = choice_back;
        }
        
        // 确保front和back字段有值
        if front.is_empty() {
            return Err(AppError::validation(format!("front字段不能为空 - 原始JSON: {}", serde_json::to_string(&json_value).unwrap_or_default())));
        }
        if back.is_empty() {
            // 尝试为选择题自动生成back内容
            if json_value.get("optiona").and_then(|v| v.as_str()).is_some() {
                let mut choice_back = String::new();
                
                // 添加选项并保存到extra_fields
                if let Some(option_a) = json_value.get("optiona").and_then(|v| v.as_str()) {
                    choice_back.push_str(&format!("A. {}\n", option_a));
                    extra_fields.insert("optiona".to_string(), option_a.to_string());
                }
                if let Some(option_b) = json_value.get("optionb").and_then(|v| v.as_str()) {
                    choice_back.push_str(&format!("B. {}\n", option_b));
                    extra_fields.insert("optionb".to_string(), option_b.to_string());
                }
                if let Some(option_c) = json_value.get("optionc").and_then(|v| v.as_str()) {
                    choice_back.push_str(&format!("C. {}\n", option_c));
                    extra_fields.insert("optionc".to_string(), option_c.to_string());
                }
                if let Some(option_d) = json_value.get("optiond").and_then(|v| v.as_str()) {
                    choice_back.push_str(&format!("D. {}\n", option_d));
                    extra_fields.insert("optiond".to_string(), option_d.to_string());
                }
                
                // 添加正确答案并保存到extra_fields
                if let Some(correct) = json_value.get("correct").and_then(|v| v.as_str()) {
                    choice_back.push_str(&format!("\n正确答案：{}\n", correct));
                    extra_fields.insert("correct".to_string(), correct.to_string());
                }
                
                // 添加解析并保存到extra_fields
                if let Some(explanation) = json_value.get("explanation").and_then(|v| v.as_str()) {
                    choice_back.push_str(&format!("\n解析：{}", explanation));
                    extra_fields.insert("explanation".to_string(), explanation.to_string());
                }
                
                back = choice_back;
            } else {
                return Err(AppError::validation("back字段不能为空".to_string()));
            }
        }
        
        Ok((front, back, tags, extra_fields))
    }
    
    /// 从JSON中提取字段值（支持大小写不敏感）
    fn extract_field_value(&self, json_value: &Value, field_name: &str) -> Option<Value> {
        let obj = json_value.as_object()?;
        
        // 首先尝试精确匹配
        if let Some(value) = obj.get(field_name) {
            return Some(value.clone());
        }
        
        // 然后尝试大小写不敏感匹配
        let field_lower = field_name.to_lowercase();
        for (key, value) in obj {
            if key.to_lowercase() == field_lower {
                return Some(value.clone());
            }
        }
        
        None
    }
    
    /// 根据字段类型处理字段值
    fn process_field_value(&self, value: &Value, field_type: &FieldType) -> Result<String, AppError> {
        match field_type {
            FieldType::Text => {
                if let Some(s) = value.as_str() {
                    Ok(s.to_string())
                } else {
                    // 如果不是字符串，尝试序列化为字符串
                    Ok(value.to_string().trim_matches('"').to_string())
                }
            }
            FieldType::Array => {
                if let Some(arr) = value.as_array() {
                    let strings = arr.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    Ok(strings)
                } else if let Some(s) = value.as_str() {
                    Ok(s.to_string())
                } else {
                    Ok(value.to_string().trim_matches('"').to_string())
                }
            }
            FieldType::Number => {
                if let Some(n) = value.as_f64() {
                    Ok(n.to_string())
                } else if let Some(s) = value.as_str() {
                    Ok(s.to_string())
                } else {
                    Ok(value.to_string().trim_matches('"').to_string())
                }
            }
            FieldType::Boolean => {
                if let Some(b) = value.as_bool() {
                    Ok(b.to_string())
                } else if let Some(s) = value.as_str() {
                    Ok(s.to_string())
                } else {
                    Ok(value.to_string().trim_matches('"').to_string())
                }
            }
        }
    }
    
    /// 处理tags字段
    fn process_tags_field(&self, value: &Value, field_type: &FieldType) -> Result<Vec<String>, AppError> {
        match field_type {
            FieldType::Array => {
                if let Some(arr) = value.as_array() {
                    Ok(arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect())
                } else if let Some(s) = value.as_str() {
                    // 尝试解析逗号分隔的字符串
                    Ok(s.split(',')
                        .map(|tag| tag.trim().to_string())
                        .filter(|tag| !tag.is_empty())
                        .collect())
                } else {
                    Ok(vec![])
                }
            }
            FieldType::Text => {
                if let Some(s) = value.as_str() {
                    Ok(s.split(',')
                        .map(|tag| tag.trim().to_string())
                        .filter(|tag| !tag.is_empty())
                        .collect())
                } else {
                    Ok(vec![])
                }
            }
            _ => Ok(vec![])
        }
    }
    
    /// 回退的旧式字段提取逻辑（兼容性）
    fn extract_fields_legacy(
        &self, 
        json_value: &Value
    ) -> Result<(String, String, Vec<String>, std::collections::HashMap<String, String>), AppError> {
        // 提取必需字段 (支持大小写不敏感)
        let front = json_value["front"].as_str()
            .or_else(|| json_value["Front"].as_str())
            .ok_or_else(|| AppError::validation("缺少front/Front字段"))?
            .to_string();
        
        let mut back = json_value["back"].as_str()
            .or_else(|| json_value["Back"].as_str())
            .map(|s| s.to_string())
            .unwrap_or_default();
            
        // 如果没有back字段，检查是否为选择题模板，自动生成back内容
        if back.is_empty() && json_value["optiona"].is_string() {
            let mut choice_back = String::new();
            
            // 添加选项
            if let Some(option_a) = json_value["optiona"].as_str() {
                choice_back.push_str(&format!("A. {}\n", option_a));
            }
            if let Some(option_b) = json_value["optionb"].as_str() {
                choice_back.push_str(&format!("B. {}\n", option_b));
            }
            if let Some(option_c) = json_value["optionc"].as_str() {
                choice_back.push_str(&format!("C. {}\n", option_c));
            }
            if let Some(option_d) = json_value["optiond"].as_str() {
                choice_back.push_str(&format!("D. {}\n", option_d));
            }
            
            // 添加正确答案
            if let Some(correct) = json_value["correct"].as_str() {
                choice_back.push_str(&format!("\n正确答案：{}\n", correct));
            }
            
            // 添加解析
            if let Some(explanation) = json_value["explanation"].as_str() {
                choice_back.push_str(&format!("\n解析：{}", explanation));
            }
            
            back = choice_back;
        }
        
        // 确保back字段不为空
        if back.is_empty() {
            return Err(AppError::validation("缺少back/Back字段".to_string()));
        }
        
        let tags = json_value["tags"].as_array()
            .or_else(|| json_value["Tags"].as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();
        
        // 提取扩展字段
        let mut extra_fields = std::collections::HashMap::new();
        if let Some(obj) = json_value.as_object() {
            for (key, value) in obj {
                // 跳过基础字段 (大小写不敏感)
                let key_lower = key.to_lowercase();
                if !matches!(key_lower.as_str(), "front" | "back" | "tags" | "images") {
                    if let Some(str_value) = value.as_str() {
                        // 将字段名转换为统一的小写格式存储
                        extra_fields.insert(key_lower, str_value.to_string());
                    } else if let Some(arr_value) = value.as_array() {
                        // 将数组转换为字符串
                        let arr_str = arr_value.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(", ");
                        extra_fields.insert(key_lower, arr_str);
                    } else {
                        // 其他类型转换为字符串
                        extra_fields.insert(key_lower, value.to_string());
                    }
                }
            }
        }
        
        Ok((front, back, tags, extra_fields))
    }

    /// 创建错误卡片
    async fn create_error_card(&self, error_content: &str, task_id: &str) -> Result<AnkiCard, AppError> {
        let now = Utc::now().to_rfc3339();
        let card = AnkiCard {
            id: Uuid::new_v4().to_string(),
            task_id: task_id.to_string(),
            front: "内容可能被截断或AI输出不完整".to_string(),
            back: "请检查以下原始片段并手动创建或编辑卡片。".to_string(),
            text: None, // 错误卡片不需要text字段
            tags: vec!["错误".to_string(), "截断".to_string()],
            images: Vec::new(),
            is_error_card: true,
            error_content: Some(error_content.to_string()),
            created_at: now.clone(),
            updated_at: now,
            extra_fields: std::collections::HashMap::new(),
            template_id: None,
        };
        
        // 保存到数据库
        self.db.insert_anki_card(&card)
            .map_err(|e| AppError::database(format!("保存错误卡片失败: {}", e)))?;
        
        Ok(card)
    }

    /// 更新任务状态
    async fn update_task_status(
        &self,
        task_id: &str,
        status: TaskStatus,
        error_message: Option<String>,
        segment_index: Option<u32>, // 新增参数
        window: &Window,
    ) -> Result<(), AppError> {
        self.db.update_document_task_status(task_id, status.clone(), error_message.clone())
            .map_err(|e| AppError::database(format!("更新任务状态失败: {}", e)))?;
        
        // 发送状态更新事件
        let event = StreamEvent {
            payload: StreamedCardPayload::TaskStatusUpdate {
                task_id: task_id.to_string(),
                status,
                message: error_message,
                segment_index, // 包含 segment_index
            },
        };
        
        if let Err(e) = window.emit("anki_generation_event", &event) {
            println!("发送任务状态更新事件失败: {}", e);
        }
        
        Ok(())
    }

    /// 发送新卡片事件
    async fn emit_new_card(&self, card: AnkiCard, window: &Window) {
        let event = StreamEvent {
            payload: StreamedCardPayload::NewCard(card),
        };
        
        if let Err(e) = window.emit("anki_generation_event", &event) {
            println!("发送新卡片事件失败: {}", e);
        }
    }

    /// 发送错误卡片事件
    async fn emit_error_card(&self, card: AnkiCard, window: &Window) {
        let event = StreamEvent {
            payload: StreamedCardPayload::NewErrorCard(card),
        };
        
        if let Err(e) = window.emit("anki_generation_event", &event) {
            println!("发送错误卡片事件失败: {}", e);
        }
    }

    /// 成功完成任务
    async fn complete_task_successfully(
        &self,
        task_id: &str,
        card_count: u32,
        window: &Window,
    ) -> Result<(), AppError> {
        // For TaskCompleted, segment_index might be less critical if task_id is already real.
        // Passing None for now, as the primary use of segment_index is for the initial ID update.
        self.update_task_status(task_id, TaskStatus::Completed, None, None, window).await?;
        
        // 发送任务完成事件
        let event = StreamEvent {
            payload: StreamedCardPayload::TaskCompleted {
                task_id: task_id.to_string(),
                final_status: TaskStatus::Completed,
                total_cards_generated: card_count,
            },
        };
        
        if let Err(e) = window.emit("anki_generation_event", &event) {
            println!("发送任务完成事件失败: {}", e);
        }
        
        Ok(())
    }

    /// 处理任务错误
    async fn handle_task_error(
        &self,
        task_id: &str,
        error: &AppError,
        window: &Window,
    ) -> Result<(), AppError> {
        let error_message = error.message.clone();
        let final_status = if error_message.contains("超时") || error_message.contains("截断") {
            TaskStatus::Truncated
        } else {
            TaskStatus::Failed
        };
        
        // Similarly for TaskProcessingError, passing None for segment_index.
        self.update_task_status(task_id, final_status.clone(), Some(error_message.clone()), None, window).await?;
        
        // 发送错误事件
        let event = StreamEvent {
            payload: StreamedCardPayload::TaskProcessingError {
                task_id: task_id.to_string(),
                error_message,
            },
        };
        
        if let Err(e) = window.emit("anki_generation_event", &event) {
            println!("发送任务错误事件失败: {}", e);
        }
        
        Ok(())
    }
}
