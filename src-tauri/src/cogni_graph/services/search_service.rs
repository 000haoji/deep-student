use crate::cogni_graph::{
    GraphConfig, ProblemCard, SearchRequest, SearchResult, Neo4jService, Tag, TagType
};
use crate::llm_manager::LLMManager;
use anyhow::{Result, anyhow};
use std::sync::Arc;
use std::collections::{HashMap, HashSet};
use tokio::sync::RwLock;

pub struct SearchService {
    neo4j: Neo4jService,
    llm_manager: Arc<LLMManager>,
    config: GraphConfig,
}

impl SearchService {
    pub async fn new(config: GraphConfig, llm_manager: Arc<LLMManager>) -> Result<Self> {
        let neo4j = Neo4jService::new(config.clone()).await?;

        Ok(Self {
            neo4j,
            llm_manager,
            config,
        })
    }

    pub async fn search(&self, request: SearchRequest) -> Result<Vec<SearchResult>> {
        let limit = request.limit.unwrap_or(self.config.max_search_results);

        // Step 1: Multi-path recall
        let (vector_results, fulltext_results) = self.multi_path_recall(&request.query, limit).await?;

        // Step 2: Merge and deduplicate results
        let merged_ids = self.merge_recall_results(vector_results, fulltext_results);

        // Step 3: Fetch full card data
        let cards_with_scores = self.fetch_cards_with_scores(merged_ids).await?;

        // Step 4: Re-rank using configurable weights
        let ranked_results = self.rerank_results(cards_with_scores, &request.query).await?;

        // Step 5: Apply final limit and return
        Ok(ranked_results.into_iter().take(limit).collect())
    }

    async fn multi_path_recall(&self, query: &str, limit: usize) -> Result<(Vec<(String, f32)>, Vec<(String, f32)>)> {
        // Generate query embedding for vector search
        let query_embedding = {
            let llm_manager = &self.llm_manager;
            let model_assignments = llm_manager.get_model_assignments().await.unwrap_or(crate::models::ModelAssignments {
                model1_config_id: None,
                model2_config_id: None,
                review_analysis_model_config_id: None,
                anki_card_model_config_id: None,
                embedding_model_config_id: None,
                reranker_model_config_id: None,
                summary_model_config_id: None,
            });
            let embedding_config_id = model_assignments.embedding_model_config_id
                .unwrap_or_else(|| "default".to_string());
            
            match llm_manager.call_embedding_api(vec![query.to_string()], &embedding_config_id).await {
                Ok(embeddings) if !embeddings.is_empty() => embeddings[0].clone(),
                _ => vec![0.0; self.config.vector_dimensions]
            }
        };

        // Execute both searches in parallel
        let (vector_future, fulltext_future) = tokio::join!(
            self.neo4j.vector_search(&query_embedding, limit),
            self.neo4j.fulltext_search(query, limit)
        );

        let vector_results = vector_future.unwrap_or_default();
        let fulltext_results = fulltext_future.unwrap_or_default();

        Ok((vector_results, fulltext_results))
    }

    fn merge_recall_results(&self, vector_results: Vec<(String, f32)>, fulltext_results: Vec<(String, f32)>) -> Vec<(String, f32, Vec<String>)> {
        let mut score_map: HashMap<String, (f32, Vec<String>)> = HashMap::new();

        // Add vector search results
        for (id, score) in vector_results {
            score_map.insert(id.clone(), (score, vec!["vector".to_string()]));
        }

        // Merge fulltext search results
        for (id, score) in fulltext_results {
            match score_map.get_mut(&id) {
                Some((existing_score, matched_by)) => {
                    // Average the scores if found by both methods
                    *existing_score = (*existing_score + score) / 2.0;
                    matched_by.push("fulltext".to_string());
                }
                None => {
                    score_map.insert(id.clone(), (score * 0.8, vec!["fulltext".to_string()])); // Weight fulltext slightly lower
                }
            }
        }

        // Convert to vector and sort by score
        let mut merged_results: Vec<(String, f32, Vec<String>)> = score_map
            .into_iter()
            .map(|(id, (score, matched_by))| (id, score, matched_by))
            .collect();

        merged_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        merged_results
    }

    async fn fetch_cards_with_scores(&self, merged_results: Vec<(String, f32, Vec<String>)>) -> Result<Vec<(ProblemCard, f32, Vec<String>)>> {
        let mut cards_with_scores = Vec::new();

        for (id, score, matched_by) in merged_results {
            if let Some(card) = self.neo4j.get_problem_card(&id).await? {
                cards_with_scores.push((card, score, matched_by));
            }
        }

        Ok(cards_with_scores)
    }

