use crate::models::{DocumentTask, TaskStatus, AnkiGenerationOptions, AppError};
use crate::database::Database;
use std::sync::Arc;
use uuid::Uuid;
use chrono::Utc;

pub struct DocumentProcessingService {
    db: Arc<Database>,
}

impl DocumentProcessingService {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// 处理文档并创建分段任务
    pub async fn process_document_and_create_tasks(
        &self,
        document_content: String,
        original_document_name: String,
        subject_name: String,
        options: AnkiGenerationOptions,
    ) -> Result<(String, Vec<DocumentTask>), AppError> {
        let document_id = Uuid::new_v4().to_string();
        
        // 分段文档
        let segments = self.segment_document(&document_content, &options)?;
        
        let mut tasks = Vec::new();
        let anki_options_json = serde_json::to_string(&options)
            .map_err(|e| AppError::validation(format!("序列化AnkiGenerationOptions失败: {}", e)))?;
        
        let now = Utc::now().to_rfc3339();
        
        for (index, segment) in segments.into_iter().enumerate() {
            let task = DocumentTask {
                id: Uuid::new_v4().to_string(),
                document_id: document_id.clone(),
                original_document_name: original_document_name.clone(),
                segment_index: index as u32,
                content_segment: segment,
                status: TaskStatus::Pending,
                created_at: now.clone(),
                updated_at: now.clone(),
                error_message: None,
                subject_name: subject_name.clone(),
                anki_generation_options_json: anki_options_json.clone(),
            };
            
            // 保存到数据库
            self.db.insert_document_task(&task)
                .map_err(|e| AppError::database(format!("保存文档任务失败: {}", e)))?;
            
            tasks.push(task);
        }
        
        Ok((document_id, tasks))
    }

    /// 文档分段逻辑
    fn segment_document(
        &self,
        content: &str,
        options: &AnkiGenerationOptions,
    ) -> Result<Vec<String>, AppError> {
        // 配置分段参数
        let max_tokens_per_segment = self.calculate_max_tokens_per_segment(options);
        let estimated_content_tokens = self.estimate_tokens(content);
        
        // 如果内容较短，不需要分段
        if estimated_content_tokens <= max_tokens_per_segment {
            return Ok(vec![content.to_string()]);
        }
        
        let overlap_size = options.segment_overlap_size as usize;
        println!("[DOCUMENT_DEBUG] 文档分段: 估计{}tokens，每段最大{}tokens，重叠区域{}字符", 
                estimated_content_tokens, max_tokens_per_segment, overlap_size);
        println!("[DOCUMENT_DEBUG] 用户设置: 每个主题最大卡片数={}, 总体令牌限制={:?}", 
                options.max_cards_per_mistake, options.max_tokens);
        
        // 使用重叠分段策略
        let segments = if overlap_size > 0 {
            self.segment_with_overlap(content, max_tokens_per_segment, overlap_size)?
        } else {
            self.segment_without_overlap(content, max_tokens_per_segment)?
        };
        
        println!("[DOCUMENT_DEBUG] 文档分段完成: {}个分段", segments.len());
        for (i, segment) in segments.iter().enumerate() {
            let segment_tokens = self.estimate_tokens(segment);
            println!("[DOCUMENT_DEBUG] 分段{}: {}字符, 估计{}tokens", i+1, segment.len(), segment_tokens);
        }
        Ok(segments)
    }

