use crate::models::{
    DocumentChunk, DocumentChunkWithEmbedding, RetrievedChunk, RagQueryOptions, 
    KnowledgeBaseStatusPayload, DocumentProcessingStatus, DocumentProcessingStage, 
    RagQueryResponse, AppError
};
use crate::vector_store::{SqliteVectorStore, VectorStore};
use crate::llm_manager::LLMManager;
use crate::database::Database;
use crate::file_manager::FileManager;
use std::sync::Arc;
use std::collections::HashMap;
use uuid::Uuid;
use tauri::{Window, Emitter};
use serde_json::Value;
use base64::{Engine as _, engine::general_purpose};
use regex::Regex;

type Result<T> = std::result::Result<T, AppError>;

/// 文档分块策略
#[derive(Debug, Clone, PartialEq)]
pub enum ChunkingStrategy {
    FixedSize,
    Semantic,
}

/// 分块配置
#[derive(Debug, Clone)]
pub struct ChunkingConfig {
    pub strategy: ChunkingStrategy,
    pub chunk_size: usize,
    pub chunk_overlap: usize,
    pub min_chunk_size: usize,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            strategy: ChunkingStrategy::FixedSize,
            chunk_size: 512,
            chunk_overlap: 50,
            min_chunk_size: 20,
        }
    }
}

/// RAG管理器 - 协调整个RAG流程
pub struct RagManager {
    vector_store: SqliteVectorStore,
    llm_manager: Arc<LLMManager>,
    file_manager: Arc<FileManager>,
    database: Arc<Database>,
}

impl RagManager {
    /// 创建新的RAG管理器实例
    pub fn new(
        database: Arc<Database>, 
        llm_manager: Arc<LLMManager>, 
        file_manager: Arc<FileManager>
    ) -> Result<Self> {
        let vector_store = SqliteVectorStore::new(database.clone())?;
        
        Ok(Self {
            vector_store,
            llm_manager,
            file_manager,
            database,
        })
    }
    
    /// 添加文档到知识库
    pub async fn add_documents_to_knowledge_base(
        &self, 
        file_paths: Vec<String>, 
        window: Window
    ) -> Result<String> {
        self.add_documents_to_knowledge_base_with_library(file_paths, window, None).await
    }
    
    /// 添加文档到指定分库
    pub async fn add_documents_to_knowledge_base_with_library(
        &self, 
        file_paths: Vec<String>, 
        window: Window,
        sub_library_id: Option<String>
    ) -> Result<String> {
        println!("🚀 开始处理 {} 个文档到知识库", file_paths.len());
        
        let mut processed_documents = Vec::new();
        let mut total_chunks = 0;
        
        for (index, file_path) in file_paths.iter().enumerate() {
            let progress = (index as f32) / (file_paths.len() as f32);
            
            // 发送处理状态更新
            self.emit_processing_status(&window, "overall", "processing", progress, 
                &format!("正在处理文档 {}/{}", index + 1, file_paths.len())).await;
            
            match self.process_single_document_with_library(file_path, &window, sub_library_id.as_deref()).await {
                Ok(chunk_count) => {
                    total_chunks += chunk_count;
                    processed_documents.push(file_path.clone());
                    println!("✅ 文档处理完成: {} ({} 个块)", file_path, chunk_count);
                }
                Err(e) => {
                    println!("❌ 文档处理失败: {} - {}", file_path, e);
                    self.emit_processing_status(&window, "overall", "error", progress,
                        &format!("文档处理失败: {}", e)).await;
                }
            }
        }
        
        // 发送完成状态
        self.emit_processing_status(&window, "overall", "completed", 1.0,
            &format!("处理完成：{} 个文档，{} 个文本块", processed_documents.len(), total_chunks)).await;
        
        Ok(format!("成功处理 {} 个文档，共 {} 个文本块", processed_documents.len(), total_chunks))
    }
    
    /// 处理单个文档
    async fn process_single_document(&self, file_path: &str, window: &Window) -> Result<usize> {
        self.process_single_document_with_library(file_path, window, None).await
    }
    