    async fn rerank_results(&self, cards_with_scores: Vec<(ProblemCard, f32, Vec<String>)>, query: &str) -> Result<Vec<SearchResult>> {
        // Configurable weights for different factors
        let vector_weight = 0.4;
        let fulltext_weight = 0.3;
        let access_count_weight = 0.1;
        let recency_weight = 0.1;
        let multi_match_bonus = 0.1;

        let mut search_results = Vec::new();
        let current_time = chrono::Utc::now();

        for (card, base_score, matched_by) in cards_with_scores {
            // Calculate component scores
            let mut final_score = base_score;

            // Access count factor (normalized)
            let access_factor = (card.access_count as f32).ln_1p() / 10.0; // Logarithmic scaling
            final_score += access_factor * access_count_weight;

            // Recency factor (days since last access, inverse relationship)
            let days_since_access = (current_time - card.last_accessed_at).num_days() as f32;
            let recency_factor = 1.0 / (1.0 + days_since_access / 30.0); // Decay over 30 days
            final_score += recency_factor * recency_weight;

            // Multi-match bonus (found by both vector and fulltext)
            if matched_by.len() > 1 {
                final_score += multi_match_bonus;
            }

            // Ensure score is within reasonable bounds
            final_score = final_score.min(1.0).max(0.0);

            search_results.push(SearchResult {
                card,
                score: final_score,
                matched_by,
            });
        }

        // Sort by final score
        search_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        Ok(search_results)
    }

    pub async fn search_similar_cards(&self, card_id: &str, limit: Option<usize>) -> Result<Vec<SearchResult>> {
        let card = match self.neo4j.get_problem_card(card_id).await? {
            Some(card) => card,
            None => return Err(anyhow!("Card not found: {}", card_id)),
        };

        // Use the card's content as search query
        let query = card.get_combined_content();
        
        let request = SearchRequest {
            query,
            limit,
            libraries: None,
        };

        let mut results = self.search(request).await?;
        
        // Remove the original card from results
        results.retain(|result| result.card.id != card_id);

        Ok(results)
    }

    pub async fn get_cards_by_tag(&self, tag_name: &str, limit: Option<usize>) -> Result<Vec<ProblemCard>> {
        // This would require a Cypher query to find all cards with a specific tag
        // For now, we'll implement a simple version
        let query = neo4rs::Query::new(
            r#"
            MATCH (pc:ProblemCard)-[:HAS_TAG]->(t:Tag {name: $tag_name})
            RETURN pc.id, pc.content_problem, pc.content_insight, pc.status, 
                   pc.embedding, pc.created_at, pc.last_accessed_at, 
                   pc.access_count, pc.source_excalidraw_path
            ORDER BY pc.last_accessed_at DESC
            LIMIT $limit
            "#.to_string()
        )
        .param("tag_name", tag_name)
        .param("limit", limit.unwrap_or(50) as i64);

        let rows = self.neo4j.execute_simple_query(query).await
            .map_err(|e| anyhow!("Failed to query cards by tag: {}", e))?;

        let mut cards = Vec::new();
        for row in rows {
            let embedding_str: String = row.get("pc.embedding").unwrap_or_default();
            let embedding = if embedding_str != "null" && !embedding_str.is_empty() {
                serde_json::from_str(&embedding_str).ok()
            } else {
                None
            };

            let card = ProblemCard {
                id: row.get("pc.id").unwrap_or_default(),
                content_problem: row.get("pc.content_problem").unwrap_or_default(),
                content_insight: row.get("pc.content_insight").unwrap_or_default(),
                status: row.get("pc.status").unwrap_or_default(),
                embedding,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String>("pc.created_at").unwrap_or_default())
                    .unwrap_or_default().with_timezone(&chrono::Utc),
                last_accessed_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String>("pc.last_accessed_at").unwrap_or_default())
                    .unwrap_or_default().with_timezone(&chrono::Utc),
                access_count: row.get("pc.access_count").unwrap_or_default(),
                source_excalidraw_path: {
                    let path: String = row.get("pc.source_excalidraw_path").unwrap_or_default();
                    if path.is_empty() { None } else { Some(path) }
                },
            };

            cards.push(card);
        }

        Ok(cards)
    }

    pub async fn get_all_tags(&self) -> Result<Vec<crate::cogni_graph::Tag>> {
        use crate::cogni_graph::Tag;
        
        let query = neo4rs::Query::new(
            r#"
            MATCH (t:Tag)
            RETURN t.name, t.type
            ORDER BY t.name
            "#.to_string()
        );

        let rows = self.neo4j.execute_simple_query(query).await
            .map_err(|e| anyhow!("Failed to query tags: {}", e))?;

        let mut tags = Vec::new();
        for row in rows {
            let tag = Tag {
                id: row.get("t.id").unwrap_or_default(),
                name: row.get("t.name").unwrap_or_default(),
                tag_type: TagType::Concept, // Default to concept for now
                level: row.get("t.level").unwrap_or(0) as i32,
                description: None,
                created_at: chrono::Utc::now(),
            };
            tags.push(tag);
        }

        Ok(tags)
    }
}

// Note: SearchService doesn't implement Clone due to Neo4jService limitations