    /// 分割过长的段落
    fn split_long_paragraph(&self, paragraph: &str, max_tokens: usize) -> Result<Vec<String>, AppError> {
        // 按句子分割
        let sentences: Vec<&str> = paragraph.split_inclusive(&['.', '!', '?', '。', '！', '？'])
            .filter(|s| !s.trim().is_empty())
            .collect();
        
        let mut segments = Vec::new();
        let mut current_segment = String::new();
        let mut current_tokens = 0;
        
        for sentence in sentences {
            let sentence_tokens = self.estimate_tokens(sentence);
            
            // 如果单个句子就超过限制，按字符数强制分割
            if sentence_tokens > max_tokens {
                // 先保存当前分段
                if !current_segment.is_empty() {
                    segments.push(current_segment.trim().to_string());
                    current_segment.clear();
                    current_tokens = 0;
                }
                
                // 按字符数分割长句子
                let char_segments = self.split_by_characters(sentence, max_tokens);
                segments.extend(char_segments);
                continue;
            }
            
            if current_tokens + sentence_tokens > max_tokens && !current_segment.is_empty() {
                segments.push(current_segment.trim().to_string());
                current_segment = sentence.to_string();
                current_tokens = sentence_tokens;
            } else {
                current_segment.push_str(sentence);
                current_tokens += sentence_tokens;
            }
        }
        
        if !current_segment.is_empty() {
            segments.push(current_segment.trim().to_string());
        }
        
        Ok(segments)
    }

    /// 按字符数强制分割
    fn split_by_characters(&self, text: &str, max_tokens: usize) -> Vec<String> {
        // 粗略估计：1个token ≈ 1.5个中文字符 或 4个英文字符
        let max_chars = max_tokens * 2; // 保守估计
        let mut segments = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        
        let mut start = 0;
        while start < chars.len() {
            let end = std::cmp::min(start + max_chars, chars.len());
            let segment: String = chars[start..end].iter().collect();
            segments.push(segment);
            start = end;
        }
        
        segments
    }

    /// 估算文本的token数量
    fn estimate_tokens(&self, text: &str) -> usize {
        // 简单的token估算算法
        // 更精确的实现可以使用tiktoken_rs库
        let char_count = text.chars().count();
        let word_count = text.split_whitespace().count();
        
        // 中文字符 ≈ 1 token per character
        // 英文单词 ≈ 1.3 tokens per word
        // 标点符号和空格 ≈ 0.2 tokens each
        
        let chinese_chars = text.chars().filter(|c| {
            let code = *c as u32;
            (0x4E00..=0x9FFF).contains(&code) // 基本汉字范围
        }).count();
        
        let other_chars = char_count - chinese_chars;
        let estimated_tokens = chinese_chars + (word_count as f32 * 1.3) as usize + (other_chars as f32 * 0.2) as usize;
        
        std::cmp::max(estimated_tokens, char_count / 4) // 最少不低于字符数的1/4
    }

    /// 计算每个分段的最大token数
    fn calculate_max_tokens_per_segment(&self, options: &AnkiGenerationOptions) -> usize {
        // 根据API配置或选项确定分段大小
        // 默认值：2000 tokens per segment
        let base_limit = 2000;
        
        // 如果用户设置了较小的max_tokens，相应调整分段大小
        if let Some(max_tokens) = options.max_output_tokens_override.or(options.max_tokens.map(|t| t as u32)) {
            if max_tokens < 4000 {
                // 如果输出限制较小，输入也应该相应减少
                return std::cmp::min(base_limit, (max_tokens / 2) as usize);
            }
        }
        
        base_limit
    }

    /// 获取文档的所有任务
    pub fn get_document_tasks(&self, document_id: &str) -> Result<Vec<DocumentTask>, AppError> {
        self.db.get_tasks_for_document(document_id)
            .map_err(|e| AppError::database(format!("获取文档任务失败: {}", e)))
    }

    /// 更新任务状态
    pub fn update_task_status(
        &self,
        task_id: &str,
        status: TaskStatus,
        error_message: Option<String>,
    ) -> Result<(), AppError> {
        self.db.update_document_task_status(task_id, status, error_message)
            .map_err(|e| AppError::database(format!("更新任务状态失败: {}", e)))
    }

    /// 获取单个任务
    pub fn get_task(&self, task_id: &str) -> Result<DocumentTask, AppError> {
        self.db.get_document_task(task_id)
            .map_err(|e| AppError::database(format!("获取任务失败: {}", e)))
    }

