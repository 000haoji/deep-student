/**
 * Database Optimization Module
 * 
 * Provides optimized database query functions for better performance,
 * especially for tag filtering and complex search operations.
 */

use anyhow::Result;
use rusqlite::Connection;
use chrono::{Utc, DateTime};
use crate::models::MistakeItem;
use std::collections::HashMap;

pub struct DatabaseOptimizations;

impl DatabaseOptimizations {
    /// Optimized tag filtering using SQLite JSON functions
    /// Much faster than application-level filtering for large datasets
    pub fn get_mistakes_optimized(
        conn: &Connection,
        subject_filter: Option<&str>,
        type_filter: Option<&str>,
        tags_filter: Option<&[String]>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<MistakeItem>> {
        let mut query_parts = vec![
            "SELECT id, subject, created_at, question_images, analysis_images, user_question, ocr_text, tags, mistake_type, status, updated_at".to_string(),
            "FROM mistakes".to_string(),
            "WHERE 1=1".to_string(),
        ];
        
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        let mut param_index = 1;

        // Subject filter
        if let Some(subject) = subject_filter {
            query_parts.push(format!(" AND subject = ?{}", param_index));
            params_vec.push(Box::new(subject.to_string()));
            param_index += 1;
        }

        // Type filter
        if let Some(mistake_type) = type_filter {
            query_parts.push(format!(" AND mistake_type = ?{}", param_index));
            params_vec.push(Box::new(mistake_type.to_string()));
            param_index += 1;
        }

        // Optimized tag filter using JSON functions
        if let Some(filter_tags) = tags_filter {
            if !filter_tags.is_empty() {
                // Use JSON_EXTRACT to check if any of the filter tags exist in the tags JSON array
                let tag_conditions: Vec<String> = filter_tags.iter().enumerate().map(|(i, _)| {
                    let param_num = param_index + i;
                    // Check if the tag exists in the JSON array using json_extract and json_each
                    format!(
                        "EXISTS (SELECT 1 FROM json_each(mistakes.tags) WHERE json_each.value = ?{})",
                        param_num
                    )
                }).collect();
                
                query_parts.push(format!(" AND ({})", tag_conditions.join(" OR ")));
                
                // Add tag parameters
                for tag in filter_tags {
                    params_vec.push(Box::new(tag.clone()));
                }
                param_index += filter_tags.len();
            }
        }

        // Add ordering for consistent results
        query_parts.push(" ORDER BY created_at DESC".to_string());

        // Add pagination
        if let Some(limit_val) = limit {
            query_parts.push(format!(" LIMIT ?{}", param_index));
            params_vec.push(Box::new(limit_val));
            param_index += 1;

            if let Some(offset_val) = offset {
                query_parts.push(format!(" OFFSET ?{}", param_index));
                params_vec.push(Box::new(offset_val));
            }
        }

        let full_query = query_parts.join("");
        println!("Optimized query: {}", full_query);
        
        let mut stmt = conn.prepare(&full_query)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        
        let rows = stmt.query_map(&params_refs[..], |row| {
            let created_at_str: String = row.get(2)?;
            let updated_at_str: String = row.get(10)?;
            let question_images_str: String = row.get(3)?;
            let analysis_images_str: String = row.get(4)?;
            let tags_str: String = row.get(7)?;
            
            Ok((
                row.get::<_, String>(0)?, // id
                row.get::<_, String>(1)?, // subject
                created_at_str,
                question_images_str,
                analysis_images_str,
                row.get::<_, String>(5)?, // user_question
                row.get::<_, String>(6)?, // ocr_text
                tags_str,
                row.get::<_, String>(8)?, // mistake_type
                row.get::<_, String>(9)?, // status
                updated_at_str,
            ))
        })?;

        let mut mistakes = Vec::new();
        for row_result in rows {
            let (id, subject, created_at_str, question_images_str, analysis_images_str, 
                 user_question, ocr_text, tags_str, mistake_type, status, updated_at_str) = row_result?;
            
            let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse created_at: {}", e))?
                .with_timezone(&Utc);
            let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse updated_at: {}", e))?
                .with_timezone(&Utc);
            let question_images: Vec<String> = serde_json::from_str(&question_images_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse question_images JSON: {}", e))?;
            let analysis_images: Vec<String> = serde_json::from_str(&analysis_images_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse analysis_images JSON: {}", e))?;
            let tags: Vec<String> = serde_json::from_str(&tags_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse tags JSON: {}", e))?;
            
            let mistake = MistakeItem {
                id,
                subject,
                created_at,
                question_images,
                analysis_images,
                user_question,
                ocr_text,
                tags,
                mistake_type,
                status,
                updated_at,
                chat_history: vec![], // Don't load chat history for list view - optimization
                mistake_summary: None,       // Optimization: not loaded in list view
                user_error_analysis: None,   // Optimization: not loaded in list view
            };
            
            mistakes.push(mistake);
        }
        
        println!("Optimized query returned {} mistakes", mistakes.len());
        Ok(mistakes)
    }

    /// Get tag statistics with optimized JSON queries
    pub fn get_tag_statistics_optimized(conn: &Connection) -> Result<HashMap<String, i32>> {
        let query = r#"
            SELECT json_each.value as tag, COUNT(*) as count
            FROM mistakes, json_each(mistakes.tags)
            WHERE json_valid(mistakes.tags) = 1
            GROUP BY json_each.value
            ORDER BY count DESC
        "#;

        let mut stmt = conn.prepare(query)?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?, // tag
                row.get::<_, i32>(1)?     // count
            ))
        })?;

        let mut tag_stats = HashMap::new();
        for row_result in rows {
            let (tag, count) = row_result?;
            tag_stats.insert(tag, count);
        }

        Ok(tag_stats)
    }

