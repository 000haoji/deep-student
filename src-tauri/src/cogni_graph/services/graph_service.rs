use crate::cogni_graph::{
    GraphConfig, ProblemCard, Tag, CreateCardRequest, RecommendationRequest, 
    Recommendation, RelationshipType, Neo4jService, CreateTagRequest, TagHierarchy, TagType
};
use crate::llm_manager::LLMManager;
use anyhow::{Result, anyhow};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct GraphService {
    neo4j: Neo4jService,
    llm_manager: Arc<LLMManager>,
    config: GraphConfig,
}

impl GraphService {
    pub async fn new(config: GraphConfig, llm_manager: Arc<LLMManager>) -> Result<Self> {
        let neo4j = Neo4jService::new(config.clone()).await?;
        
        // Initialize Neo4j schema
        neo4j.initialize_schema().await?;

        Ok(Self {
            neo4j,
            llm_manager,
            config,
        })
    }

    pub async fn create_problem_card(&self, request: CreateCardRequest) -> Result<String> {
        // Create initial problem card
        let mut card = ProblemCard::new(
            request.content_problem.clone(),
            request.content_insight.clone(),
            request.source_excalidraw_path,
        );

        // Generate embedding using existing LLM manager
        let combined_content = card.get_combined_content();
        let embedding = self.generate_embedding(&combined_content).await?;
        card.embedding = Some(embedding);

        // Store in Neo4j with tags
        let card_id = self.neo4j.create_problem_card(card, request.tags).await?;

        // Note: Skip async AI recommendations for now due to Clone limitations
        // This can be implemented later using shared service instances or message passing
        eprintln!("Note: AI recommendations will be generated on-demand via get_recommendations API");

        Ok(card_id)
    }

    pub async fn get_problem_card(&self, card_id: &str) -> Result<Option<ProblemCard>> {
        // Update access count
        if let Err(e) = self.neo4j.update_access_count(card_id).await {
            eprintln!("Failed to update access count for card {}: {}", card_id, e);
        }

        self.neo4j.get_problem_card(card_id).await
    }

    async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
        let llm_manager = &self.llm_manager;
        
        // Get embedding model assignment
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
        
