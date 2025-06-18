/**
 * Batch Operations Module
 * 
 * Provides efficient batch operations for database operations to improve performance
 * when dealing with multiple mistakes, chat messages, or other bulk operations.
 */

use anyhow::Result;
use rusqlite::{Connection, params};
use chrono::Utc;
use crate::models::{MistakeItem, ChatMessage};
use std::collections::HashMap;

pub struct BatchOperations<'a> {
    conn: &'a mut Connection,
}

impl<'a> BatchOperations<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    /// Batch save multiple mistakes with their chat histories
    /// More efficient than individual saves when dealing with multiple items
    pub fn batch_save_mistakes(&mut self, mistakes: &[MistakeItem]) -> Result<usize> {
        if mistakes.is_empty() {
            return Ok(0);
        }

        let tx = self.conn.transaction()?;
        let mut saved_count = 0;

        {
            // Prepare statements for reuse in a separate scope
            let mut mistake_stmt = tx.prepare(
                "INSERT OR REPLACE INTO mistakes (id, subject, created_at, question_images, analysis_images, user_question, ocr_text, tags, mistake_type, status, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
            )?;

            let mut delete_messages_stmt = tx.prepare(
                "DELETE FROM chat_messages WHERE mistake_id = ?1"
            )?;

            let mut message_stmt = tx.prepare(
                "INSERT INTO chat_messages (mistake_id, role, content, timestamp, thinking_content) 
                 VALUES (?1, ?2, ?3, ?4, ?5)"
            )?;

            for mistake in mistakes {
                // Insert/update mistake
                mistake_stmt.execute(params![
                    mistake.id,
                    mistake.subject,
                    mistake.created_at.to_rfc3339(),
                    serde_json::to_string(&mistake.question_images)?,
                    serde_json::to_string(&mistake.analysis_images)?,
                    mistake.user_question,
                    mistake.ocr_text,
                    serde_json::to_string(&mistake.tags)?,
                    mistake.mistake_type,
                    mistake.status,
                    Utc::now().to_rfc3339(),
                ])?;

                // Delete old chat messages
                delete_messages_stmt.execute(params![mistake.id])?;

                // Insert new chat messages
                for message in &mistake.chat_history {
                    message_stmt.execute(params![
                        mistake.id,
                        message.role,
                        message.content,
                        message.timestamp.to_rfc3339(),
                        message.thinking_content
                    ])?;
                }

                saved_count += 1;
            }
        } // Statements are dropped here

        tx.commit()?;
        println!("✅ Batch saved {} mistakes with their chat histories", saved_count);
        Ok(saved_count)
    }

    /// Batch delete multiple mistakes by IDs
    pub fn batch_delete_mistakes(&mut self, mistake_ids: &[String]) -> Result<usize> {
        if mistake_ids.is_empty() {
            return Ok(0);
        }

        let tx = self.conn.transaction()?;
        let deleted_count;

        {
            // Create placeholders for IN clause
            let placeholders = mistake_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let query = format!("DELETE FROM mistakes WHERE id IN ({})", placeholders);

            let mut stmt = tx.prepare(&query)?;
            let params: Vec<&dyn rusqlite::ToSql> = mistake_ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
            
            deleted_count = stmt.execute(&params[..])?;
        } // Statement is dropped here

        tx.commit()?;
        println!("✅ Batch deleted {} mistakes", deleted_count);
        Ok(deleted_count)
    }

    /// Batch update mistake statuses
    pub fn batch_update_mistake_statuses(&mut self, updates: &HashMap<String, String>) -> Result<usize> {
        if updates.is_empty() {
            return Ok(0);
        }

        let tx = self.conn.transaction()?;
        let mut updated_count = 0;

        {
            let mut stmt = tx.prepare(
                "UPDATE mistakes SET status = ?1, updated_at = ?2 WHERE id = ?3"
            )?;

            let now = Utc::now().to_rfc3339();
            for (mistake_id, new_status) in updates {
                let changes = stmt.execute(params![new_status, now, mistake_id])?;
                updated_count += changes;
            }
        } // Statement is dropped here

        tx.commit()?;
        println!("✅ Batch updated {} mistake statuses", updated_count);
        Ok(updated_count)
    }

    /// Batch add chat messages to multiple mistakes
    pub fn batch_add_chat_messages(&mut self, messages_by_mistake: &HashMap<String, Vec<ChatMessage>>) -> Result<usize> {
        if messages_by_mistake.is_empty() {
            return Ok(0);
        }

        let tx = self.conn.transaction()?;
        let mut added_count = 0;

        {
            let mut stmt = tx.prepare(
                "INSERT INTO chat_messages (mistake_id, role, content, timestamp, thinking_content) 
                 VALUES (?1, ?2, ?3, ?4, ?5)"
            )?;

            for (mistake_id, messages) in messages_by_mistake {
                for message in messages {
                    stmt.execute(params![
                        mistake_id,
                        message.role,
                        message.content,
                        message.timestamp.to_rfc3339(),
                        message.thinking_content
                    ])?;
                    added_count += 1;
                }
            }
        } // Statement is dropped here

        tx.commit()?;
        println!("✅ Batch added {} chat messages", added_count);
        Ok(added_count)
    }

    /// Batch update mistake tags
    pub fn batch_update_mistake_tags(&mut self, tag_updates: &HashMap<String, Vec<String>>) -> Result<usize> {
        if tag_updates.is_empty() {
            return Ok(0);
        }

        let tx = self.conn.transaction()?;
        let mut updated_count = 0;

        {
            let mut stmt = tx.prepare(
                "UPDATE mistakes SET tags = ?1, updated_at = ?2 WHERE id = ?3"
            )?;

            let now = Utc::now().to_rfc3339();
            for (mistake_id, new_tags) in tag_updates {
                let tags_json = serde_json::to_string(new_tags)?;
                let changes = stmt.execute(params![tags_json, now, mistake_id])?;
                updated_count += changes;
            }
        } // Statement is dropped here

        tx.commit()?;
        println!("✅ Batch updated tags for {} mistakes", updated_count);
        Ok(updated_count)
    }

    /// Batch operation: Archive old mistakes (change status to "archived")
    pub fn batch_archive_old_mistakes(&mut self, days_old: i64) -> Result<usize> {
        let tx = self.conn.transaction()?;
        
        let cutoff_date = (Utc::now() - chrono::Duration::days(days_old)).to_rfc3339();
        
        let updated_count = tx.execute(
            "UPDATE mistakes SET status = 'archived', updated_at = ?1 
             WHERE created_at < ?2 AND status != 'archived'",
            params![Utc::now().to_rfc3339(), cutoff_date]
        )?;

        tx.commit()?;
        println!("✅ Batch archived {} old mistakes (older than {} days)", updated_count, days_old);
        Ok(updated_count)
    }

    /// Batch cleanup: Remove orphaned chat messages
    pub fn batch_cleanup_orphaned_messages(&mut self) -> Result<usize> {
        let tx = self.conn.transaction()?;
        
        let deleted_count = tx.execute(
            "DELETE FROM chat_messages 
             WHERE mistake_id NOT IN (SELECT id FROM mistakes)",
            []
        )?;

        tx.commit()?;
        println!("✅ Batch cleaned up {} orphaned chat messages", deleted_count);
        Ok(deleted_count)
    }

    /// Batch export: Get multiple mistakes with their full data for export
    pub fn batch_export_mistakes(&self, mistake_ids: &[String]) -> Result<Vec<MistakeItem>> {
        if mistake_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Create placeholders for IN clause
        let placeholders = mistake_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT id, subject, created_at, question_images, analysis_images, user_question, ocr_text, tags, mistake_type, status, updated_at 
             FROM mistakes WHERE id IN ({})",
            placeholders
        );

        let mut stmt = self.conn.prepare(&query)?;
        let params: Vec<&dyn rusqlite::ToSql> = mistake_ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        
        let mut mistakes = Vec::new();
        let rows = stmt.query_map(&params[..], |row| {
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

        // Collect all mistake basic data first
        let mut mistake_data = Vec::new();
        for row_result in rows {
            mistake_data.push(row_result?);
        }

        // Now fetch chat messages for all mistakes in one query
        let mut chat_messages: HashMap<String, Vec<ChatMessage>> = HashMap::new();
        if !mistake_ids.is_empty() {
            let placeholders = mistake_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let chat_query = format!(
                "SELECT mistake_id, role, content, timestamp, thinking_content 
                 FROM chat_messages WHERE mistake_id IN ({}) ORDER BY mistake_id, timestamp ASC",
                placeholders
            );
            
            let mut chat_stmt = self.conn.prepare(&chat_query)?;
            let chat_rows = chat_stmt.query_map(&params[..], |row| {
                Ok((
                    row.get::<_, String>(0)?, // mistake_id
                    row.get::<_, String>(1)?, // role
                    row.get::<_, String>(2)?, // content
                    row.get::<_, String>(3)?, // timestamp
                    row.get::<_, Option<String>>(4)?, // thinking_content
                ))
            })?;

            for chat_row_result in chat_rows {
                let (mistake_id, role, content, timestamp_str, thinking_content) = chat_row_result?;
                let timestamp = chrono::DateTime::parse_from_rfc3339(&timestamp_str)
                    .map_err(|e| anyhow::anyhow!("Failed to parse timestamp: {}", e))?
                    .with_timezone(&Utc);
                
                let message = ChatMessage {
                    role,
                    content,
                    timestamp,
                    thinking_content,
                    rag_sources: None,
                    image_paths: None,
                    image_base64: None,
                };
                
                chat_messages.entry(mistake_id).or_insert_with(Vec::new).push(message);
            }
        }

        // Combine mistake data with chat messages
        for (id, subject, created_at_str, question_images_str, analysis_images_str, user_question, ocr_text, tags_str, mistake_type, status, updated_at_str) in mistake_data {
            let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse created_at: {}", e))?
                .with_timezone(&Utc);
            let updated_at = chrono::DateTime::parse_from_rfc3339(&updated_at_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse updated_at: {}", e))?
                .with_timezone(&Utc);
            let question_images: Vec<String> = serde_json::from_str(&question_images_str)?;
            let analysis_images: Vec<String> = serde_json::from_str(&analysis_images_str)?;
            let tags: Vec<String> = serde_json::from_str(&tags_str)?;
            
            let mistake = MistakeItem {
                id: id.clone(),
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
                chat_history: chat_messages.remove(&id).unwrap_or_default(),
                mistake_summary: None,       // 批量操作不加载总结
                user_error_analysis: None,   // 批量操作不加载分析
            };
            
            mistakes.push(mistake);
        }

        println!("✅ Batch exported {} mistakes with full data", mistakes.len());
        Ok(mistakes)
    }
}

/// Extension trait for Database to add batch operations
pub trait BatchOperationExt {
    fn with_batch_operations<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut BatchOperations) -> Result<R>;
}