    /// 删除文档及其所有任务
    pub fn delete_document(&self, document_id: &str) -> Result<(), AppError> {
        self.db.delete_document_session(document_id)
            .map_err(|e| AppError::database(format!("删除文档失败: {}", e)))
    }

    /// 无重叠的文档分段（原始逻辑）
    fn segment_without_overlap(
        &self,
        content: &str,
        max_tokens_per_segment: usize,
    ) -> Result<Vec<String>, AppError> {
        // 按自然段落分割
        let paragraphs: Vec<&str> = content.split("\n\n")
            .filter(|p| !p.trim().is_empty())
            .collect();
        
        let mut segments = Vec::new();
        let mut current_segment = String::new();
        let mut current_tokens = 0;
        
        for paragraph in paragraphs {
            let paragraph_tokens = self.estimate_tokens(paragraph);
            
            // 如果单个段落就超过限制，需要进一步分割
            if paragraph_tokens > max_tokens_per_segment {
                // 先保存当前分段（如果有内容）
                if !current_segment.is_empty() {
                    segments.push(current_segment.trim().to_string());
                    current_segment.clear();
                    current_tokens = 0;
                }
                
                // 分割长段落
                let sub_segments = self.split_long_paragraph(paragraph, max_tokens_per_segment)?;
                segments.extend(sub_segments);
                continue;
            }
            
            // 检查添加这个段落是否会超出限制
            if current_tokens + paragraph_tokens > max_tokens_per_segment && !current_segment.is_empty() {
                // 保存当前分段并开始新分段
                segments.push(current_segment.trim().to_string());
                current_segment = paragraph.to_string();
                current_tokens = paragraph_tokens;
            } else {
                // 添加到当前分段
                if !current_segment.is_empty() {
                    current_segment.push_str("\n\n");
                }
                current_segment.push_str(paragraph);
                current_tokens += paragraph_tokens;
            }
        }
        
        // 添加最后一个分段
        if !current_segment.is_empty() {
            segments.push(current_segment.trim().to_string());
        }
        
        // 确保至少有一个分段
        if segments.is_empty() {
            segments.push(content.to_string());
        }

        Ok(segments)
    }

    /// 带重叠的文档分段
    fn segment_with_overlap(
        &self,
        content: &str,
        max_tokens_per_segment: usize,
        overlap_size: usize,
    ) -> Result<Vec<String>, AppError> {
        // 首先进行无重叠分段
        let base_segments = self.segment_without_overlap(content, max_tokens_per_segment)?;
        
        // 如果只有一个分段，不需要重叠
        if base_segments.len() <= 1 {
            return Ok(base_segments);
        }

        println!("[DOCUMENT_DEBUG] 应用重叠策略，基础分段数: {}", base_segments.len());

        let mut overlapped_segments = Vec::new();
        
        for (i, segment) in base_segments.iter().enumerate() {
            let mut final_segment = segment.clone();
            
            // 为非第一个分段添加前重叠
            if i > 0 {
                let prev_segment = &base_segments[i - 1];
                if let Some(overlap_prefix) = self.get_overlap_suffix(prev_segment, overlap_size) {
                    final_segment = format!("{}\n\n{}", overlap_prefix, final_segment);
                    println!("[DOCUMENT_DEBUG] 分段{}添加前重叠: {}字符", i+1, overlap_prefix.len());
                }
            }
            
            // 为非最后一个分段添加后重叠
            if i < base_segments.len() - 1 {
                let next_segment = &base_segments[i + 1];
                if let Some(overlap_suffix) = self.get_overlap_prefix(next_segment, overlap_size) {
                    final_segment = format!("{}\n\n{}", final_segment, overlap_suffix);
                    println!("[DOCUMENT_DEBUG] 分段{}添加后重叠: {}字符", i+1, overlap_suffix.len());
                }
            }
            
            overlapped_segments.push(final_segment);
        }

        println!("[DOCUMENT_DEBUG] 重叠处理完成，最终分段数: {}", overlapped_segments.len());
        Ok(overlapped_segments)
    }

