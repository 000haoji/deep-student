use crate::models::{
    DocumentTask, TaskStatus, AnkiCard, AnkiGenerationOptions, AppError, StreamedCardPayload,
    StreamEvent, AnkiDocumentGenerationRequest
};
use crate::database::Database;
use crate::llm_manager::LLMManager;
use crate::document_processing_service::DocumentProcessingService;
use crate::streaming_anki_service::StreamingAnkiService;
use std::sync::Arc;
use tauri::{Window, Emitter};
use tokio::task::JoinHandle;

pub struct EnhancedAnkiService {
    db: Arc<Database>,
    doc_processor: DocumentProcessingService,
    streaming_service: StreamingAnkiService,
}

impl EnhancedAnkiService {
    pub fn new(db: Arc<Database>, llm_manager: Arc<LLMManager>) -> Self {
        let doc_processor = DocumentProcessingService::new(db.clone());
        let streaming_service = StreamingAnkiService::new(db.clone(), llm_manager);
        
        Self {
            db,
            doc_processor,
            streaming_service,
        }
    }

    /// 开始文档处理 - 主要入口点
    pub async fn start_document_processing(
        &self,
        request: AnkiDocumentGenerationRequest,
        window: Window,
    ) -> Result<String, AppError> {
        let document_content = request.document_content;
        let subject_name = request.subject_name;
        let options = request.options.unwrap_or_else(|| AnkiGenerationOptions {
            deck_name: "默认牌组".to_string(),
            note_type: "Basic".to_string(),
            enable_images: false,
            max_cards_per_mistake: 10,
            max_tokens: None,
            temperature: None,
            max_output_tokens_override: None,
            temperature_override: None,
            template_id: None,
            custom_anki_prompt: None,
            template_fields: None,
            field_extraction_rules: None,
            custom_requirements: None,
            segment_overlap_size: 200,
            system_prompt: None,
        });

        // 确定文档名称
        let document_name = format!("文档_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S"));

        // 创建分段任务
        let (document_id, tasks) = self.doc_processor
            .process_document_and_create_tasks(
                document_content,
                document_name,
                subject_name,
                options,
            )
            .await?;

        // 发送文档处理开始事件
        let start_event = StreamEvent {
            payload: StreamedCardPayload::DocumentProcessingStarted {
                document_id: document_id.clone(),
                total_segments: tasks.len() as u32,
            },
        };

        if let Err(e) = window.emit("anki_generation_event", &start_event) {
            println!("发送文档处理开始事件失败: {}", e);
        }

        // 异步处理所有任务
        let window_clone = window.clone();
        let streaming_service = Arc::new(self.streaming_service.clone());
        let document_id_clone = document_id.clone();
        
        tokio::spawn(async move {
            Self::process_all_tasks_async(streaming_service, tasks, window_clone, document_id_clone).await;
        });