    /// Full-text search across mistake content with ranking
    pub fn search_mistakes_fulltext(
        conn: &Connection,
        search_term: &str,
        subject_filter: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<MistakeItem>> {
        let mut query = r#"
            SELECT id, subject, created_at, question_images, analysis_images, user_question, ocr_text, tags, mistake_type, status, updated_at,
                   -- Calculate relevance score
                   (CASE WHEN user_question LIKE ? THEN 3 ELSE 0 END +
                    CASE WHEN ocr_text LIKE ? THEN 2 ELSE 0 END +
                    CASE WHEN mistake_type LIKE ? THEN 1 ELSE 0 END) as relevance
            FROM mistakes
            WHERE (user_question LIKE ? OR ocr_text LIKE ? OR mistake_type LIKE ?)
        "#.to_string();

        let search_pattern = format!("%{}%", search_term);
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![
            Box::new(search_pattern.clone()), // user_question scoring
            Box::new(search_pattern.clone()), // ocr_text scoring
            Box::new(search_pattern.clone()), // mistake_type scoring
            Box::new(search_pattern.clone()), // user_question filter
            Box::new(search_pattern.clone()), // ocr_text filter
            Box::new(search_pattern.clone()), // mistake_type filter
        ];

        if let Some(subject) = subject_filter {
            query.push_str(" AND subject = ?");
            params_vec.push(Box::new(subject.to_string()));
        }

        query.push_str(" ORDER BY relevance DESC, created_at DESC");

        if let Some(limit_val) = limit {
            query.push_str(" LIMIT ?");
            params_vec.push(Box::new(limit_val));
        }

        let mut stmt = conn.prepare(&query)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        
        let rows = stmt.query_map(&params_refs[..], |row| {
            let created_at_str: String = row.get(2)?;
            let updated_at_str: String = row.get(10)?;
            let question_images_str: String = row.get(3)?;
            let analysis_images_str: String = row.get(4)?;
            let tags_str: String = row.get(7)?;
            let relevance: i32 = row.get(11)?;
            
            Ok((
                row.get::<_, String>(0)?, // id
                row.get::<_, String>(1)?, // subject
                created_at_str,
                question_images_str,
                analysis_images_str,
                row.get::<_, String>(5)?, // user_question
                row.get::<_, String>(6)?, // ocr_text
                tags_str,
                row.get::<_, String>(8)?, // mistake_type
                row.get::<_, String>(9)?, // status
                updated_at_str,
                relevance,
            ))
        })?;

        let mut mistakes = Vec::new();
        for row_result in rows {
            let (id, subject, created_at_str, question_images_str, analysis_images_str, 
                 user_question, ocr_text, tags_str, mistake_type, status, updated_at_str, _relevance) = row_result?;
            
            let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse created_at: {}", e))?
                .with_timezone(&Utc);
            let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse updated_at: {}", e))?
                .with_timezone(&Utc);
            let question_images: Vec<String> = serde_json::from_str(&question_images_str)?;
            let analysis_images: Vec<String> = serde_json::from_str(&analysis_images_str)?;
            let tags: Vec<String> = serde_json::from_str(&tags_str)?;
            
            let mistake = MistakeItem {
                id,
                subject,
                created_at,
                question_images,
                analysis_images,
                user_question,
                ocr_text,
                tags,
                mistake_type,
                status,
                updated_at,
                chat_history: vec![],
                mistake_summary: None,       // Search doesn't need summary
                user_error_analysis: None,   // Search doesn't need analysis
            };
            
            mistakes.push(mistake);
        }
        
        println!("Full-text search returned {} mistakes for term: '{}'", mistakes.len(), search_term);
        Ok(mistakes)
    }

    /// Get mistakes by date range with optimized indexing
    pub fn get_mistakes_by_date_range(
        conn: &Connection,
        start_date: &str, // RFC3339 format
        end_date: &str,   // RFC3339 format
        subject_filter: Option<&str>,
    ) -> Result<Vec<MistakeItem>> {
        let mut query = r#"
            SELECT id, subject, created_at, question_images, analysis_images, user_question, ocr_text, tags, mistake_type, status, updated_at
            FROM mistakes
            WHERE created_at >= ? AND created_at <= ?
        "#.to_string();

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![
            Box::new(start_date.to_string()),
            Box::new(end_date.to_string()),
        ];

        if let Some(subject) = subject_filter {
            query.push_str(" AND subject = ?");
            params_vec.push(Box::new(subject.to_string()));
        }

        query.push_str(" ORDER BY created_at DESC");

        let mut stmt = conn.prepare(&query)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        
        let rows = stmt.query_map(&params_refs[..], |row| {
            let created_at_str: String = row.get(2)?;
            let updated_at_str: String = row.get(10)?;
            let question_images_str: String = row.get(3)?;
            let analysis_images_str: String = row.get(4)?;
            let tags_str: String = row.get(7)?;
            
            Ok((
                row.get::<_, String>(0)?, // id
                row.get::<_, String>(1)?, // subject
                created_at_str,
                question_images_str,
                analysis_images_str,
                row.get::<_, String>(5)?, // user_question
                row.get::<_, String>(6)?, // ocr_text
                tags_str,
                row.get::<_, String>(8)?, // mistake_type
                row.get::<_, String>(9)?, // status
                updated_at_str,
            ))
        })?;

        let mut mistakes = Vec::new();
        for row_result in rows {
            let (id, subject, created_at_str, question_images_str, analysis_images_str, 
                 user_question, ocr_text, tags_str, mistake_type, status, updated_at_str) = row_result?;
            
            let created_at = DateTime::parse_from_rfc3339(&created_at_str)?
                .with_timezone(&Utc);
            let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)?
                .with_timezone(&Utc);
            let question_images: Vec<String> = serde_json::from_str(&question_images_str)?;
            let analysis_images: Vec<String> = serde_json::from_str(&analysis_images_str)?;
            let tags: Vec<String> = serde_json::from_str(&tags_str)?;
            
            let mistake = MistakeItem {
                id,
                subject,
                created_at,
                question_images,
                analysis_images,
                user_question,
                ocr_text,
                tags,
                mistake_type,
                status,
                updated_at,
                chat_history: vec![],
                mistake_summary: None,       // Search doesn't need summary
                user_error_analysis: None,   // Search doesn't need analysis
            };
            
            mistakes.push(mistake);
        }
        
        Ok(mistakes)
    }

