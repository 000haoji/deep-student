use crate::models::{
    DocumentChunk, DocumentChunkWithEmbedding, RetrievedChunk, VectorStoreStats, AppError
};
use crate::database::Database;
use std::sync::Arc;
use serde_json::Value;
use rusqlite::params;

type Result<T> = std::result::Result<T, AppError>;

/// 向量存储抽象接口
pub trait VectorStore {
    /// 添加文档块和对应的向量
    async fn add_chunks(&self, chunks: Vec<DocumentChunkWithEmbedding>) -> Result<()>;
    
    /// 搜索相似的文档块
    async fn search_similar_chunks(&self, query_embedding: Vec<f32>, top_k: usize) -> Result<Vec<RetrievedChunk>>;
    
    /// 在指定分库中搜索相似的文档块
    async fn search_similar_chunks_in_libraries(&self, query_embedding: Vec<f32>, top_k: usize, sub_library_ids: Option<Vec<String>>) -> Result<Vec<RetrievedChunk>>;
    
    /// 根据文档ID删除所有相关块
    async fn delete_chunks_by_document_id(&self, document_id: &str) -> Result<()>;
    
    /// 获取统计信息
    async fn get_stats(&self) -> Result<VectorStoreStats>;
    
    /// 清空所有向量数据
    async fn clear_all(&self) -> Result<()>;
}

/// SQLite向量存储实现 (基础版本，使用余弦相似度)
pub struct SqliteVectorStore {
    database: Arc<Database>,
}

impl SqliteVectorStore {
    pub fn new(database: Arc<Database>) -> Result<Self> {
        let store = Self { database };
        store.initialize_tables()?;
        Ok(store)
    }
    
    /// 初始化向量存储相关表
    fn initialize_tables(&self) -> Result<()> {
        let conn = self.database.conn().lock()
            .map_err(|e| AppError::database(format!("获取数据库连接失败: {}", e)))?;
        
        // 分库表
        conn.execute(
            "CREATE TABLE IF NOT EXISTS rag_sub_libraries (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                description TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
            [],
        ).map_err(|e| AppError::database(format!("创建分库表失败: {}", e)))?;
        
        // 检查并创建默认分库
        let default_library_exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM rag_sub_libraries WHERE name = 'default')",
            [],
            |row| row.get(0)
        ).unwrap_or(false);
        
        if !default_library_exists {
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO rag_sub_libraries (id, name, description, created_at, updated_at) 
                 VALUES ('default', 'default', '默认知识库', ?, ?)",
                params![now, now],
            ).map_err(|e| AppError::database(format!("创建默认分库失败: {}", e)))?;
        }
        
        // 文档表（增加分库外键）
        conn.execute(
            "CREATE TABLE IF NOT EXISTS rag_documents (
                id TEXT PRIMARY KEY,
                file_name TEXT NOT NULL,
                file_path TEXT,
                file_size INTEGER,
                content_type TEXT,
                total_chunks INTEGER DEFAULT 0,
                sub_library_id TEXT NOT NULL DEFAULT 'default',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (sub_library_id) REFERENCES rag_sub_libraries (id) ON DELETE SET DEFAULT
            )",
            [],
        ).map_err(|e| AppError::database(format!("创建文档表失败: {}", e)))?;
        
        // 检查现有文档表是否有sub_library_id列
        let has_sub_library_column: bool = conn.prepare("SELECT sub_library_id FROM rag_documents LIMIT 1")
            .is_ok();
        
        if !has_sub_library_column {
            conn.execute(
                "ALTER TABLE rag_documents ADD COLUMN sub_library_id TEXT NOT NULL DEFAULT 'default'",
                []
            ).map_err(|e| AppError::database(format!("添加分库列失败: {}", e)))?;
        }
        
        // 文档块表
        conn.execute(
            "CREATE TABLE IF NOT EXISTS rag_document_chunks (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                text TEXT NOT NULL,
                metadata TEXT NOT NULL, -- JSON格式的元数据
                created_at TEXT NOT NULL,
                FOREIGN KEY (document_id) REFERENCES rag_documents (id) ON DELETE CASCADE
            )",
            [],
        ).map_err(|e| AppError::database(format!("创建文档块表失败: {}", e)))?;
        
        // 向量表（优化版本，存储为BLOB）
        conn.execute(
            "CREATE TABLE IF NOT EXISTS rag_vectors (
                chunk_id TEXT PRIMARY KEY,
                embedding BLOB NOT NULL, -- 二进制格式的向量数据
                dimension INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (chunk_id) REFERENCES rag_document_chunks (id) ON DELETE CASCADE
            )",
            [],
        ).map_err(|e| AppError::database(format!("创建向量表失败: {}", e)))?;
        
        // 创建索引以提高查询性能
        let indexes = vec![
            "CREATE INDEX IF NOT EXISTS idx_rag_chunks_document_id ON rag_document_chunks(document_id)",
            "CREATE INDEX IF NOT EXISTS idx_rag_chunks_index ON rag_document_chunks(chunk_index)",
            "CREATE INDEX IF NOT EXISTS idx_rag_vectors_dimension ON rag_vectors(dimension)",
            "CREATE INDEX IF NOT EXISTS idx_rag_documents_sub_library ON rag_documents(sub_library_id)",
            "CREATE INDEX IF NOT EXISTS idx_rag_sub_libraries_name ON rag_sub_libraries(name)",
        ];
        
        for index_sql in indexes {
            conn.execute(index_sql, [])
                .map_err(|e| AppError::database(format!("创建索引失败: {}", e)))?;
        }
        
        println!("✅ RAG向量存储表初始化完成");
        Ok(())
    }
    
    /// 将向量序列化为BLOB
    fn serialize_vector_to_blob(vector: &[f32]) -> Result<Vec<u8>> {
        let mut blob = Vec::with_capacity(vector.len() * 4);
        for &value in vector {
            blob.extend_from_slice(&value.to_le_bytes());
        }
        Ok(blob)
    }
    
    /// 从BLOB反序列化向量
    fn deserialize_vector_from_blob(blob: &[u8]) -> Result<Vec<f32>> {
        if blob.len() % 4 != 0 {
            return Err(AppError::database("向量BLOB大小不正确".to_string()));
        }
        
        let mut vector = Vec::with_capacity(blob.len() / 4);
        for chunk in blob.chunks_exact(4) {
            let bytes: [u8; 4] = chunk.try_into()
                .map_err(|_| AppError::database("向量BLOB格式错误".to_string()))?;
            vector.push(f32::from_le_bytes(bytes));
        }
        Ok(vector)
    }
    
    /// 计算余弦相似度
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }
        
        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        
        if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            dot_product / (norm_a * norm_b)
        }
    }
}