impl BatchOperationExt for crate::database::Database {
    fn with_batch_operations<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut BatchOperations) -> Result<R>
    {
        let mut conn = self.conn().lock().unwrap();
        let mut batch_ops = BatchOperations::new(&mut conn);
        f(&mut batch_ops)
    }
}

/// Utility functions for common batch operations
pub mod batch_utils {
    use super::*;
    use crate::database::Database;

    /// High-level function to perform bulk import of mistakes
    pub fn bulk_import_mistakes(database: &Database, mistakes: &[MistakeItem]) -> Result<usize> {
        database.with_batch_operations(|batch_ops| {
            batch_ops.batch_save_mistakes(mistakes)
        })
    }

    /// High-level function to perform bulk cleanup operations
    pub fn bulk_cleanup(database: &Database, archive_days: Option<i64>) -> Result<(usize, usize)> {
        database.with_batch_operations(|batch_ops| {
            let orphaned = batch_ops.batch_cleanup_orphaned_messages()?;
            let archived = if let Some(days) = archive_days {
                batch_ops.batch_archive_old_mistakes(days)?
            } else {
                0
            };
            Ok((orphaned, archived))
        })
    }

    /// High-level function to bulk update mistake properties
    pub fn bulk_update_mistakes(
        database: &Database,
        status_updates: Option<&HashMap<String, String>>,
        tag_updates: Option<&HashMap<String, Vec<String>>>,
    ) -> Result<(usize, usize)> {
        database.with_batch_operations(|batch_ops| {
            let status_count = if let Some(updates) = status_updates {
                batch_ops.batch_update_mistake_statuses(updates)?
            } else {
                0
            };
            
            let tag_count = if let Some(updates) = tag_updates {
                batch_ops.batch_update_mistake_tags(updates)?
            } else {
                0
            };
            
            Ok((status_count, tag_count))
        })
    }

    /// High-level function for bulk export with progress reporting
    pub fn bulk_export_mistakes(database: &Database, mistake_ids: &[String]) -> Result<Vec<MistakeItem>> {
        const BATCH_SIZE: usize = 50; // Export in batches to avoid memory issues
        
        let mut all_mistakes = Vec::new();
        
        for chunk in mistake_ids.chunks(BATCH_SIZE) {
            let mut batch_mistakes = database.with_batch_operations(|batch_ops| {
                batch_ops.batch_export_mistakes(chunk)
            })?;
            
            all_mistakes.append(&mut batch_mistakes);
            println!("Exported batch: {} mistakes, total so far: {}", chunk.len(), all_mistakes.len());
        }
        
        Ok(all_mistakes)
    }
}