    /// Create database indexes for better performance
    pub fn create_performance_indexes(conn: &Connection) -> Result<()> {
        let indexes = vec![
            // Index on created_at for date-based queries
            "CREATE INDEX IF NOT EXISTS idx_mistakes_created_at ON mistakes(created_at)",
            
            // Index on subject for subject filtering
            "CREATE INDEX IF NOT EXISTS idx_mistakes_subject ON mistakes(subject)",
            
            // Index on mistake_type for type filtering
            "CREATE INDEX IF NOT EXISTS idx_mistakes_type ON mistakes(mistake_type)",
            
            // Index on status for status filtering
            "CREATE INDEX IF NOT EXISTS idx_mistakes_status ON mistakes(status)",
            
            // Composite index for common filtering combinations
            "CREATE INDEX IF NOT EXISTS idx_mistakes_subject_type ON mistakes(subject, mistake_type)",
            
            // Index on chat_messages for faster joins
            "CREATE INDEX IF NOT EXISTS idx_chat_messages_mistake_id ON chat_messages(mistake_id)",
            "CREATE INDEX IF NOT EXISTS idx_chat_messages_timestamp ON chat_messages(timestamp)",
        ];

        for index_sql in indexes {
            conn.execute(index_sql, [])?;
            println!("Created index: {}", index_sql);
        }

        Ok(())
    }

