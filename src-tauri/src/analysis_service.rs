use crate::models::{ChatMessage, MistakeItem};
use crate::llm_manager::LLMManager;
use crate::database::Database;
use anyhow::Result;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::Window;

// 分析结果结构
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub ocr_text: String,
    pub tags: Vec<String>,
    pub mistake_type: String,
    pub first_answer: String,
}

pub struct AnalysisService {
    llm_manager: LLMManager,
    // 移除重复的temp_sessions，统一使用AppState中的会话管理
}

impl AnalysisService {
    pub fn new(database: Arc<Database>, file_manager: Arc<crate::file_manager::FileManager>) -> Self {
        Self {
            llm_manager: LLMManager::new(database, file_manager),
        }
    }

    // 分析错题（使用统一AI接口）- 流式版本
    pub async fn analyze_mistake_stream(
        &self,
        question_image_paths: &[String],
        user_question: &str,
        subject: &str,
        window: Window,
        stream_event: &str,
    ) -> Result<AnalysisResult> {
        println!("开始分析错题(流式): 科目={}, 图片数量={}", subject, question_image_paths.len());
        
        // 调用统一模型一接口进行OCR和分类（第一模型不使用流式，因为需要结构化输出）
        let model1_result = self.llm_manager.call_unified_model_1(
            question_image_paths.to_vec(),
            user_question,
            subject,
            None, // 暂时不使用任务上下文
        ).await.map_err(|e| anyhow::anyhow!("模型一调用失败: {}", e))?;

        // 构建上下文
        let mut context = HashMap::new();
        context.insert("ocr_text".to_string(), json!(model1_result.ocr_text));
        context.insert("tags".to_string(), json!(model1_result.tags));
        context.insert("mistake_type".to_string(), json!(model1_result.mistake_type));
        context.insert("user_question".to_string(), json!(user_question));

        // 获取模型配置以判断是否是推理模型
        let model_config = self.llm_manager.get_model2_config().await
            .map_err(|e| anyhow::anyhow!("获取模型配置失败: {}", e))?;
        
        // 推理模型自动启用思维链
        let enable_chain_of_thought = model_config.is_reasoning;

        // 调用统一模型二接口获取首次解答（流式）
        let model2_result = self.llm_manager.call_unified_model_2_stream(
            &context,
            &[], // 空的聊天历史
            subject,
            enable_chain_of_thought, // 推理模型自动启用思维链
            Some(question_image_paths.to_vec()), // 🎯 修复：传入图片路径给第二模型
            None, // 暂时不使用任务上下文
            window,
            stream_event,
        ).await.map_err(|e| anyhow::anyhow!("模型二调用失败: {}", e))?;

        Ok(AnalysisResult {
            ocr_text: model1_result.ocr_text,
            tags: model1_result.tags,
            mistake_type: model1_result.mistake_type,
            first_answer: model2_result.assistant_message,
        })
    }

    // 分析错题（使用统一AI接口）- 非流式版本（已废弃，统一使用流式）
    pub async fn analyze_mistake(
        &self,
        _question_image_paths: &[String],
        _user_question: &str,
        _subject: &str,
    ) -> Result<AnalysisResult> {
        println!("警告: analyze_mistake 非流式版本已废弃，请使用 analyze_mistake_stream");
        
        // 为了兼容性，创建一个虚拟的 Window 对象
        // 实际上这个函数不应该被调用
        return Err(anyhow::anyhow!("非流式版本已废弃，请使用流式版本"));
    }

    // 继续对话（使用统一AI接口）- 流式版本
    pub async fn continue_conversation_stream(
        &self,
        ocr_text: &str,
        tags: &[String],
        chat_history: &[ChatMessage],
        subject: &str,
        window: Window,
        stream_event: &str,
    ) -> Result<String> {
        println!("继续对话(流式): 科目={}, 聊天历史长度={}", subject, chat_history.len());
        
        // 构建上下文
        let mut context = HashMap::new();
        context.insert("ocr_text".to_string(), json!(ocr_text));
        context.insert("tags".to_string(), json!(tags));
        context.insert("subject".to_string(), json!(subject));

        // 获取模型配置以判断是否是推理模型
        let model_config = self.llm_manager.get_model2_config().await
            .map_err(|e| anyhow::anyhow!("获取模型配置失败: {}", e))?;
        
        // 推理模型自动启用思维链
        let enable_chain_of_thought = model_config.is_reasoning;

        // 调用统一模型二接口（流式）
        let model2_result = self.llm_manager.call_unified_model_2_stream(
            &context,
            chat_history,
            subject,
            enable_chain_of_thought, // 推理模型自动启用思维链
            None, // 继续对话时不传入图片
            None, // 暂时不使用任务上下文
            window,
            stream_event,
        ).await.map_err(|e| anyhow::anyhow!("模型二调用失败: {}", e))?;

        Ok(model2_result.assistant_message)
    }