    /// 处理单个文档到指定分库
    async fn process_single_document_with_library(&self, file_path: &str, window: &Window, sub_library_id: Option<&str>) -> Result<usize> {
        let document_id = Uuid::new_v4().to_string();
        let file_name = std::path::Path::new(file_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string();
        
        println!("📄 开始处理文档: {} (ID: {})", file_name, document_id);
        
        // 1. 读取文件内容
        self.emit_document_status(window, &document_id, &file_name, DocumentProcessingStage::Reading, 0.1).await;
        let content = self.read_file_content(file_path).await?;
        
        // 2. 预处理
        self.emit_document_status(window, &document_id, &file_name, DocumentProcessingStage::Preprocessing, 0.2).await;
        let processed_content = self.preprocess_content(&content);
        
        // 3. 文本分块
        self.emit_document_status(window, &document_id, &file_name, DocumentProcessingStage::Chunking, 0.3).await;
        let chunks = self.chunk_text_with_progress(&document_id, &processed_content, &file_name, Some(window), &document_id, &file_name).await?;
        
        // 4. 生成向量嵌入
        let chunks_with_embeddings = self.generate_embeddings_for_chunks_with_progress(chunks, Some(window), &document_id, &file_name).await?;
        
        // 5. 存储到向量数据库
        self.emit_document_status(window, &document_id, &file_name, DocumentProcessingStage::Storing, 0.8).await;
        self.vector_store.add_chunks(chunks_with_embeddings.clone()).await?;
        
        // 6. 更新文档记录  
        let target_library_id = sub_library_id.unwrap_or("default");
        self.vector_store.add_document_record_with_library(&document_id, &file_name, Some(file_path), None, target_library_id)?;
        self.vector_store.update_document_chunk_count(&document_id, chunks_with_embeddings.len())?;
        
        // 7. 完成
        self.emit_document_status(window, &document_id, &file_name, DocumentProcessingStage::Completed, 1.0).await;
        
        Ok(chunks_with_embeddings.len())
    }
    
    /// 从文件内容添加文档到知识库
    pub async fn add_documents_from_content(
        &self, 
        documents: Vec<serde_json::Value>, 
        window: Window
    ) -> Result<String> {
        println!("🚀 开始从内容处理 {} 个文档到知识库", documents.len());
        
        let mut processed_documents = Vec::new();
        let mut total_chunks = 0;
        
        for (index, doc_data) in documents.iter().enumerate() {
            let progress = (index as f32) / (documents.len() as f32);
            
            // 解析文档数据
            let file_name = doc_data.get("fileName")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            
            let content = doc_data.get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            
            if content.is_empty() {
                println!("⚠️ 跳过空内容文档: {}", file_name);
                continue;
            }
            
            // 发送处理状态更新
            self.emit_processing_status(&window, "overall", "processing", progress, 
                &format!("正在处理文档 {}/{}", index + 1, documents.len())).await;
            
            match self.process_document_content(&file_name, content, &window).await {
                Ok(chunk_count) => {
                    total_chunks += chunk_count;
                    processed_documents.push(file_name.clone());
                    println!("✅ 文档内容处理完成: {} ({} 个块)", file_name, chunk_count);
                }
                Err(e) => {
                    println!("❌ 文档内容处理失败: {} - {}", file_name, e);
                    self.emit_processing_status(&window, "overall", "error", progress,
                        &format!("文档处理失败: {}", e)).await;
                }
            }
        }
        
        // 发送完成状态
        self.emit_processing_status(&window, "overall", "completed", 1.0,
            &format!("处理完成：{} 个文档，{} 个文本块", processed_documents.len(), total_chunks)).await;
        
        Ok(format!("成功处理 {} 个文档，共 {} 个文本块", processed_documents.len(), total_chunks))
    }
    
    /// 从文件内容添加文档到指定分库
    pub async fn add_documents_from_content_to_library(
        &self, 
        documents: Vec<serde_json::Value>, 
        window: Window,
        sub_library_id: Option<String>
    ) -> Result<String> {
        println!("🚀 开始从内容处理 {} 个文档到分库: {:?}", documents.len(), sub_library_id);
        
        let mut processed_documents = Vec::new();
        let mut total_chunks = 0;
        
        for (index, doc_data) in documents.iter().enumerate() {
            let progress = (index as f32) / (documents.len() as f32);
            
            // 解析文档数据
            let file_name = doc_data.get("fileName")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            
            let content = doc_data.get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            
            if content.is_empty() {
                println!("⚠️ 跳过空内容文档: {}", file_name);
                continue;
            }
            
            // 发送处理状态更新
            self.emit_processing_status(&window, "overall", "processing", progress, 
                &format!("正在处理文档 {}/{}", index + 1, documents.len())).await;
            
            match self.process_document_content_with_library(&file_name, content, &window, sub_library_id.as_deref()).await {
                Ok(chunk_count) => {
                    total_chunks += chunk_count;
                    processed_documents.push(file_name.clone());
                    println!("✅ 文档内容处理完成: {} ({} 个块)", file_name, chunk_count);
                }
                Err(e) => {
                    println!("❌ 文档内容处理失败: {} - {}", file_name, e);
                    self.emit_processing_status(&window, "overall", "error", progress,
                        &format!("文档处理失败: {}", e)).await;
                }
            }
        }
        
        // 发送完成状态
        self.emit_processing_status(&window, "overall", "completed", 1.0,
            &format!("处理完成：{} 个文档，{} 个文本块", processed_documents.len(), total_chunks)).await;
        
        Ok(format!("成功处理 {} 个文档，共 {} 个文本块", processed_documents.len(), total_chunks))
    }
    
    /// 处理文档内容（不从文件路径读取，直接使用提供的内容）
    async fn process_document_content(&self, file_name: &str, content: &str, window: &Window) -> Result<usize> {
        let document_id = Uuid::new_v4().to_string();
        
        println!("📄 开始处理文档内容: {} (ID: {})", file_name, document_id);
        
        // 发送文档开始处理状态
        self.emit_document_status(window, &document_id, file_name, DocumentProcessingStage::Reading, 0.1).await;
        
        // 根据文件扩展名判断是否需要解码base64
        let processed_content = if file_name.ends_with(".txt") || file_name.ends_with(".md") {
            // 文本文件，直接使用内容
            println!("📝 处理文本文件: {}", file_name);
            content.to_string()
        } else {
            // 二进制文件，假设是base64编码，需要解码后处理
            println!("🔄 开始解码Base64内容: {} 字节", content.len());
            match general_purpose::STANDARD.decode(content) {
                Ok(decoded_bytes) => {
                    println!("✅ Base64解码成功，解码后大小: {} 字节", decoded_bytes.len());
                    // 根据文件扩展名处理二进制文件
                    if file_name.ends_with(".pdf") {
                        println!("📄 开始解析PDF文件: {}", file_name);
                        self.extract_pdf_text_from_memory(&decoded_bytes).await?
                    } else if file_name.ends_with(".docx") {
                        println!("📄 开始解析DOCX文件: {}", file_name);
                        self.extract_docx_text_from_memory(&decoded_bytes).await?
                    } else {
                        return Err(AppError::validation(format!(
                            "不支持的文件格式: {}", 
                            file_name
                        )));
                    }
                }
                Err(_) => {
                    // 不是有效的base64，当作普通文本处理
                    println!("⚠️ Base64解码失败，当作普通文本处理: {}", file_name);
                    content.to_string()
                }
            }
        };
        
        println!("📊 文档解析完成，提取文本长度: {} 字符", processed_content.len());
        
        // 预处理内容
        self.emit_document_status(window, &document_id, file_name, DocumentProcessingStage::Preprocessing, 0.2).await;
        println!("🔧 开始预处理文档内容: {}", file_name);
        let preprocessed_content = self.preprocess_content(&processed_content);
        println!("✅ 预处理完成，处理后长度: {} 字符", preprocessed_content.len());
        
        // 分块
        self.emit_document_status(window, &document_id, file_name, DocumentProcessingStage::Chunking, 0.3).await;
        println!("✂️ 开始文本分块: {}", file_name);
        let chunks = self.chunk_text_with_progress(&document_id, &preprocessed_content, file_name, Some(window), &document_id, file_name).await?;
        println!("✅ 分块完成，生成 {} 个文本块", chunks.len());
        
        if chunks.is_empty() {
            return Err(AppError::validation("文档内容为空或无法分块"));
        }
        
        // 生成嵌入
        println!("🧠 开始生成向量嵌入: {} 个文本块", chunks.len());
        let chunks_with_embeddings = self.generate_embeddings_for_chunks_with_progress(chunks, Some(window), &document_id, file_name).await?;
        println!("✅ 向量嵌入生成完成: {} 个向量", chunks_with_embeddings.len());
        
        // 记录文档信息
        let file_size = processed_content.len() as u64;
        self.vector_store.add_document_record(&document_id, file_name, None, Some(file_size))?;
        
        // 存储到向量数据库
        self.emit_document_status(window, &document_id, file_name, DocumentProcessingStage::Storing, 0.8).await;
        self.vector_store.add_chunks(chunks_with_embeddings.clone()).await?;
        
        // 更新文档块数统计
        self.vector_store.update_document_chunk_count(&document_id, chunks_with_embeddings.len())?;
        
        // 发送完成状态
        self.emit_document_status(window, &document_id, file_name, DocumentProcessingStage::Completed, 1.0).await;
        println!("🎉 文档处理完全完成: {} ({} 个文本块)", file_name, chunks_with_embeddings.len());
        
        Ok(chunks_with_embeddings.len())
    }
    
    /// 处理文档内容到指定分库（不从文件路径读取，直接使用提供的内容）
    async fn process_document_content_with_library(&self, file_name: &str, content: &str, window: &Window, sub_library_id: Option<&str>) -> Result<usize> {
        let document_id = Uuid::new_v4().to_string();
        
        println!("📄 开始处理文档内容到分库: {} (ID: {}, 分库: {:?})", file_name, document_id, sub_library_id);
        println!("📊 文档原始大小: {} 字节", content.len());
        
        // 发送文档开始处理状态
        self.emit_document_status(window, &document_id, file_name, DocumentProcessingStage::Reading, 0.1).await;
        
        // 根据文件扩展名判断是否需要解码base64
        println!("🔍 开始解析文档内容: {}", file_name);
        let processed_content = if file_name.ends_with(".txt") || file_name.ends_with(".md") {
            // 文本文件，直接使用内容
            println!("📝 处理文本文件: {}", file_name);
            content.to_string()
        } else {
            // 二进制文件，假设是base64编码，需要解码后处理
            println!("🔄 开始解码Base64内容: {} 字节", content.len());
            match general_purpose::STANDARD.decode(content) {
                Ok(decoded_bytes) => {
                    println!("✅ Base64解码成功，解码后大小: {} 字节", decoded_bytes.len());
                    // 根据文件扩展名处理二进制文件
                    if file_name.ends_with(".pdf") {
                        println!("📄 开始解析PDF文件: {}", file_name);
                        self.extract_pdf_text_from_memory(&decoded_bytes).await?
                    } else if file_name.ends_with(".docx") {
                        println!("📄 开始解析DOCX文件: {}", file_name);
                        self.extract_docx_text_from_memory(&decoded_bytes).await?
                    } else {
                        return Err(AppError::validation(format!(
                            "不支持的文件格式: {}", 
                            file_name
                        )));
                    }
                }
                Err(_) => {
                    // 不是有效的base64，当作普通文本处理
                    println!("⚠️ Base64解码失败，当作普通文本处理: {}", file_name);
                    content.to_string()
                }
            }
        };
        
        println!("📊 文档解析完成，提取文本长度: {} 字符", processed_content.len());
        
        // 预处理内容
        self.emit_document_status(window, &document_id, file_name, DocumentProcessingStage::Preprocessing, 0.2).await;
        println!("🔧 开始预处理文档内容: {}", file_name);
        let preprocessed_content = self.preprocess_content(&processed_content);
        println!("✅ 预处理完成，处理后长度: {} 字符", preprocessed_content.len());
        
        // 分块
        self.emit_document_status(window, &document_id, file_name, DocumentProcessingStage::Chunking, 0.3).await;
        println!("✂️ 开始文本分块: {}", file_name);
        let chunks = self.chunk_text_with_progress(&document_id, &preprocessed_content, file_name, Some(window), &document_id, file_name).await?;
        println!("✅ 分块完成，生成 {} 个文本块", chunks.len());
        
        if chunks.is_empty() {
            return Err(AppError::validation("文档内容为空或无法分块"));
        }
        
        // 生成嵌入
        println!("🧠 开始生成向量嵌入: {} 个文本块", chunks.len());
        let chunks_with_embeddings = self.generate_embeddings_for_chunks_with_progress(chunks, Some(window), &document_id, file_name).await?;
        println!("✅ 向量嵌入生成完成: {} 个向量", chunks_with_embeddings.len());
        
        // 记录文档信息到指定分库
        let file_size = processed_content.len() as u64;
        let target_library_id = sub_library_id.unwrap_or("default");
        println!("📋 添加文档记录到分库: {} -> {}", file_name, target_library_id);
        self.vector_store.add_document_record_with_library(&document_id, file_name, None, Some(file_size), target_library_id)?;
        
        // 存储到向量数据库
        self.emit_document_status(window, &document_id, file_name, DocumentProcessingStage::Storing, 0.8).await;
        println!("💾 开始存储到向量数据库: {} 个向量", chunks_with_embeddings.len());
        self.vector_store.add_chunks(chunks_with_embeddings.clone()).await?;
        println!("✅ 向量存储完成");
        
        // 更新文档块数统计
        println!("📊 更新文档统计信息");
        self.vector_store.update_document_chunk_count(&document_id, chunks_with_embeddings.len())?;
        
        // 发送完成状态
        self.emit_document_status(window, &document_id, file_name, DocumentProcessingStage::Completed, 1.0).await;
        println!("🎉 文档处理完全完成: {} ({} 个文本块)", file_name, chunks_with_embeddings.len());
        
        Ok(chunks_with_embeddings.len())
    }
    
    /// 读取文件内容
    async fn read_file_content(&self, file_path: &str) -> Result<String> {
        let path = std::path::Path::new(file_path);
        
        // 检查文件是否存在
        if !path.exists() {
            return Err(AppError::file_system(format!("读取文档文件失败: 系统找不到指定的文件。文件路径: {}", file_path)));
        }
        
        // 检查是否为文件（而非目录）
        if !path.is_file() {
            return Err(AppError::file_system(format!("读取文档文件失败: 指定路径不是文件。文件路径: {}", file_path)));
        }
        
        let extension = path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_lowercase();
        
        println!("📖 正在读取文件: {} (类型: {})", file_path, extension);
        
        match extension.as_str() {
            "txt" | "md" | "markdown" => {
                std::fs::read_to_string(file_path)
                    .map_err(|e| AppError::file_system(format!("读取文本文件失败: {} (文件路径: {})", e, file_path)))
            }
            "pdf" => {
                self.extract_pdf_text(file_path).await
            }
            "docx" => {
                self.extract_docx_text(file_path).await
            }
            _ => {
                Err(AppError::validation(format!("不支持的文件类型: {} (文件路径: {})", extension, file_path)))
            }
        }
    }
    
    /// 预处理文本内容
    fn preprocess_content(&self, content: &str) -> String {
        // 基础文本清理
        content
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
    
    /// 从数据库加载分块配置
    fn load_chunking_config(&self) -> Result<ChunkingConfig> {
        match self.database.get_rag_configuration() {
            Ok(Some(config)) => {
                let strategy = match config.chunking_strategy.as_str() {
                    "semantic" => ChunkingStrategy::Semantic,
                    _ => ChunkingStrategy::FixedSize,
                };
                
                Ok(ChunkingConfig {
                    strategy,
                    chunk_size: config.chunk_size as usize,
                    chunk_overlap: config.chunk_overlap as usize,
                    min_chunk_size: config.min_chunk_size as usize,
                })
            }
            Ok(None) => {
                // 没有配置时使用默认值
                Ok(ChunkingConfig::default())
            }
            Err(e) => {
                println!("⚠️ 无法加载RAG配置，使用默认值: {}", e);
                Ok(ChunkingConfig::default())
            }
        }
    }
    
    /// 将文本分块 - 支持不同策略
    fn chunk_text(&self, document_id: &str, content: &str, file_name: &str) -> Result<Vec<DocumentChunk>> {
        // 从数据库加载分块配置
        let config = self.load_chunking_config()?;
        
        match config.strategy {
            ChunkingStrategy::FixedSize => {
                self.chunk_text_fixed_size(document_id, content, file_name, &config)
            }
            ChunkingStrategy::Semantic => {
                self.chunk_text_semantic(document_id, content, file_name, &config)
            }
        }
    }
    
    /// 将文本分块并发送进度更新
    async fn chunk_text_with_progress(&self, document_id: &str, content: &str, file_name: &str, window: Option<&Window>, doc_id: &str, doc_name: &str) -> Result<Vec<DocumentChunk>> {
        // 从数据库加载分块配置
        let config = self.load_chunking_config()?;
        
        match config.strategy {
            ChunkingStrategy::FixedSize => {
                self.chunk_text_fixed_size_with_progress(document_id, content, file_name, &config, window, doc_id, doc_name).await
            }
            ChunkingStrategy::Semantic => {
                self.chunk_text_semantic_with_progress(document_id, content, file_name, &config, window, doc_id, doc_name).await
            }
        }
    }
    
    /// 固定大小分块策略
    fn chunk_text_fixed_size(&self, document_id: &str, content: &str, file_name: &str, config: &ChunkingConfig) -> Result<Vec<DocumentChunk>> {
        let mut chunks = Vec::new();
        let chars: Vec<char> = content.chars().collect();
        let mut start = 0;
        let mut chunk_index = 0;
        
        while start < chars.len() {
            let end = std::cmp::min(start + config.chunk_size, chars.len());
            let chunk_text: String = chars[start..end].iter().collect();
            
            // 跳过过短的块
            if chunk_text.trim().len() < config.min_chunk_size {
                start = end;
                continue;
            }
            
            let mut metadata = HashMap::new();
            metadata.insert("file_name".to_string(), file_name.to_string());
            metadata.insert("chunk_index".to_string(), chunk_index.to_string());
            metadata.insert("start_pos".to_string(), start.to_string());
            metadata.insert("end_pos".to_string(), end.to_string());
            metadata.insert("chunking_strategy".to_string(), "fixed_size".to_string());
            
            let chunk = DocumentChunk {
                id: Uuid::new_v4().to_string(),
                document_id: document_id.to_string(),
                chunk_index,
                text: chunk_text.trim().to_string(),
                metadata,
            };
            
            chunks.push(chunk);
            chunk_index += 1;
            
            // 计算下一个起始位置，考虑重叠
            start = if end == chars.len() { 
                end 
            } else { 
                std::cmp::max(start + 1, end - config.chunk_overlap) 
            };
        }
        
        println!("📝 固定大小分块完成: {} 个块", chunks.len());
        Ok(chunks)
    }
    
    /// 固定大小分块策略（带进度更新）
    async fn chunk_text_fixed_size_with_progress(&self, document_id: &str, content: &str, file_name: &str, config: &ChunkingConfig, window: Option<&Window>, doc_id: &str, doc_name: &str) -> Result<Vec<DocumentChunk>> {
        let mut chunks = Vec::new();
        let chars: Vec<char> = content.chars().collect();
        let mut start = 0;
        let mut chunk_index = 0;
        
        // 估算总的分块数量（用于进度计算）
        let estimated_total_chunks = (chars.len() / config.chunk_size) + 1;
        
        while start < chars.len() {
            let end = std::cmp::min(start + config.chunk_size, chars.len());
            let chunk_text: String = chars[start..end].iter().collect();
            
            // 跳过过短的块
            if chunk_text.trim().len() < config.min_chunk_size {
                start = end;
                continue;
            }
            
            let mut metadata = HashMap::new();
            metadata.insert("file_name".to_string(), file_name.to_string());
            metadata.insert("chunk_index".to_string(), chunk_index.to_string());
            metadata.insert("start_pos".to_string(), start.to_string());
            metadata.insert("end_pos".to_string(), end.to_string());
            metadata.insert("chunking_strategy".to_string(), "fixed_size".to_string());
            
            let chunk = DocumentChunk {
                id: Uuid::new_v4().to_string(),
                document_id: document_id.to_string(),
                chunk_index,
                text: chunk_text.trim().to_string(),
                metadata,
            };
            
            chunks.push(chunk);
            chunk_index += 1;
            
            // 发送进度更新
            if let Some(w) = window {
                let progress = 0.3 + (chunk_index as f32 / estimated_total_chunks as f32) * 0.15; // 从30%到45%
                self.emit_document_status_with_chunks(w, doc_id, doc_name, DocumentProcessingStage::Chunking, progress, chunk_index, estimated_total_chunks).await;
            }
            
            // 计算下一个起始位置，考虑重叠
            start = if end == chars.len() { 
                end 
            } else { 
                std::cmp::max(start + 1, end - config.chunk_overlap) 
            };
        }
        
        // 发送分块完成状态
        if let Some(w) = window {
            self.emit_document_status_with_chunks(w, doc_id, doc_name, DocumentProcessingStage::Chunking, 0.45, chunks.len(), chunks.len()).await;
        }
        
        println!("📝 固定大小分块完成: {} 个块", chunks.len());
        Ok(chunks)
    }
    
    /// 语义分块策略
    fn chunk_text_semantic(&self, document_id: &str, content: &str, file_name: &str, config: &ChunkingConfig) -> Result<Vec<DocumentChunk>> {
        let mut chunks = Vec::new();
        let mut chunk_index = 0;
        
        // 1. 首先按段落分割
        let paragraphs = self.split_into_paragraphs(content);
        
        let mut current_chunk = String::new();
        let mut current_sentences = Vec::new();
        let mut paragraph_index = 0;
        
        for paragraph in paragraphs {
            if paragraph.trim().is_empty() {
                continue;
            }
            
            // 2. 将段落按句子分割
            let sentences = self.split_into_sentences(&paragraph);
            
            for sentence in sentences {
                let sentence = sentence.trim();
                if sentence.is_empty() {
                    continue;
                }
                
                // 检查当前块加上新句子是否超过目标大小
                let potential_chunk = if current_chunk.is_empty() {
                    sentence.to_string()
                } else {
                    format!("{} {}", current_chunk, sentence)
                };
                
                if potential_chunk.len() <= config.chunk_size {
                    // 可以添加到当前块
                    current_chunk = potential_chunk;
                    current_sentences.push(sentence.to_string());
                } else {
                    // 当前块已满，保存当前块并开始新块
                    if !current_chunk.is_empty() && current_chunk.len() >= config.min_chunk_size {
                        let chunk = self.create_semantic_chunk(
                            document_id,
                            &current_chunk,
                            file_name,
                            chunk_index,
                            paragraph_index,
                            &current_sentences,
                        );
                        chunks.push(chunk);
                        chunk_index += 1;
                        
                        // 实现重叠：保留最后1-2个句子作为新块的开始
                        let overlap_sentences = if current_sentences.len() > 2 {
                            current_sentences[current_sentences.len()-2..].to_vec()
                        } else if current_sentences.len() > 1 {
                            current_sentences[current_sentences.len()-1..].to_vec()
                        } else {
                            Vec::new()
                        };
                        
                        current_chunk = if overlap_sentences.is_empty() {
                            sentence.to_string()
                        } else {
                            format!("{} {}", overlap_sentences.join(" "), sentence)
                        };
                        current_sentences = overlap_sentences;
                        current_sentences.push(sentence.to_string());
                    } else {
                        // 如果当前句子本身就很长，直接作为一个块
                        if sentence.len() >= config.min_chunk_size {
                            let chunk = self.create_semantic_chunk(
                                document_id,
                                sentence,
                                file_name,
                                chunk_index,
                                paragraph_index,
                                &vec![sentence.to_string()],
                            );
                            chunks.push(chunk);
                            chunk_index += 1;
                        }
                        current_chunk.clear();
                        current_sentences.clear();
                    }
                }
            }
            paragraph_index += 1;
        }
        
        // 处理最后一个块
        if !current_chunk.is_empty() && current_chunk.len() >= config.min_chunk_size {
            let chunk = self.create_semantic_chunk(
                document_id,
                &current_chunk,
                file_name,
                chunk_index,
                paragraph_index,
                &current_sentences,
            );
            chunks.push(chunk);
        }
        
        println!("📝 语义分块完成: {} 个块", chunks.len());
        Ok(chunks)
    }
    
    /// 语义分块策略（带进度更新）
    async fn chunk_text_semantic_with_progress(&self, document_id: &str, content: &str, file_name: &str, config: &ChunkingConfig, window: Option<&Window>, doc_id: &str, doc_name: &str) -> Result<Vec<DocumentChunk>> {
        let mut chunks = Vec::new();
        let mut chunk_index = 0;
        
        // 1. 首先按段落分割
        let paragraphs = self.split_into_paragraphs(content);
        let total_paragraphs = paragraphs.len();
        
        let mut current_chunk = String::new();
        let mut current_sentences = Vec::new();
        let mut paragraph_index = 0;
        
        for paragraph in paragraphs {
            if paragraph.trim().is_empty() {
                paragraph_index += 1;
                continue;
            }
            
            // 2. 将段落按句子分割
            let sentences = self.split_into_sentences(&paragraph);
            
            for sentence in sentences {
                let sentence = sentence.trim();
                if sentence.is_empty() {
                    continue;
                }
                
                // 检查当前块加上新句子是否超过目标大小
                let potential_chunk = if current_chunk.is_empty() {
                    sentence.to_string()
                } else {
                    format!("{} {}", current_chunk, sentence)
                };
                
                if potential_chunk.len() <= config.chunk_size {
                    // 可以添加到当前块
                    current_chunk = potential_chunk;
                    current_sentences.push(sentence.to_string());
                } else {
                    // 当前块已满，保存当前块并开始新块
                    if !current_chunk.is_empty() && current_chunk.len() >= config.min_chunk_size {
                        let chunk = self.create_semantic_chunk(
                            document_id,
                            &current_chunk,
                            file_name,
                            chunk_index,
                            paragraph_index,
                            &current_sentences,
                        );
                        chunks.push(chunk);
                        chunk_index += 1;
                        
                        // 发送进度更新
                        if let Some(w) = window {
                            let progress = 0.3 + (paragraph_index as f32 / total_paragraphs as f32) * 0.15; // 从30%到45%
                            let estimated_total_chunks = (content.len() / config.chunk_size) + 1;
                            self.emit_document_status_with_chunks(w, doc_id, doc_name, DocumentProcessingStage::Chunking, progress, chunk_index, estimated_total_chunks).await;
                        }
                        
                        // 实现重叠：保留最后1-2个句子作为新块的开始
                        let overlap_sentences = if current_sentences.len() > 2 {
                            current_sentences[current_sentences.len()-2..].to_vec()
                        } else if current_sentences.len() > 1 {
                            current_sentences[current_sentences.len()-1..].to_vec()
                        } else {
                            Vec::new()
                        };
                        
                        current_chunk = if overlap_sentences.is_empty() {
                            sentence.to_string()
                        } else {
                            format!("{} {}", overlap_sentences.join(" "), sentence)
                        };
                        current_sentences = overlap_sentences;
                        current_sentences.push(sentence.to_string());
                    } else {
                        // 如果当前句子本身就很长，直接作为一个块
                        if sentence.len() >= config.min_chunk_size {
                            let chunk = self.create_semantic_chunk(
                                document_id,
                                sentence,
                                file_name,
                                chunk_index,
                                paragraph_index,
                                &vec![sentence.to_string()],
                            );
                            chunks.push(chunk);
                            chunk_index += 1;
                        }
                        current_chunk.clear();
                        current_sentences.clear();
                    }
                }
            }
            paragraph_index += 1;
        }
        
        // 处理最后一个块
        if !current_chunk.is_empty() && current_chunk.len() >= config.min_chunk_size {
            let chunk = self.create_semantic_chunk(
                document_id,
                &current_chunk,
                file_name,
                chunk_index,
                paragraph_index,
                &current_sentences,
            );
            chunks.push(chunk);
        }
        
        // 发送分块完成状态
        if let Some(w) = window {
            self.emit_document_status_with_chunks(w, doc_id, doc_name, DocumentProcessingStage::Chunking, 0.45, chunks.len(), chunks.len()).await;
        }
        
        println!("📝 语义分块完成: {} 个块", chunks.len());
        Ok(chunks)
    }
    
    /// 按段落分割文本
    fn split_into_paragraphs(&self, content: &str) -> Vec<String> {
        // 使用双换行符或多个换行符分割段落
        let paragraph_regex = Regex::new(r"\n\s*\n").unwrap();
        paragraph_regex.split(content)
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect()
    }
    
    /// 按句子分割文本
    fn split_into_sentences(&self, text: &str) -> Vec<String> {
        // 使用句号、问号、感叹号等标点符号分割句子
        let sentence_regex = Regex::new(r"[.!?。！？]+").unwrap();
        let mut sentences = Vec::new();
        let mut last_end = 0;
        
        for mat in sentence_regex.find_iter(text) {
            let sentence = text[last_end..mat.end()].trim();
            if !sentence.is_empty() {
                sentences.push(sentence.to_string());
            }
            last_end = mat.end();
        }
        
        // 处理最后一个句子（如果没有以标点结尾）
        if last_end < text.len() {
            let sentence = text[last_end..].trim();
            if !sentence.is_empty() {
                sentences.push(sentence.to_string());
            }
        }
        
        // 如果没有找到句子分隔符，将整个文本作为一个句子
        if sentences.is_empty() {
            sentences.push(text.trim().to_string());
        }
        
        sentences
    }
    
    /// 创建语义块
    fn create_semantic_chunk(
        &self,
        document_id: &str,
        content: &str,
        file_name: &str,
        chunk_index: usize,
        paragraph_index: usize,
        sentences: &[String],
    ) -> DocumentChunk {
        let mut metadata = HashMap::new();
        metadata.insert("file_name".to_string(), file_name.to_string());
        metadata.insert("chunk_index".to_string(), chunk_index.to_string());
        metadata.insert("paragraph_index".to_string(), paragraph_index.to_string());
        metadata.insert("sentence_count".to_string(), sentences.len().to_string());
        metadata.insert("chunking_strategy".to_string(), "semantic".to_string());
        
        DocumentChunk {
            id: Uuid::new_v4().to_string(),
            document_id: document_id.to_string(),
            chunk_index,
            text: content.to_string(),
            metadata,
        }
    }
    
    /// 为文本块生成向量嵌入
    async fn generate_embeddings_for_chunks(&self, chunks: Vec<DocumentChunk>) -> Result<Vec<DocumentChunkWithEmbedding>> {
        self.generate_embeddings_for_chunks_with_progress(chunks, None, "", "").await
    }
    
    /// 为文本块生成向量嵌入并发送进度更新
    async fn generate_embeddings_for_chunks_with_progress(
        &self, 
        chunks: Vec<DocumentChunk>, 
        window: Option<&Window>, 
        document_id: &str, 
        file_name: &str
    ) -> Result<Vec<DocumentChunkWithEmbedding>> {
        println!("🧠 开始为 {} 个文本块生成向量嵌入", chunks.len());
        let mut chunks_with_embeddings = Vec::new();
        
        let total_chunks = chunks.len();
        
        // 如果提供了window，发送初始状态并显示总块数
        if let Some(w) = window {
            self.emit_document_status_with_chunks(w, document_id, file_name, DocumentProcessingStage::Embedding, 0.0, 0, total_chunks).await;
        }
        
        for (index, chunk) in chunks.into_iter().enumerate() {
            if index % 10 == 0 || index == total_chunks - 1 {
                println!("📊 向量生成进度: {}/{} ({:.1}%)", index + 1, total_chunks, (index + 1) as f32 / total_chunks as f32 * 100.0);
            }
            
            println!("🔤 正在为文本块 {} 生成向量 (长度: {} 字符)", index + 1, chunk.text.len());
            
            // 获取嵌入模型配置
            let model_assignments = self.llm_manager.get_model_assignments().await
                .map_err(|e| AppError::configuration(format!("获取模型分配失败: {}", e)))?;
            
            let embedding_model_id = model_assignments.embedding_model_config_id
                .ok_or_else(|| AppError::configuration("未配置嵌入模型"))?;
            
            // 调用LLM管理器生成嵌入
            let embeddings = self.llm_manager.call_embedding_api(vec![chunk.text.clone()], &embedding_model_id).await
                .map_err(|e| AppError::llm(format!("生成嵌入向量失败: {}", e)))?;
            
            let embedding = embeddings.into_iter().next()
                .ok_or_else(|| AppError::llm("嵌入向量生成失败"))?;
            
            if index % 10 == 0 {
                println!("✅ 文本块 {} 向量生成完成 (维度: {})", index + 1, embedding.len());
            }
            
            chunks_with_embeddings.push(DocumentChunkWithEmbedding {
                chunk,
                embedding,
            });
            
            // 发送进度更新
            if let Some(w) = window {
                let progress = 0.5 + (index + 1) as f32 / total_chunks as f32 * 0.3; // 从50%到80%
                self.emit_document_status_with_chunks(w, document_id, file_name, DocumentProcessingStage::Embedding, progress, index + 1, total_chunks).await;
            }
        }
        
        println!("🎉 所有向量嵌入生成完成: {} 个向量", chunks_with_embeddings.len());
        Ok(chunks_with_embeddings)
    }
    
    /// 获取默认RAG查询选项
    pub fn get_default_rag_query_options(&self) -> RagQueryOptions {
        match self.database.get_rag_configuration() {
            Ok(Some(config)) => {
                RagQueryOptions {
                    top_k: config.default_top_k as usize,
                    enable_reranking: Some(config.default_rerank_enabled),
                }
            }
            _ => {
                // 使用默认值
                RagQueryOptions {
                    top_k: 5,
                    enable_reranking: Some(false),
                }
            }
        }
    }
    
    /// 查询知识库
    pub async fn query_knowledge_base(&self, user_query: &str, options: RagQueryOptions) -> Result<RagQueryResponse> {
        let start_time = std::time::Instant::now();
        
        println!("🔍 开始RAG查询: '{}' (top_k: {})", user_query, options.top_k);
        
        // 1. 生成查询向量
        let query_vector_start = std::time::Instant::now();
        
        let model_assignments = self.llm_manager.get_model_assignments().await
            .map_err(|e| AppError::configuration(format!("获取模型分配失败: {}", e)))?;
        
        let embedding_model_id = model_assignments.embedding_model_config_id
            .ok_or_else(|| AppError::configuration("未配置嵌入模型"))?;
        
        let query_embeddings = self.llm_manager.call_embedding_api(vec![user_query.to_string()], &embedding_model_id).await
            .map_err(|e| AppError::llm(format!("生成查询向量失败: {}", e)))?;
        
        let query_embedding = query_embeddings.into_iter().next()
            .ok_or_else(|| AppError::llm("未获取到查询向量"))?;
        
        let query_vector_time = query_vector_start.elapsed();
        
        // 2. 向量搜索
        let search_start = std::time::Instant::now();
        let mut retrieved_chunks = self.vector_store.search_similar_chunks(query_embedding, options.top_k).await?;
        let search_time = search_start.elapsed();
        
        // 3. 可选的重排序
        let reranking_time = if options.enable_reranking.unwrap_or(false) {
            let rerank_start = std::time::Instant::now();
            retrieved_chunks = self.rerank_chunks(user_query, retrieved_chunks).await?;
            Some(rerank_start.elapsed())
        } else {
            None
        };
        
        let total_time = start_time.elapsed();
        
        println!("✅ RAG查询完成: {} 个结果 (总耗时: {:?})", retrieved_chunks.len(), total_time);
        
        Ok(RagQueryResponse {
            retrieved_chunks,
            query_vector_time_ms: query_vector_time.as_millis() as u64,
            search_time_ms: search_time.as_millis() as u64,
            reranking_time_ms: reranking_time.map(|t| t.as_millis() as u64),
            total_time_ms: total_time.as_millis() as u64,
        })
    }
    
    /// 在指定分库中查询知识库
    pub async fn query_knowledge_base_in_libraries(&self, user_query: &str, options: RagQueryOptions, sub_library_ids: Option<Vec<String>>) -> Result<RagQueryResponse> {
        let start_time = std::time::Instant::now();
        
        let library_filter_msg = if let Some(ref lib_ids) = sub_library_ids {
            format!(" (分库: {:?})", lib_ids)
        } else {
            " (所有分库)".to_string()
        };
        
        println!("🔍 开始RAG查询: '{}' (top_k: {}){}", user_query, options.top_k, library_filter_msg);
        
        // 1. 生成查询向量
        let query_vector_start = std::time::Instant::now();
        
        let model_assignments = self.llm_manager.get_model_assignments().await
            .map_err(|e| AppError::configuration(format!("获取模型分配失败: {}", e)))?;
        
        let embedding_model_id = model_assignments.embedding_model_config_id
            .ok_or_else(|| AppError::configuration("未配置嵌入模型"))?;
        
        let query_embeddings = self.llm_manager.call_embedding_api(vec![user_query.to_string()], &embedding_model_id).await
            .map_err(|e| AppError::llm(format!("生成查询向量失败: {}", e)))?;
        
        if query_embeddings.is_empty() {
            return Err(AppError::llm("查询向量生成失败"));
        }
        
        let query_vector = query_embeddings.into_iter().next().unwrap();
        let query_vector_time = query_vector_start.elapsed();
        
        // 2. 在指定分库中检索相似文档块
        let search_start = std::time::Instant::now();
        let mut retrieved_chunks = self.vector_store.search_similar_chunks_in_libraries(query_vector, options.top_k, sub_library_ids).await?;
        let search_time = search_start.elapsed();
        
        // 3. 可选的重排序
        let reranking_time = if options.enable_reranking.unwrap_or(false) && !retrieved_chunks.is_empty() {
            let rerank_start = std::time::Instant::now();
            retrieved_chunks = self.rerank_chunks(user_query, retrieved_chunks).await?;
            Some(rerank_start.elapsed())
        } else {
            None
        };
        
        let total_time = start_time.elapsed();
        
        println!("✅ RAG查询完成: 返回 {} 个结果 (总耗时: {}ms){}", 
                retrieved_chunks.len(), total_time.as_millis(), library_filter_msg);
        
        Ok(RagQueryResponse {
            retrieved_chunks,
            query_vector_time_ms: query_vector_time.as_millis() as u64,
            search_time_ms: search_time.as_millis() as u64,
            reranking_time_ms: reranking_time.map(|t| t.as_millis() as u64),
            total_time_ms: total_time.as_millis() as u64,
        })
    }
    
    /// 重排序检索结果
    async fn rerank_chunks(&self, query: &str, chunks: Vec<RetrievedChunk>) -> Result<Vec<RetrievedChunk>> {
        println!("🔄 开始重排序 {} 个检索结果", chunks.len());
        
        let model_assignments = self.llm_manager.get_model_assignments().await
            .map_err(|e| AppError::configuration(format!("获取模型分配失败: {}", e)))?;
        
        if let Some(reranker_model_id) = model_assignments.reranker_model_config_id {
            // 调用重排序模型
            let reranked_chunks = self.llm_manager.call_reranker_api(query.to_string(), chunks, &reranker_model_id).await
                .map_err(|e| AppError::llm(format!("重排序失败: {}", e)))?;
            
            println!("✅ 重排序完成");
            Ok(reranked_chunks)
        } else {
            println!("⚠️ 未配置重排序模型，跳过重排序");
            Ok(chunks)
        }
    }
    
    /// 获取知识库状态
    pub async fn get_knowledge_base_status(&self) -> Result<KnowledgeBaseStatusPayload> {
        let stats = self.vector_store.get_stats().await?;
        
        // 获取当前嵌入模型名称
        let embedding_model_name = match self.llm_manager.get_model_assignments().await {
            Ok(assignments) => {
                if let Some(model_id) = assignments.embedding_model_config_id {
                    match self.llm_manager.get_api_configs().await {
                        Ok(configs) => {
                            configs.iter()
                                .find(|config| config.id == model_id)
                                .map(|config| config.name.clone())
                        }
                        Err(_) => None
                    }
                } else {
                    None
                }
            }
            Err(_) => None
        };
        
        Ok(KnowledgeBaseStatusPayload {
            total_documents: stats.total_documents,
            total_chunks: stats.total_chunks,
            embedding_model_name,
            vector_store_type: "SQLite".to_string(),
        })
    }
    
    /// 删除文档
    pub async fn delete_document_from_knowledge_base(&self, document_id: &str) -> Result<()> {
        println!("🗑️ 删除文档: {}", document_id);
        self.vector_store.delete_chunks_by_document_id(document_id).await
    }
    
    /// 清空知识库
    pub async fn clear_knowledge_base(&self) -> Result<()> {
        println!("🧹 清空知识库");
        self.vector_store.clear_all().await
    }
    
    /// 获取所有文档列表
    pub async fn get_all_documents(&self) -> Result<Vec<Value>> {
        self.vector_store.get_all_documents()
    }
    
    // 辅助方法：发送处理状态更新
    async fn emit_processing_status(&self, window: &Window, id: &str, status: &str, progress: f32, message: &str) {
        let payload = serde_json::json!({
            "id": id,
            "status": status,
            "progress": progress,
            "message": message,
            "timestamp": chrono::Utc::now().to_rfc3339()
        });
        
        if let Err(e) = window.emit("rag_processing_status", payload) {
            println!("⚠️ 发送处理状态失败: {}", e);
        }
    }
    
    // 辅助方法：发送文档处理状态
    async fn emit_document_status(&self, window: &Window, document_id: &str, file_name: &str, stage: DocumentProcessingStage, progress: f32) {
        let status = DocumentProcessingStatus {
            document_id: document_id.to_string(),
            file_name: file_name.to_string(),
            status: stage,
            progress,
            error_message: None,
            chunks_processed: 0,
            total_chunks: 0,
        };
        
        if let Err(e) = window.emit("rag_document_status", &status) {
            println!("⚠️ 发送文档状态失败: {}", e);
        }
    }
    
    // 辅助方法：发送带详细信息的文档处理状态
    async fn emit_document_status_with_chunks(&self, window: &Window, document_id: &str, file_name: &str, stage: DocumentProcessingStage, progress: f32, chunks_processed: usize, total_chunks: usize) {
        let status = DocumentProcessingStatus {
            document_id: document_id.to_string(),
            file_name: file_name.to_string(),
            status: stage,
            progress,
            error_message: None,
            chunks_processed,
            total_chunks,
        };
        
        if let Err(e) = window.emit("rag_document_status", &status) {
            println!("⚠️ 发送文档状态失败: {}", e);
        }
    }
    
    /// 提取PDF文件文本内容
    async fn extract_pdf_text(&self, file_path: &str) -> Result<String> {
        use pdf_extract::extract_text;
        
        println!("📕 开始提取PDF文本: {}", file_path);
        
        // 再次检查文件是否存在
        let path = std::path::Path::new(file_path);
        if !path.exists() {
            return Err(AppError::file_system(format!("PDF文件不存在: {}", file_path)));
        }
        
        let text = extract_text(file_path)
            .map_err(|e| AppError::validation(format!("PDF文本提取失败: {} (文件路径: {})", e, file_path)))?;
        
        if text.trim().is_empty() {
            return Err(AppError::validation(format!("PDF文件没有可提取的文本内容 (文件路径: {})", file_path)));
        }
        
        println!("✅ PDF文本提取完成，长度: {} 字符", text.len());
        Ok(text)
    }
    
    /// 提取DOCX文件文本内容
    async fn extract_docx_text(&self, file_path: &str) -> Result<String> {
        use docx_rs::*;
        
        println!("📘 开始提取DOCX文本: {}", file_path);
        
        // 再次检查文件是否存在
        let path = std::path::Path::new(file_path);
        if !path.exists() {
            return Err(AppError::file_system(format!("DOCX文件不存在: {}", file_path)));
        }
        
        let bytes = std::fs::read(file_path)
            .map_err(|e| AppError::file_system(format!("读取DOCX文件失败: {} (文件路径: {})", e, file_path)))?;
        
        let docx = read_docx(&bytes)
            .map_err(|e| AppError::validation(format!("DOCX文件解析失败: {} (文件路径: {})", e, file_path)))?;
        
        // 提取文档中的所有文本
        let mut text_content = String::new();
        
        // 遍历文档的所有段落
        for child in docx.document.children {
            match child {
                docx_rs::DocumentChild::Paragraph(paragraph) => {
                    for run in paragraph.children {
                        match run {
                            docx_rs::ParagraphChild::Run(run) => {
                                for run_child in run.children {
                                    if let docx_rs::RunChild::Text(text) = run_child {
                                        text_content.push_str(&text.text);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    text_content.push('\n'); // 段落结束添加换行
                }
                _ => {}
            }
        }
        
        if text_content.trim().is_empty() {
            return Err(AppError::validation(format!("DOCX文件没有可提取的文本内容 (文件路径: {})", file_path)));
        }
        
        println!("✅ DOCX文本提取完成，长度: {} 字符", text_content.len());
        Ok(text_content)
    }
    
    /// 从内存中的PDF字节数据提取文本
    async fn extract_pdf_text_from_memory(&self, pdf_bytes: &[u8]) -> Result<String> {
        println!("📄 开始解析PDF文件 (大小: {} 字节)", pdf_bytes.len());
        
        // 使用文档解析器处理PDF
        let parser = crate::document_parser::DocumentParser::new();
        println!("🔧 初始化PDF解析器");
        
        match parser.extract_text_from_bytes("document.pdf", pdf_bytes.to_vec()) {
            Ok(text) => {
                println!("✅ PDF解析成功，提取文本长度: {} 字符", text.len());
                if text.trim().is_empty() {
                    println!("⚠️ PDF文件解析结果为空");
                    Err(AppError::validation("PDF文件内容为空或无法解析"))
                } else {
                    Ok(text)
                }
            }
            Err(e) => {
                println!("❌ PDF解析失败: {}", e);
                Err(AppError::file_system(format!("PDF解析失败: {}", e)))
            }
        }
    }
    
    /// 从内存中的DOCX字节数据提取文本
    async fn extract_docx_text_from_memory(&self, docx_bytes: &[u8]) -> Result<String> {
        println!("📄 开始解析DOCX文件 (大小: {} 字节)", docx_bytes.len());
        
        // 使用文档解析器处理DOCX
        let parser = crate::document_parser::DocumentParser::new();
        println!("🔧 初始化DOCX解析器");
        
        match parser.extract_text_from_bytes("document.docx", docx_bytes.to_vec()) {
            Ok(text) => {
                println!("✅ DOCX解析成功，提取文本长度: {} 字符", text.len());
                if text.trim().is_empty() {
                    println!("⚠️ DOCX文件解析结果为空");
                    Err(AppError::validation("DOCX文件内容为空或无法解析"))
                } else {
                    Ok(text)
                }
            }
            Err(e) => {
                println!("❌ DOCX解析失败: {}", e);
                Err(AppError::file_system(format!("DOCX解析失败: {}", e)))
            }
        }
    }
}