    /// 将字节索引转换为字符索引
    fn byte_index_to_char_index(&self, text: &str, byte_index: usize) -> usize {
        text.char_indices().take_while(|(i, _)| *i <= byte_index).count() - 1
    }

    /// 获取文本的前缀（用于重叠）
    fn get_overlap_prefix(&self, text: &str, max_chars: usize) -> Option<String> {
        let char_count = text.chars().count();
        if char_count <= max_chars {
            return Some(text.to_string());
        }
        
        // 安全地获取前缀（按字符数而非字节数）
        let prefix: String = text.chars().take(max_chars).collect();
        
        // 尝试在句子边界处截断
        if let Some(last_sentence_end_bytes) = prefix.rfind(&['.', '!', '?', '。', '！', '？'][..]) {
            let last_sentence_end_chars = self.byte_index_to_char_index(&prefix, last_sentence_end_bytes);
            if last_sentence_end_chars > max_chars / 2 { // 确保不会截断太多
                return Some(prefix.chars().take(last_sentence_end_chars + 1).collect());
            }
        }
        
        // 尝试在段落边界处截断
        if let Some(last_paragraph_end_bytes) = prefix.rfind("\n\n") {
            let last_paragraph_end_chars = self.byte_index_to_char_index(&prefix, last_paragraph_end_bytes);
            if last_paragraph_end_chars > max_chars / 2 {
                return Some(prefix.chars().take(last_paragraph_end_chars).collect());
            }
        }
        
        // 尝试在词边界处截断
        if let Some(last_space_bytes) = prefix.rfind(' ') {
            let last_space_chars = self.byte_index_to_char_index(&prefix, last_space_bytes);
            if last_space_chars > max_chars / 2 {
                return Some(prefix.chars().take(last_space_chars).collect());
            }
        }
        
        // 最后选择：直接返回安全截断的前缀
        Some(prefix)
    }

    /// 获取文本的后缀（用于重叠）
    fn get_overlap_suffix(&self, text: &str, max_chars: usize) -> Option<String> {
        let char_count = text.chars().count();
        if char_count <= max_chars {
            return Some(text.to_string());
        }
        
        // 安全地获取后缀（按字符数而非字节数）
        let suffix: String = text.chars().skip(char_count - max_chars).collect();
        
        // 尝试在句子边界处开始
        if let Some(first_sentence_start_bytes) = suffix.find(&['.', '!', '?', '。', '！', '？'][..]) {
            let first_sentence_start_chars = self.byte_index_to_char_index(&suffix, first_sentence_start_bytes);
            let remaining: String = suffix.chars().skip(first_sentence_start_chars + 1).collect();
            if remaining.chars().count() > max_chars / 2 { // 确保不会截断太多
                return Some(remaining.trim().to_string());
            }
        }
        
        // 尝试在段落边界处开始
        if let Some(first_paragraph_start_bytes) = suffix.find("\n\n") {
            let first_paragraph_start_chars = self.byte_index_to_char_index(&suffix, first_paragraph_start_bytes);
            let remaining: String = suffix.chars().skip(first_paragraph_start_chars + 2).collect();
            if remaining.chars().count() > max_chars / 2 {
                return Some(remaining.to_string());
            }
        }
        
        // 尝试在词边界处开始
        if let Some(first_space_bytes) = suffix.find(' ') {
            let first_space_chars = self.byte_index_to_char_index(&suffix, first_space_bytes);
            let remaining: String = suffix.chars().skip(first_space_chars + 1).collect();
            if remaining.chars().count() > max_chars / 2 {
                return Some(remaining.to_string());
            }
        }
        
        // 最后选择：直接返回安全截断的后缀
        Some(suffix)
    }
}