        match llm_manager.call_embedding_api(vec![text.to_string()], &embedding_config_id).await {
            Ok(embeddings) if !embeddings.is_empty() => Ok(embeddings[0].clone()),
            Ok(_) => {
                eprintln!("No embeddings returned");
                Ok(vec![0.0; self.config.vector_dimensions])
            },
            Err(e) => {
                eprintln!("Failed to generate embedding: {}", e);
                // Return zero vector as fallback
                Ok(vec![0.0; self.config.vector_dimensions])
            }
        }
    }

    // Tag management methods
    pub async fn create_tag(&self, request: CreateTagRequest) -> Result<String> {
        self.neo4j.create_tag(request).await
    }

    pub async fn get_tag_hierarchy(&self, root_tag_id: Option<String>) -> Result<Vec<TagHierarchy>> {
        self.neo4j.get_tag_hierarchy(root_tag_id).await
    }

    pub async fn get_tags_by_type(&self, tag_type: TagType) -> Result<Vec<Tag>> {
        self.neo4j.get_tags_by_type(tag_type).await
    }

    async fn generate_ai_recommendations(&self, card_id: &str) -> Result<()> {
        // Get the card details
        let card = match self.neo4j.get_problem_card(card_id).await? {
            Some(card) => card,
            None => return Err(anyhow!("Card not found: {}", card_id)),
        };

        // Find candidate cards using multi-path recall
        let candidates = self.find_candidate_cards(&card).await?;

        // Generate AI recommendations for promising candidates
        for (candidate_id, similarity_score) in candidates {
            if similarity_score > self.config.similarity_threshold {
                if let Some(candidate_card) = self.neo4j.get_problem_card(&candidate_id).await? {
                    // Use LLM to determine relationship type and confidence
                    if let Ok(recommendation) = self.analyze_relationship(&card, &candidate_card).await {
                        if recommendation.confidence > 0.7 {
                            // Create the relationship in Neo4j
                            if let Err(e) = self.neo4j.create_relationship(
                                &card.id,
                                &candidate_card.id,
                                recommendation.relationship_type.clone()
                            ).await {
                                eprintln!("Failed to create relationship: {}", e);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn find_candidate_cards(&self, card: &ProblemCard) -> Result<Vec<(String, f32)>> {
        let mut candidates = Vec::new();

        // Vector similarity search
        if let Some(ref embedding) = card.embedding {
            if let Ok(vector_results) = self.neo4j.vector_search(embedding, 50).await {
                candidates.extend(vector_results);
            }
        }

        // Full-text search
        let search_text = format!("{} {}", card.content_problem, card.content_insight);
        if let Ok(fulltext_results) = self.neo4j.fulltext_search(&search_text, 50).await {
            for (id, score) in fulltext_results {
                // Merge with existing results or add new ones
                if let Some(existing) = candidates.iter_mut().find(|(existing_id, _)| existing_id == &id) {
                    existing.1 = (existing.1 + score) / 2.0; // Average the scores
                } else {
                    candidates.push((id, score * 0.8)); // Weight fulltext lower than vector
                }
            }
        }

        // Filter out the card itself
        candidates.retain(|(id, _)| id != &card.id);

        // Sort by combined score
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top candidates
        candidates.truncate(20);

        Ok(candidates)
    }

    async fn analyze_relationship(&self, card1: &ProblemCard, card2: &ProblemCard) -> Result<Recommendation> {
        let prompt = format!(
            r#"Analyze the relationship between these two problem cards:

Card 1:
Problem: {}
Insight: {}

Card 2:
Problem: {}
Insight: {}

Determine the most appropriate relationship type and provide a confidence score (0.0-1.0).

Respond in JSON format:
{{
    "relationship_type": "IS_VARIATION_OF" | "USES_GENERAL_METHOD" | "CONTRASTS_WITH" | "SIMILAR_CONCEPT",
    "confidence": 0.0-1.0,
    "reasoning": "Explanation for this relationship"
}}

Only suggest high-confidence relationships (>0.7) that would be valuable for learning."#,
            card1.content_problem,
            card1.content_insight,
            card2.content_problem,
            card2.content_insight
        );

        let llm_manager = &self.llm_manager;
        let response = llm_manager.call_unified_model_1(
            vec![], // No images
            &prompt,
            "数学", // Default subject  
            Some("Relationship analysis task")
        ).await.map_err(|e| anyhow!("Failed to analyze relationship: {}", e))?;

        // For now, create a simple recommendation based on OCR result
        // In a full implementation, this would parse JSON from the response
        let relationship_type = RelationshipType::UsesGeneralMethod; // Default
        let confidence = 0.8; // Default confidence
        let reasoning = if !response.ocr_text.is_empty() {
            response.ocr_text
        } else {
            "Similar problem pattern detected".to_string()
        };

        Ok(Recommendation {
            card: card2.clone(),
            relationship_type,
            confidence,
            reasoning,
        })
    }

    pub async fn get_recommendations(&self, request: RecommendationRequest) -> Result<Vec<Recommendation>> {
        let card = match self.neo4j.get_problem_card(&request.card_id).await? {
            Some(card) => card,
            None => return Err(anyhow!("Card not found: {}", request.card_id)),
        };

        let candidates = self.find_candidate_cards(&card).await?;
        let limit = request.limit.unwrap_or(self.config.recommendation_limit);

        let mut recommendations = Vec::new();
        
        for (candidate_id, _) in candidates.into_iter().take(limit * 2) { // Get more candidates than needed
            if let Some(candidate_card) = self.neo4j.get_problem_card(&candidate_id).await? {
                if let Ok(recommendation) = self.analyze_relationship(&card, &candidate_card).await {
                    recommendations.push(recommendation);
                }
                
                if recommendations.len() >= limit {
                    break;
                }
            }
        }

        // Sort by confidence
        recommendations.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

        Ok(recommendations)
    }
}

// Note: GraphService doesn't implement Clone due to Neo4jService limitations
// For async spawning, we'll create a new service instance when needed

// Note: Neo4jService doesn't implement Clone due to private fields
// We'll need to handle this differently in the async spawning