impl VectorStore for SqliteVectorStore {
    async fn add_chunks(&self, chunks: Vec<DocumentChunkWithEmbedding>) -> Result<()> {
        let conn = self.database.conn().lock()
            .map_err(|e| AppError::database(format!("获取数据库连接失败: {}", e)))?;
        
        let transaction = conn.unchecked_transaction()
            .map_err(|e| AppError::database(format!("开始事务失败: {}", e)))?;
        
        let chunks_len = chunks.len();
        
        for chunk_with_embedding in chunks {
            let chunk = &chunk_with_embedding.chunk;
            let embedding = &chunk_with_embedding.embedding;
            
            // 插入文档块
            let metadata_json = serde_json::to_string(&chunk.metadata)
                .map_err(|e| AppError::database(format!("序列化元数据失败: {}", e)))?;
            
            transaction.execute(
                "INSERT OR REPLACE INTO rag_document_chunks 
                 (id, document_id, chunk_index, text, metadata, created_at) 
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    chunk.id,
                    chunk.document_id,
                    chunk.chunk_index,
                    chunk.text,
                    metadata_json,
                    chrono::Utc::now().to_rfc3339()
                ],
            ).map_err(|e| AppError::database(format!("插入文档块失败: {}", e)))?;
            
            // 将向量转换为BLOB
            let embedding_blob = Self::serialize_vector_to_blob(embedding)?;
            
            // 插入向量到rag_vectors表
            transaction.execute(
                "INSERT OR REPLACE INTO rag_vectors 
                 (chunk_id, embedding, dimension, created_at) 
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    chunk.id,
                    embedding_blob,
                    embedding.len() as i32,
                    chrono::Utc::now().to_rfc3339()
                ],
            ).map_err(|e| AppError::database(format!("插入向量失败: {}", e)))?;
        }
        
        transaction.commit()
            .map_err(|e| AppError::database(format!("提交事务失败: {}", e)))?;
        
        println!("✅ 成功添加 {} 个文档块到向量存储", chunks_len);
        Ok(())
    }
    
    async fn search_similar_chunks(&self, query_embedding: Vec<f32>, top_k: usize) -> Result<Vec<RetrievedChunk>> {
        let conn = self.database.conn().lock()
            .map_err(|e| AppError::database(format!("获取数据库连接失败: {}", e)))?;
        
        // 基础SQLite实现：获取所有向量，在应用层计算相似度
        let mut stmt = conn.prepare(
            "SELECT v.chunk_id, v.embedding, c.document_id, c.chunk_index, c.text, c.metadata 
             FROM rag_vectors v
             JOIN rag_document_chunks c ON v.chunk_id = c.id
             WHERE v.dimension = ?"
        ).map_err(|e| AppError::database(format!("准备查询语句失败: {}", e)))?;
        
        let query_dim = query_embedding.len() as i32;
        let rows = stmt.query_map(params![query_dim], |row| {
            let chunk_id: String = row.get(0)?;
            let embedding_blob: Vec<u8> = row.get(1)?;
            let document_id: String = row.get(2)?;
            let chunk_index: usize = row.get(3)?;
            let text: String = row.get(4)?;
            let metadata_json: String = row.get(5)?;
            
            Ok((chunk_id, embedding_blob, document_id, chunk_index, text, metadata_json))
        }).map_err(|e| AppError::database(format!("执行查询失败: {}", e)))?;
        
        let mut candidates = Vec::new();
        
        for row in rows {
            let (chunk_id, embedding_blob, document_id, chunk_index, text, metadata_json) = row
                .map_err(|e| AppError::database(format!("读取查询结果失败: {}", e)))?;
            
            // 反序列化向量
            let stored_embedding = Self::deserialize_vector_from_blob(&embedding_blob)?;
            
            // 计算余弦相似度
            let similarity = Self::cosine_similarity(&query_embedding, &stored_embedding);
            
            // 解析元数据
            let metadata: std::collections::HashMap<String, String> = serde_json::from_str(&metadata_json)
                .map_err(|e| AppError::database(format!("解析元数据失败: {}", e)))?;
            
            candidates.push(RetrievedChunk {
                chunk: DocumentChunk {
                    id: chunk_id,
                    document_id,
                    chunk_index,
                    text,
                    metadata,
                },
                score: similarity,
            });
        }
        
        // 按相似度排序并返回前top_k个结果
        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        candidates.truncate(top_k);
        
        println!("✅ 检索到 {} 个相似文档块", candidates.len());
        Ok(candidates)
    }
    
    async fn search_similar_chunks_in_libraries(&self, query_embedding: Vec<f32>, top_k: usize, sub_library_ids: Option<Vec<String>>) -> Result<Vec<RetrievedChunk>> {
        let conn = self.database.conn().lock()
            .map_err(|e| AppError::database(format!("获取数据库连接失败: {}", e)))?;
        
        // 构建SQL查询，根据是否有分库ID过滤
        // 构建SQL查询，根据是否有分库ID过滤
        let (sql, params): (String, Vec<rusqlite::types::Value>) = if let Some(ref library_ids) = sub_library_ids {
            if library_ids.is_empty() {
                return Ok(Vec::new()); // 如果提供了空的分库ID列表，返回空结果
            }
            
            let placeholders = library_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT v.chunk_id, v.embedding, c.document_id, c.chunk_index, c.text, c.metadata 
                 FROM rag_vectors v
                 JOIN rag_document_chunks c ON v.chunk_id = c.id
                 JOIN rag_documents d ON c.document_id = d.id
                 WHERE v.dimension = ? AND d.sub_library_id IN ({})",
                placeholders
            );
            
            let mut params = vec![rusqlite::types::Value::Integer(query_embedding.len() as i64)];
            for lib_id in library_ids.iter() {
                params.push(rusqlite::types::Value::Text(lib_id.clone()));
            }
            
            (sql, params)
        } else {
            // 如果没有指定分库ID，查询所有分库
            let sql = "SELECT v.chunk_id, v.embedding, c.document_id, c.chunk_index, c.text, c.metadata 
                       FROM rag_vectors v
                       JOIN rag_document_chunks c ON v.chunk_id = c.id
                       WHERE v.dimension = ?".to_string();
            let params = vec![rusqlite::types::Value::Integer(query_embedding.len() as i64)];
            (sql, params)
        };
        
        let mut stmt = conn.prepare(&sql)
            .map_err(|e| AppError::database(format!("准备查询语句失败: {}", e)))?;
        
        let rows = stmt.query_map(rusqlite::params_from_iter(params), |row| {
            let chunk_id: String = row.get(0)?;
            let embedding_blob: Vec<u8> = row.get(1)?;
            let document_id: String = row.get(2)?;
            let chunk_index: usize = row.get(3)?;
            let text: String = row.get(4)?;
            let metadata_json: String = row.get(5)?;
            
            Ok((chunk_id, embedding_blob, document_id, chunk_index, text, metadata_json))
        }).map_err(|e| AppError::database(format!("执行查询失败: {}", e)))?;
        
        let mut candidates = Vec::new();
        
        for row in rows {
            let (chunk_id, embedding_blob, document_id, chunk_index, text, metadata_json) = row
                .map_err(|e| AppError::database(format!("读取查询结果失败: {}", e)))?;
            
            // 反序列化向量
            let stored_embedding = Self::deserialize_vector_from_blob(&embedding_blob)?;
            
            // 计算余弦相似度
            let similarity = Self::cosine_similarity(&query_embedding, &stored_embedding);
            
            // 解析元数据
            let metadata: std::collections::HashMap<String, String> = serde_json::from_str(&metadata_json)
                .map_err(|e| AppError::database(format!("解析元数据失败: {}", e)))?;
            
            candidates.push(RetrievedChunk {
                chunk: DocumentChunk {
                    id: chunk_id,
                    document_id,
                    chunk_index,
                    text,
                    metadata,
                },
                score: similarity,
            });
        }
        
        // 按相似度排序并返回前top_k个结果
        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        candidates.truncate(top_k);
        
        let library_filter_msg = if let Some(ref lib_ids) = sub_library_ids {
            format!(" (过滤分库: {:?})", lib_ids)
        } else {
            " (所有分库)".to_string()
        };
        
        println!("✅ 检索到 {} 个相似文档块{}", candidates.len(), library_filter_msg);
        Ok(candidates)
    }
    
    async fn delete_chunks_by_document_id(&self, document_id: &str) -> Result<()> {
        let conn = self.database.conn().lock()
            .map_err(|e| AppError::database(format!("获取数据库连接失败: {}", e)))?;
        
        let transaction = conn.unchecked_transaction()
            .map_err(|e| AppError::database(format!("开始事务失败: {}", e)))?;
        
        // VSS已移除，直接删除相关记录
        
        // 删除向量（通过外键级联删除）
        transaction.execute(
            "DELETE FROM rag_document_chunks WHERE document_id = ?1",
            params![document_id],
        ).map_err(|e| AppError::database(format!("删除文档块失败: {}", e)))?;
        
        // 删除文档记录
        transaction.execute(
            "DELETE FROM rag_documents WHERE id = ?1",
            params![document_id],
        ).map_err(|e| AppError::database(format!("删除文档记录失败: {}", e)))?;
        
        transaction.commit()
            .map_err(|e| AppError::database(format!("提交删除事务失败: {}", e)))?;
        
        println!("✅ 成功删除文档 {} 的所有块和VSS索引", document_id);
        Ok(())
    }
    
    async fn get_stats(&self) -> Result<VectorStoreStats> {
        let conn = self.database.conn().lock()
            .map_err(|e| AppError::database(format!("获取数据库连接失败: {}", e)))?;
        
        let total_documents: usize = conn.query_row(
            "SELECT COUNT(*) FROM rag_documents",
            [],
            |row| row.get(0),
        ).map_err(|e| AppError::database(format!("查询文档数失败: {}", e)))?;
        
        let total_chunks: usize = conn.query_row(
            "SELECT COUNT(*) FROM rag_document_chunks",
            [],
            |row| row.get(0),
        ).map_err(|e| AppError::database(format!("查询块数失败: {}", e)))?;
        
        // 计算存储大小（估算）
        let storage_size_bytes: u64 = conn.query_row(
            "SELECT COALESCE(SUM(LENGTH(text) + LENGTH(embedding)), 0) 
             FROM rag_document_chunks c 
             LEFT JOIN rag_vectors v ON c.id = v.chunk_id",
            [],
            |row| row.get::<_, i64>(0).map(|s| s as u64),
        ).unwrap_or(0);
        
        Ok(VectorStoreStats {
            total_documents,
            total_chunks,
            storage_size_bytes,
        })
    }
    
    async fn clear_all(&self) -> Result<()> {
        let conn = self.database.conn().lock()
            .map_err(|e| AppError::database(format!("获取数据库连接失败: {}", e)))?;
        
        let transaction = conn.unchecked_transaction()
            .map_err(|e| AppError::database(format!("开始事务失败: {}", e)))?;
        
        // 清空VSS索引
        transaction.execute("DELETE FROM vss_chunks", [])
            .map_err(|e| AppError::database(format!("清空VSS索引失败: {}", e)))?;
        
        transaction.execute("DELETE FROM rag_vectors", [])
            .map_err(|e| AppError::database(format!("清空向量表失败: {}", e)))?;
        
        transaction.execute("DELETE FROM rag_document_chunks", [])
            .map_err(|e| AppError::database(format!("清空文档块表失败: {}", e)))?;
        
        transaction.execute("DELETE FROM rag_documents", [])
            .map_err(|e| AppError::database(format!("清空文档表失败: {}", e)))?;
        
        transaction.commit()
            .map_err(|e| AppError::database(format!("提交清空事务失败: {}", e)))?;
        
        println!("✅ 成功清空所有向量存储数据和VSS索引");
        Ok(())
    }
}