        Ok(document_id)
    }

    /// 异步处理所有任务
    async fn process_all_tasks_async(
        streaming_service: Arc<StreamingAnkiService>,
        tasks: Vec<DocumentTask>,
        window: Window,
        document_id: String,
    ) {
        let mut task_handles: Vec<JoinHandle<()>> = Vec::new();

        // 顺序处理任务以避免API限制
        for task in tasks {
            let service = streaming_service.clone();
            let window_clone = window.clone();
            
            let handle = tokio::spawn(async move {
                if let Err(e) = service.process_task_and_generate_cards_stream(task, window_clone).await {
                    println!("任务处理失败: {}", e);
                }
            });
            
            task_handles.push(handle);
            
            // 等待当前任务完成再开始下一个，避免并发API调用
            if let Some(handle) = task_handles.last_mut() {
                let _ = handle.await;
            }
        }

        // 所有任务已在循环内按顺序等待完成，这里不需要再次等待。
        // for handle in task_handles {
        //     let _ = handle.await;
        // }

        // 发送文档处理完成事件
        let complete_event = StreamEvent {
            payload: StreamedCardPayload::DocumentProcessingCompleted {
                document_id,
            },
        };

        if let Err(e) = window.emit("anki_generation_event", &complete_event) {
            println!("发送文档处理完成事件失败: {}", e);
        }
    }

    /// 手动触发单个任务处理
    pub async fn trigger_task_processing(
        &self,
        task_id: String,
        window: Window,
    ) -> Result<(), AppError> {
        let task = self.doc_processor.get_task(&task_id)?;
        
        if task.status != TaskStatus::Pending {
            return Err(AppError::validation("任务状态不是待处理"));
        }

        let streaming_service = Arc::new(self.streaming_service.clone());
        let window_clone = window.clone();
        
        tokio::spawn(async move {
            if let Err(e) = streaming_service.process_task_and_generate_cards_stream(task, window_clone).await {
                println!("任务处理失败: {}", e);
            }
        });

        Ok(())
    }


    /// 获取文档任务列表
    pub fn get_document_tasks(&self, document_id: String) -> Result<Vec<DocumentTask>, AppError> {
        self.doc_processor.get_document_tasks(&document_id)
    }

    /// 获取任务的卡片列表
    pub fn get_task_cards(&self, task_id: String) -> Result<Vec<AnkiCard>, AppError> {
        self.db.get_cards_for_task(&task_id)
            .map_err(|e| AppError::database(format!("获取任务卡片失败: {}", e)))
    }

    /// 更新卡片
    pub fn update_anki_card(&self, card: AnkiCard) -> Result<(), AppError> {
        self.db.update_anki_card(&card)
            .map_err(|e| AppError::database(format!("更新卡片失败: {}", e)))
    }

    /// 删除卡片
    pub fn delete_anki_card(&self, card_id: String) -> Result<(), AppError> {
        self.db.delete_anki_card(&card_id)
            .map_err(|e| AppError::database(format!("删除卡片失败: {}", e)))
    }

    /// 删除任务
    pub fn delete_document_task(&self, task_id: String) -> Result<(), AppError> {
        self.db.delete_document_task(&task_id)
            .map_err(|e| AppError::database(format!("删除任务失败: {}", e)))
    }

    /// 删除文档会话
    pub fn delete_document_session(&self, document_id: String) -> Result<(), AppError> {
        self.db.delete_document_session(&document_id)
            .map_err(|e| AppError::database(format!("删除文档会话失败: {}", e)))
    }

    /// 导出选定内容为APKG
    pub async fn export_apkg_for_selection(
        &self,
        document_id: Option<String>,
        task_ids: Option<Vec<String>>,
        card_ids: Option<Vec<String>>,
        options: AnkiGenerationOptions,
    ) -> Result<String, AppError> {
        // 根据选择获取卡片
        let cards = if let Some(ids) = card_ids {
            self.db.get_cards_by_ids(&ids)
                .map_err(|e| AppError::database(format!("获取指定卡片失败: {}", e)))?
        } else if let Some(task_ids) = task_ids {
            let mut all_cards = Vec::new();
            for task_id in task_ids {
                let mut task_cards = self.db.get_cards_for_task(&task_id)
                    .map_err(|e| AppError::database(format!("获取任务卡片失败: {}", e)))?;
                all_cards.append(&mut task_cards);
            }
            all_cards
        } else if let Some(doc_id) = document_id {
            self.db.get_cards_for_document(&doc_id)
                .map_err(|e| AppError::database(format!("获取文档卡片失败: {}", e)))?
        } else {
            return Err(AppError::validation("必须指定要导出的内容"));
        };

        // 过滤掉错误卡片（除非用户明确要求包含）
        let valid_cards: Vec<AnkiCard> = cards.into_iter()
            .filter(|card| !card.is_error_card)
            .collect();

        if valid_cards.is_empty() {
            return Err(AppError::validation("没有有效的卡片可以导出"));
        }

        // 调用现有的APKG导出服务
        // 注意：这里需要将enhanced AnkiCard转换为原始AnkiCard格式
        let simple_cards: Vec<crate::models::AnkiCard> = valid_cards.into_iter()
            .map(|card| crate::models::AnkiCard {
                front: card.front,
                back: card.back,
                text: None, // enhanced_anki_service 中的卡片没有text字段
                tags: card.tags,
                images: card.images,
                id: card.id,
                task_id: card.task_id,
                is_error_card: card.is_error_card,
                error_content: card.error_content,
                created_at: card.created_at,
                updated_at: card.updated_at,
                extra_fields: std::collections::HashMap::new(),
                template_id: None,
            })
            .collect();

        // 使用现有的导出服务
        let output_path = std::env::temp_dir().join(format!("anki_export_{}.apkg", uuid::Uuid::new_v4()));
        crate::apkg_exporter_service::export_cards_to_apkg(
            simple_cards, 
            options.deck_name, 
            options.note_type, 
            output_path.clone()
        ).await
            .map_err(|e| AppError::file_system(format!("导出APKG失败: {}", e)))?;
        
        Ok(output_path.to_string_lossy().to_string())
    }
}