    // 继续对话（使用统一AI接口）- 非流式版本（已废弃，统一使用流式）
    pub async fn continue_conversation(
        &self,
        _ocr_text: &str,
        _tags: &[String],
        _chat_history: &[ChatMessage],
        _subject: &str,
    ) -> Result<String> {
        println!("警告: continue_conversation 非流式版本已废弃，请使用 continue_conversation_stream");
        
        // 为了兼容性，返回错误
        return Err(anyhow::anyhow!("非流式版本已废弃，请使用流式版本"));
    }

    // 回顾分析（使用统一AI接口）- 流式版本
    pub async fn analyze_review_session_stream(
        &self,
        mistakes: &[MistakeItem],
        subject: &str,
        window: Window,
        stream_event: &str,
    ) -> Result<String> {
        println!("开始回顾分析(流式): 科目={}, 错题数量={}", subject, mistakes.len());
        
        // 构建回顾分析的上下文
        let mut context = HashMap::new();
        context.insert("subject".to_string(), json!(subject));
        context.insert("mistake_count".to_string(), json!(mistakes.len()));

        // 收集所有错题的信息
        let mut mistake_summaries = Vec::new();
        for (index, mistake) in mistakes.iter().enumerate() {
            let summary = json!({
                "index": index + 1,
                "question": mistake.user_question,
                "ocr_text": mistake.ocr_text,
                "tags": mistake.tags,
                "mistake_type": mistake.mistake_type,
                "created_at": mistake.created_at.format("%Y-%m-%d").to_string()
            });
            mistake_summaries.push(summary);
        }
        context.insert("mistakes".to_string(), json!(mistake_summaries));

        // 🎯 修复BUG-04：获取回顾分析专用模型配置
        let model_assignments = self.llm_manager.get_model_assignments().await
            .map_err(|e| anyhow::anyhow!("获取模型分配失败: {}", e))?;

        // 优先使用回顾分析专用模型，如果未配置则回退到第二模型
        let target_model_id = model_assignments.review_analysis_model_config_id
            .or(model_assignments.model2_config_id)
            .ok_or_else(|| anyhow::anyhow!("没有配置可用的回顾分析模型或第二模型"))?;

        // 获取目标模型配置
        let api_configs = self.llm_manager.get_api_configs().await
            .map_err(|e| anyhow::anyhow!("获取API配置失败: {}", e))?;

        let model_config = api_configs.iter()
            .find(|config| config.id == target_model_id && config.enabled)
            .ok_or_else(|| anyhow::anyhow!("找不到可用的回顾分析模型配置: {}", target_model_id))?;

        println!("📋 回顾分析使用模型: {} ({})", model_config.name, model_config.model);

        // 推理模型自动启用思维链，回顾分析特别需要深度思考
        let enable_chain_of_thought = model_config.is_reasoning || true; // 回顾分析总是启用思维链

        // 调用统一模型接口进行回顾分析（流式）
        // 使用默认的回顾分析任务上下文（科目配置的提示词已经在 LLMManager 中处理）
        let task_context = "多道错题的回顾分析和学习建议";

        let model2_result = self.llm_manager.call_unified_model_stream_with_config(
            model_config,
            &context,
            &[], // 回顾分析不需要聊天历史
            subject,
            enable_chain_of_thought, // 回顾分析启用思维链
            None, // 回顾分析不传入图片
            Some(task_context), // 使用任务上下文
            window,
            stream_event,
        ).await.map_err(|e| anyhow::anyhow!("回顾分析失败: {}", e))?;

        Ok(model2_result.assistant_message)
    }

    // 回顾分析（使用统一AI接口）- 非流式版本（已废弃，统一使用流式）
    pub async fn analyze_review_session(
        &self,
        _mistakes: &[MistakeItem],
        _subject: &str,
    ) -> Result<String> {
        println!("警告: analyze_review_session 非流式版本已废弃，请使用 analyze_review_session_stream");
        
        // 为了兼容性，返回错误
        return Err(anyhow::anyhow!("非流式版本已废弃，请使用流式版本"));
    }

    // 测试API连接
    pub async fn test_connection(&self, api_key: &str, api_base: &str) -> Result<bool> {
        // 使用现有的LLM管理器进行测试
        self.llm_manager.test_connection(api_key, api_base).await
            .map_err(|e| anyhow::anyhow!("API连接测试失败: {}", e))
    }
    
    // 获取初始解答（使用统一AI接口）
    pub async fn get_initial_answer(
        &self,
        ocr_text: &str,
        tags: &[String],
        user_question: &str,
        subject: &str,
    ) -> Result<String> {
        println!("获取初始解答: 科目={}", subject);
        
        // 构建上下文
        let mut context = HashMap::new();
        context.insert("ocr_text".to_string(), json!(ocr_text));
        context.insert("tags".to_string(), json!(tags));
        context.insert("user_question".to_string(), json!(user_question));

        // 调用统一模型二接口获取首次解答
        let model2_result = self.llm_manager.call_unified_model_2(
            &context,
            &[], // 空的聊天历史
            subject,
            false, // 初始解答默认不启用思维链
            None, // 不传入图片
            Some("提供题目的初始解答"), // 任务上下文
        ).await.map_err(|e| anyhow::anyhow!("获取初始解答失败: {}", e))?;

        Ok(model2_result.assistant_message)
    }
}
