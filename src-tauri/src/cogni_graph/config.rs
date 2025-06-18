use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Neo4jConfig {
    pub uri: String,
    pub username: String,
    pub password: String,
    pub database: Option<String>,
}

impl Default for Neo4jConfig {
    fn default() -> Self {
        Self {
            uri: "bolt://localhost:7687".to_string(),
            username: "neo4j".to_string(),
            password: "password".to_string(),
            database: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GraphConfig {
    pub neo4j: Neo4jConfig,
    pub vector_dimensions: usize,
    pub similarity_threshold: f32,
    pub max_search_results: usize,
    pub recommendation_limit: usize,
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            neo4j: Neo4jConfig::default(),
            vector_dimensions: 1536, // OpenAI embedding dimensions
            similarity_threshold: 0.7,
            max_search_results: 100,
            recommendation_limit: 10,
        }
    }
}