    /// Analyze query performance and suggest optimizations
    pub fn analyze_query_performance(conn: &Connection, query: &str) -> Result<String> {
        let explain_query = format!("EXPLAIN QUERY PLAN {}", query);
        
        let mut stmt = conn.prepare(&explain_query)?;
        let rows = stmt.query_map([], |row| {
            Ok(format!(
                "Level {}: {} - {}",
                row.get::<_, i32>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?
            ))
        })?;

        let mut analysis = Vec::new();
        for row_result in rows {
            analysis.push(row_result?);
        }

        Ok(analysis.join("\n"))
    }
}

/// Extension trait to add optimized methods to Database
pub trait DatabaseOptimizationExt {
    fn get_mistakes_optimized(
        &self,
        subject_filter: Option<&str>,
        type_filter: Option<&str>,
        tags_filter: Option<&[String]>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<MistakeItem>>;

    fn get_tag_statistics_optimized(&self) -> Result<HashMap<String, i32>>;
    
    fn search_mistakes_fulltext(
        &self,
        search_term: &str,
        subject_filter: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<MistakeItem>>;

    fn get_mistakes_by_date_range(
        &self,
        start_date: &str,
        end_date: &str,
        subject_filter: Option<&str>,
    ) -> Result<Vec<MistakeItem>>;

    fn create_performance_indexes(&self) -> Result<()>;
    fn analyze_query_performance(&self, query: &str) -> Result<String>;
}

impl DatabaseOptimizationExt for crate::database::Database {
    fn get_mistakes_optimized(
        &self,
        subject_filter: Option<&str>,
        type_filter: Option<&str>,
        tags_filter: Option<&[String]>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<MistakeItem>> {
        let conn = self.conn().lock().unwrap();
        DatabaseOptimizations::get_mistakes_optimized(
            &conn,
            subject_filter,
            type_filter,
            tags_filter,
            limit,
            offset,
        )
    }

    fn get_tag_statistics_optimized(&self) -> Result<HashMap<String, i32>> {
        let conn = self.conn().lock().unwrap();
        DatabaseOptimizations::get_tag_statistics_optimized(&conn)
    }

    fn search_mistakes_fulltext(
        &self,
        search_term: &str,
        subject_filter: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<MistakeItem>> {
        let conn = self.conn().lock().unwrap();
        DatabaseOptimizations::search_mistakes_fulltext(&conn, search_term, subject_filter, limit)
    }

    fn get_mistakes_by_date_range(
        &self,
        start_date: &str,
        end_date: &str,
        subject_filter: Option<&str>,
    ) -> Result<Vec<MistakeItem>> {
        let conn = self.conn().lock().unwrap();
        DatabaseOptimizations::get_mistakes_by_date_range(&conn, start_date, end_date, subject_filter)
    }

    fn create_performance_indexes(&self) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        DatabaseOptimizations::create_performance_indexes(&conn)
    }

    fn analyze_query_performance(&self, query: &str) -> Result<String> {
        let conn = self.conn().lock().unwrap();
        DatabaseOptimizations::analyze_query_performance(&conn, query)
    }
}