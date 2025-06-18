use crate::models::{
    ImageOcrRequest, ImageOcrResponse, TextRegion, CreateImageOcclusionRequest,
    ImageOcclusionResponse, ImageOcclusionCard, OcclusionMask, MaskStyle, AppError
};
use crate::llm_manager::{LLMManager, ApiConfig};
use crate::database::Database;
use std::sync::Arc;
use uuid::Uuid;
use chrono::Utc;
use base64::{Engine as _, engine::general_purpose};
use std::io::Cursor;
use image::{ImageFormat, DynamicImage};
use serde_json::{json, Value};

#[derive(Clone)]
pub struct ImageOcclusionService {
    db: Arc<Database>,
    llm_manager: Arc<LLMManager>,
}

impl ImageOcclusionService {
    pub fn new(db: Arc<Database>, llm_manager: Arc<LLMManager>) -> Self {
        Self {
            db,
            llm_manager,
        }
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

    /// 使用qwen2.5vl进行图片文字坐标识别
    pub async fn extract_text_coordinates(&self, request: ImageOcrRequest) -> Result<ImageOcrResponse, AppError> {
        eprintln!("🔍 开始OCR识别 - 高分辨率模式: {}", request.vl_high_resolution_images);
        // 获取模型分配配置
        let model_assignments = self.llm_manager.get_model_assignments().await
            .map_err(|e| AppError::configuration(format!("获取模型分配失败: {}", e)))?;

        // 获取第一模型配置（qwen2.5vl）
        let model1_config_id = model_assignments.model1_config_id
            .ok_or_else(|| AppError::configuration("第一模型在模型分配中未配置"))?;

        let api_configs = self.llm_manager.get_api_configs().await
            .map_err(|e| AppError::configuration(format!("获取API配置失败: {}", e)))?;

        let api_config = api_configs.into_iter()
            .find(|config| config.id == model1_config_id && config.enabled)
            .ok_or_else(|| AppError::configuration("找不到有效的qwen2.5vl模型配置"))?;

        // 解码图片获取尺寸
        let (image_width, image_height) = self.get_image_dimensions(&request.image_base64)?;

        // 构建提示词
        let prompt = self.build_ocr_prompt(&request)?;

        // 调用qwen2.5vl进行识别
        let text_regions = self.call_vision_model(&api_config, &request.image_base64, &prompt, request.vl_high_resolution_images).await?;

        // 提取完整文本
        let full_text = text_regions.iter()
            .map(|region| region.text.clone())
            .collect::<Vec<_>>()
            .join(" ");

        Ok(ImageOcrResponse {
            success: true,
            text_regions,
            full_text,
            image_width,
            image_height,
            error_message: None,
        })
    }

    /// 创建图片遮罩卡
    pub async fn create_image_occlusion_card(&self, request: CreateImageOcclusionRequest) -> Result<ImageOcclusionResponse, AppError> {
        // 首先进行OCR识别获取文字区域
        let ocr_request = ImageOcrRequest {
            image_base64: request.image_base64.clone(),
            extract_coordinates: true,
            target_text: None,
            vl_high_resolution_images: request.use_high_resolution,
        };

        let ocr_response = self.extract_text_coordinates(ocr_request).await?;

        // 根据用户选择的区域创建遮罩
        let mut masks = Vec::new();
        for region_id in &request.selected_regions {
            if let Some(region) = ocr_response.text_regions.iter().find(|r| r.region_id == *region_id) {
                let mask = OcclusionMask {
                    mask_id: Uuid::new_v4().to_string(),
                    bbox: region.bbox,
                    original_text: region.text.clone(),
                    hint: None,
                    mask_style: request.mask_style.clone(),
                };
                masks.push(mask);
            }
        }

        // 保存图片到本地（可选）
        let image_path = self.save_image(&request.image_base64)?;

        // 创建图片遮罩卡
        let now = Utc::now().to_rfc3339();
        let card = ImageOcclusionCard {
            id: Uuid::new_v4().to_string(),
            task_id: Uuid::new_v4().to_string(), // 可以关联到具体任务
            image_path,
            image_base64: Some(request.image_base64),
            image_width: ocr_response.image_width,
            image_height: ocr_response.image_height,
            masks,
            title: request.title,
            description: request.description,
            tags: request.tags,
            created_at: now.clone(),
            updated_at: now,
            subject: request.subject,
        };

        // 保存到数据库
        self.save_card_to_database(&card)?;

        Ok(ImageOcclusionResponse {
            success: true,
            card: Some(card),
            error_message: None,
        })
    }

    /// 构建OCR识别提示词
    fn build_ocr_prompt(&self, request: &ImageOcrRequest) -> Result<String, AppError> {
        let base_prompt = if request.extract_coordinates {
            r#"请分析这张图片，识别图片中的所有文字内容，并提供每个文字区域的精确坐标位置。

要求：
1. 识别图片中的所有可见文字
2. 为每个文字区域提供边界框坐标
3. 坐标格式为 [x1, y1, x2, y2]，其中(x1,y1)是左上角，(x2,y2)是右下角
4. 坐标值使用相对于图片的像素位置
5. 提供识别置信度

请以JSON格式返回结果：
{
  "text_regions": [
    {
      "text": "识别到的文字",
      "bbox": [x1, y1, x2, y2],
      "confidence": 0.95
    }
  ]
}

请确保返回的坐标准确，这将用于制作交互式学习卡片。"#
        } else {
            "请识别图片中的所有文字内容，以纯文本形式返回。"
        };

        let prompt = if let Some(target) = &request.target_text {
            format!("{}\n\n特别注意：重点识别包含'{}'的文字区域。", base_prompt, target)
        } else {
            base_prompt.to_string()
        };

        Ok(prompt)
    }

    /// 调用视觉模型进行识别
    async fn call_vision_model(&self, api_config: &ApiConfig, image_base64: &str, prompt: &str, vl_high_resolution_images: bool) -> Result<Vec<TextRegion>, AppError> {
        // 🎯 优化：使用新的适配器架构
        if api_config.model_adapter == "google" {
            // 使用新的 Gemini 适配器
            let message = crate::models::ChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
                timestamp: chrono::Utc::now(),
                thinking_content: None,
                rag_sources: None,
                image_paths: None,
                image_base64: Some(vec![image_base64.to_string()]),
            };

            let client = reqwest::Client::new();
            let gemini_result = crate::gemini_adapter::non_stream_chat(
                &client,
                api_config,
                &[message],
            ).await?;

            // 解析 Gemini 响应
            return self.parse_ocr_result(&gemini_result.assistant_message);
        }

        // 原有的 OpenAI 兼容逻辑
        let client = reqwest::Client::new();
        let model_name = api_config.model.to_lowercase();

        // OpenAI / Qwen 格式
        let url = format!("{}/chat/completions", api_config.base_url.trim_end_matches('/'));
        
        // 检测图像格式并构建image URL
        let image_format = Self::detect_image_format_from_base64(image_base64);
        eprintln!("🖼️ OCR服务检测到图像格式: {}", image_format);
        let image_url = format!("data:image/{};base64,{}", image_format, image_base64);
        
        let mut body = json!({
            "model": api_config.model,
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "text",
                            "text": prompt
                        },
                        {
                            "type": "image_url",
                            "image_url": {
                                "url": image_url
                            }
                        }
                    ]
                }
            ],
            "max_tokens": api_config.max_output_tokens,
            "temperature": api_config.temperature
        });

        if vl_high_resolution_images {
            if let Some(obj) = body.as_object_mut() {
                obj.insert("vl_high_resolution_images".to_string(), json!(true));
                eprintln!("✅ 已添加高分辨率参数到API请求");
            }
        } else {
            eprintln!("📍 使用标准分辨率模式");
        }

        let response = client.post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", api_config.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::network(format!("视觉模型请求失败: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            eprintln!("❌ API请求失败: 状态码={}, 响应={}", status, error_text);
            return Err(AppError::llm(format!("视觉模型API错误 (状态码: {}): {}", status, error_text)));
        }

        let response_json: Value = response.json().await
            .map_err(|e| AppError::llm(format!("解析响应失败: {}", e)))?;

        // 提取 OpenAI 格式的响应内容
        let content = response_json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| AppError::llm("模型响应格式错误，无法提取content字段"))?;

        // 解析JSON格式的识别结果
        self.parse_ocr_result(content)
    }

    /// 解析OCR识别结果
    fn parse_ocr_result(&self, content: &str) -> Result<Vec<TextRegion>, AppError> {
        // 更强大的内容清理逻辑，处理多种AI响应格式
        let cleaned_content = self.extract_json_from_ai_response(content)?;

        // 解析JSON
        let json_value: Value = serde_json::from_str(&cleaned_content)
            .map_err(|e| AppError::validation(format!("解析OCR结果失败: {} - 清理后内容: {}", e, cleaned_content)))?;

        // 提取text_regions数组
        let regions_array = json_value["text_regions"]
            .as_array()
            .ok_or_else(|| AppError::validation("OCR结果中缺少text_regions字段"))?;

        let mut text_regions = Vec::new();
        for (index, region) in regions_array.iter().enumerate() {
            let text = region["text"]
                .as_str()
                .ok_or_else(|| AppError::validation(format!("第{}个区域缺少text字段", index)))?
                .to_string();

            let bbox_array = region["bbox"]
                .as_array()
                .ok_or_else(|| AppError::validation(format!("第{}个区域缺少bbox字段", index)))?;

            if bbox_array.len() != 4 {
                return Err(AppError::validation(format!("第{}个区域的bbox格式错误，应该包含4个坐标值", index)));
            }

            let bbox = [
                bbox_array[0].as_f64().unwrap_or(0.0) as f32,
                bbox_array[1].as_f64().unwrap_or(0.0) as f32,
                bbox_array[2].as_f64().unwrap_or(0.0) as f32,
                bbox_array[3].as_f64().unwrap_or(0.0) as f32,
            ];

            let confidence = region["confidence"]
                .as_f64()
                .unwrap_or(1.0) as f32;

            text_regions.push(TextRegion {
                text,
                bbox,
                confidence,
                region_id: format!("region_{}", index),
            });
        }

        Ok(text_regions)
    }

    /// 获取图片尺寸
    fn get_image_dimensions(&self, image_base64: &str) -> Result<(u32, u32), AppError> {
        let image_data = general_purpose::STANDARD.decode(image_base64)
            .map_err(|e| AppError::validation(format!("base64解码失败: {}", e)))?;

        let img = image::load_from_memory(&image_data)
            .map_err(|e| AppError::validation(format!("图片加载失败: {}", e)))?;

        Ok((img.width(), img.height()))
    }

    /// 保存图片到本地
    fn save_image(&self, image_base64: &str) -> Result<String, AppError> {
        let image_data = general_purpose::STANDARD.decode(image_base64)
            .map_err(|e| AppError::validation(format!("base64解码失败: {}", e)))?;

        // 创建图片目录
        let app_data_dir = std::env::current_dir()
            .map_err(|e| AppError::file_system(format!("获取当前目录失败: {}", e)))?
            .join("app_data")
            .join("images")
            .join("occlusion");

        std::fs::create_dir_all(&app_data_dir)
            .map_err(|e| AppError::file_system(format!("创建图片目录失败: {}", e)))?;

        // 生成文件名
        let file_name = format!("{}.jpg", Uuid::new_v4());
        let file_path = app_data_dir.join(&file_name);

        // 保存图片
        std::fs::write(&file_path, &image_data)
            .map_err(|e| AppError::file_system(format!("保存图片失败: {}", e)))?;

        Ok(file_path.to_string_lossy().to_string())
    }

    /// 保存卡片到数据库
    fn save_card_to_database(&self, card: &ImageOcclusionCard) -> Result<(), AppError> {
        self.db.save_image_occlusion_card(card)
            .map_err(|e| AppError::database(format!("保存图片遮罩卡失败: {}", e)))
    }

    /// 获取所有图片遮罩卡
    pub fn get_all_image_occlusion_cards(&self) -> Result<Vec<ImageOcclusionCard>, AppError> {
        self.db.get_all_image_occlusion_cards()
            .map_err(|e| AppError::database(format!("获取图片遮罩卡列表失败: {}", e)))
    }

    /// 根据ID获取图片遮罩卡
    pub fn get_image_occlusion_card(&self, card_id: &str) -> Result<Option<ImageOcclusionCard>, AppError> {
        self.db.get_image_occlusion_card_by_id(card_id)
            .map_err(|e| AppError::database(format!("获取图片遮罩卡失败: {}", e)))
    }

    /// 更新图片遮罩卡
    pub fn update_image_occlusion_card(&self, card: &ImageOcclusionCard) -> Result<(), AppError> {
        self.db.update_image_occlusion_card(card)
            .map_err(|e| AppError::database(format!("更新图片遮罩卡失败: {}", e)))
    }

    /// 删除图片遮罩卡
    pub fn delete_image_occlusion_card(&self, card_id: &str) -> Result<(), AppError> {
        self.db.delete_image_occlusion_card(card_id)
            .map_err(|e| AppError::database(format!("删除图片遮罩卡失败: {}", e)))
    }

    /// 从AI响应中提取JSON内容，处理markdown格式和额外文本
    fn extract_json_from_ai_response(&self, content: &str) -> Result<String, AppError> {
        let content = content.trim();
        
        // 方法1: 查找```json代码块
        if let Some(start) = content.find("```json") {
            let after_start = &content[start + 7..]; // 跳过```json
            if let Some(end) = after_start.find("```") {
                let json_part = after_start[..end].trim();
                return Ok(json_part.to_string());
            }
        }
        
        // 方法2: 查找普通```代码块（有时AI不会标记json）
        if let Some(start) = content.find("```") {
            let after_start = &content[start + 3..];
            if let Some(end) = after_start.find("```") {
                let potential_json = after_start[..end].trim();
                // 验证这是否看起来像JSON
                if potential_json.starts_with('{') && potential_json.ends_with('}') {
                    return Ok(potential_json.to_string());
                }
            }
        }
        
        // 方法3: 查找JSON对象（从第一个{到最后一个}）
        if let Some(start) = content.find('{') {
            if let Some(end) = content.rfind('}') {
                if start < end {
                    let json_part = &content[start..=end];
                    return Ok(json_part.to_string());
                }
            }
        }
        
        // 方法4: 尝试按行分割，查找以{开头的行
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('{') {
                // 从这行开始，收集到对应的}结束
                let from_here = &content[content.find(trimmed).unwrap()..];
                if let Some(start) = from_here.find('{') {
                    if let Some(end) = from_here.rfind('}') {
                        if start < end {
                            let json_part = &from_here[start..=end];
                            return Ok(json_part.to_string());
                        }
                    }
                }
            }
        }
        
        // 如果所有方法都失败，返回错误
        Err(AppError::validation(format!(
            "无法从AI响应中提取有效的JSON内容。响应内容: {}", 
            content
        )))
    }
}