/// 文档管理相关方法
impl SqliteVectorStore {
    /// 添加文档记录
    pub fn add_document_record(&self, document_id: &str, file_name: &str, file_path: Option<&str>, file_size: Option<u64>) -> Result<()> {
        self.add_document_record_with_library(document_id, file_name, file_path, file_size, "default")
    }
    
    /// 添加文档记录到指定分库
    pub fn add_document_record_with_library(&self, document_id: &str, file_name: &str, file_path: Option<&str>, file_size: Option<u64>, sub_library_id: &str) -> Result<()> {
        let conn = self.database.conn().lock()
            .map_err(|e| AppError::database(format!("获取数据库连接失败: {}", e)))?;
        
        conn.execute(
            "INSERT OR REPLACE INTO rag_documents 
             (id, file_name, file_path, file_size, sub_library_id, created_at, updated_at) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                document_id,
                file_name,
                file_path,
                file_size.map(|s| s as i64),
                sub_library_id,
                chrono::Utc::now().to_rfc3339(),
                chrono::Utc::now().to_rfc3339()
            ],
        ).map_err(|e| AppError::database(format!("添加文档记录失败: {}", e)))?;
        
        Ok(())
    }
    
    /// 更新文档的块数统计
    pub fn update_document_chunk_count(&self, document_id: &str, chunk_count: usize) -> Result<()> {
        let conn = self.database.conn().lock()
            .map_err(|e| AppError::database(format!("获取数据库连接失败: {}", e)))?;
        
        conn.execute(
            "UPDATE rag_documents SET total_chunks = ?1, updated_at = ?2 WHERE id = ?3",
            params![
                chunk_count as i32,
                chrono::Utc::now().to_rfc3339(),
                document_id
            ],
        ).map_err(|e| AppError::database(format!("更新文档块数失败: {}", e)))?;
        
        Ok(())
    }
    
    /// 获取所有文档列表
    pub fn get_all_documents(&self) -> Result<Vec<Value>> {
        let conn = self.database.conn().lock()
            .map_err(|e| AppError::database(format!("获取数据库连接失败: {}", e)))?;
        
        let mut stmt = conn.prepare(
            "SELECT id, file_name, file_path, file_size, total_chunks, created_at, updated_at FROM rag_documents ORDER BY created_at DESC"
        ).map_err(|e| AppError::database(format!("准备查询语句失败: {}", e)))?;
        
        let rows = stmt.query_map([], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "file_name": row.get::<_, String>(1)?,
                "file_path": row.get::<_, Option<String>>(2)?,
                "file_size": row.get::<_, Option<i64>>(3)?,
                "total_chunks": row.get::<_, i32>(4)?,
                "created_at": row.get::<_, String>(5)?,
                "updated_at": row.get::<_, String>(6)?
            }))
        }).map_err(|e| AppError::database(format!("查询文档列表失败: {}", e)))?;
        
        let mut documents = Vec::new();
        for row in rows {
            documents.push(row.map_err(|e| AppError::database(format!("读取文档行失败: {}", e)))?);
        }
        
        Ok(documents)
    }
}