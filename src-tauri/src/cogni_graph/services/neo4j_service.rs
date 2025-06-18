use crate::cogni_graph::{GraphConfig, ProblemCard, Tag, RelationshipType, CreateTagRequest, TagHierarchy, TagType};
use anyhow::{Result, anyhow};
use neo4rs::{Graph, Query};
use serde_json::Value;
use std::collections::HashMap;

pub struct Neo4jService {
    pub graph: Graph,
    config: GraphConfig,
}

impl Neo4jService {
    pub async fn new(config: GraphConfig) -> Result<Self> {
        let graph = Graph::new(&config.neo4j.uri, &config.neo4j.username, &config.neo4j.password)
            .await
            .map_err(|e| anyhow!("Failed to connect to Neo4j at {}: {}. Please check that Neo4j is running and credentials are correct.", config.neo4j.uri, e))?;

        Ok(Self { graph, config })
    }

    pub async fn initialize_schema(&self) -> Result<()> {
        // Create constraints and indexes as per the design document
        let queries = vec![
            // Create unique constraints
            "CREATE CONSTRAINT pc_id IF NOT EXISTS FOR (n:ProblemCard) REQUIRE n.id IS UNIQUE",
            "CREATE CONSTRAINT tag_id IF NOT EXISTS FOR (n:Tag) REQUIRE n.id IS UNIQUE",
            
            // Create full-text index for content search (Neo4j 4.x/5.x compatible syntax)
            "CREATE FULLTEXT INDEX problem_card_content IF NOT EXISTS FOR (n:ProblemCard) ON EACH [n.content_problem, n.content_insight]",
            
            // Create vector index for embeddings (Note: this requires Neo4j 5.11+ with vector search plugin)
            "CREATE VECTOR INDEX problem_card_embedding IF NOT EXISTS FOR (n:ProblemCard) ON (n.embedding) OPTIONS { indexConfig: { `vector.dimensions`: 1536, `vector.similarity_function`: 'cosine' }}"
        ];

        for query_str in queries {
            let query = Query::new(query_str.to_string());
            if let Err(e) = self.execute_simple_query(query).await {
                // Log warning for advanced features that may not be supported
                if query_str.contains("VECTOR INDEX") {
                    eprintln!("Warning: Vector index creation failed (may not be supported in this Neo4j version): {}", e);
                } else if query_str.contains("FULLTEXT INDEX") {
                    eprintln!("Warning: Fulltext index creation failed (trying alternative approach): {}", e);
                    // Try alternative fulltext syntax for older versions
                    let alt_query = "CREATE FULLTEXT INDEX problem_card_content IF NOT EXISTS FOR (n:ProblemCard) ON (n.content_problem, n.content_insight)";
                    if let Err(e2) = self.execute_simple_query(Query::new(alt_query.to_string())).await {
                        eprintln!("Warning: Alternative fulltext index syntax also failed: {}", e2);
                    }
                } else {
                    return Err(anyhow!("Failed to execute schema query '{}': {}", query_str, e));
                }
            }
        }

        Ok(())
    }

    pub async fn create_problem_card(&self, mut card: ProblemCard, tags: Vec<String>) -> Result<String> {
        let card_id = card.id.clone();
        
        // Convert embedding vector to string representation for Neo4j
        let embedding_str = if let Some(ref embedding) = card.embedding {
            serde_json::to_string(embedding)?
        } else {
            "null".to_string()
        };

        let query = Query::new(
            r#"
            CREATE (pc:ProblemCard {
                id: $id,
                content_problem: $content_problem,
                content_insight: $content_insight,
                status: $status,
                embedding: $embedding,
                created_at: $created_at,
                last_accessed_at: $last_accessed_at,
                access_count: $access_count,
                source_excalidraw_path: $source_excalidraw_path
            })
            WITH pc
            UNWIND $tags_list AS tag_name
            MERGE (t:Tag {name: tag_name, tag_type: 'concept', level: 2})
            ON CREATE SET t.id = randomUUID(), t.created_at = datetime()
            CREATE (pc)-[:HAS_TAG]->(t)
            RETURN pc.id
            "#.to_string()
        )
        .param("id", card.id.clone())
        .param("content_problem", card.content_problem.clone())
        .param("content_insight", card.content_insight.clone()) 
        .param("status", card.status.clone())
        .param("embedding", embedding_str)
        .param("created_at", card.created_at.to_rfc3339())
        .param("last_accessed_at", card.last_accessed_at.to_rfc3339())
        .param("access_count", card.access_count)
        .param("source_excalidraw_path", card.source_excalidraw_path.clone().unwrap_or_default())
        .param("tags_list", tags);

        let mut result = self.graph.execute(query).await
            .map_err(|e| anyhow!("Failed to create problem card: {}", e))?;

        if let Some(row) = result.next().await? {
            let id: String = row.get("pc.id").unwrap_or_default();
            Ok(id)
        } else {
            Err(anyhow!("Failed to create problem card: no result returned"))
        }
    }

    pub async fn get_problem_card(&self, card_id: &str) -> Result<Option<ProblemCard>> {
        let query = Query::new(
            r#"
            MATCH (pc:ProblemCard {id: $id})
            RETURN pc.id, pc.content_problem, pc.content_insight, pc.status, 
                   pc.embedding, pc.created_at, pc.last_accessed_at, 
                   pc.access_count, pc.source_excalidraw_path
            "#.to_string()
        ).param("id", card_id);

        let mut result = self.graph.execute(query).await
            .map_err(|e| anyhow!("Failed to query problem card: {}", e))?;

        if let Some(row) = result.next().await? {
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

            Ok(Some(card))
        } else {
            Ok(None)
        }
    }

    pub async fn vector_search(&self, query_vector: &[f32], limit: usize) -> Result<Vec<(String, f32)>> {
        // Try vector index search first (if supported)
        let vector_str = serde_json::to_string(query_vector)?;
        
        let query = Query::new(
            r#"
            CALL db.index.vector.queryNodes('problem_card_embedding', $limit, $query_vector)
            YIELD node, score
            RETURN node.id, score
            ORDER BY score DESC
            "#.to_string()
        )
        .param("limit", limit as i64)
        .param("query_vector", vector_str);

        match self.graph.execute(query).await {
            Ok(mut result) => {
                let mut results = Vec::new();
                while let Some(row) = result.next().await? {
                    let id: String = row.get("node.id").unwrap_or_default();
                    let score: f64 = row.get("score").unwrap_or_default();
                    results.push((id, score as f32));
                }
                Ok(results)
            }
            Err(_) => {
                // Fallback to manual vector search if index is not available
                self.manual_vector_search(query_vector, limit).await
            }
        }
    }

    async fn manual_vector_search(&self, query_vector: &[f32], limit: usize) -> Result<Vec<(String, f32)>> {
        let query = Query::new(
            r#"
            MATCH (pc:ProblemCard)
            WHERE pc.embedding IS NOT NULL AND pc.embedding <> 'null'
            RETURN pc.id, pc.embedding
            "#.to_string()
        );

        let mut result = self.graph.execute(query).await
            .map_err(|e| anyhow!("Failed to execute manual vector search: {}", e))?;

        let mut results = Vec::new();
        while let Some(row) = result.next().await? {
            let id: String = row.get("pc.id").unwrap_or_default();
            let embedding_str: String = row.get("pc.embedding").unwrap_or_default();
            
            if let Ok(embedding) = serde_json::from_str::<Vec<f32>>(&embedding_str) {
                let similarity = cosine_similarity(query_vector, &embedding);
                results.push((id, similarity));
            }
        }

        // Sort by similarity score (descending) and take top results
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);

        Ok(results)
    }

    pub async fn fulltext_search(&self, query: &str, limit: usize) -> Result<Vec<(String, f32)>> {
        let query = Query::new(
            r#"
            CALL db.index.fulltext.queryNodes('problem_card_content', $query_string, {limit: $limit})
            YIELD node, score
            RETURN node.id, score
            ORDER BY score DESC
            "#.to_string()
        )
        .param("query_string", query)
        .param("limit", limit as i64);

        let mut result = self.graph.execute(query).await
            .map_err(|e| anyhow!("Failed to execute fulltext search: {}", e))?;

        let mut results = Vec::new();
        while let Some(row) = result.next().await? {
            let id: String = row.get("node.id").unwrap_or_default();
            let score: f64 = row.get("score").unwrap_or_default();
            results.push((id, score as f32));
        }

        Ok(results)
    }

    pub async fn get_card_relationships(&self, card_id: &str) -> Result<Vec<(String, String, f32)>> {
        let query = Query::new(
            r#"
            MATCH (c:ProblemCard {id: $candidate_id})
            OPTIONAL MATCH (c)-[r]-()
            RETURN c.access_count as access_count, count(r) AS degree
            "#.to_string()
        ).param("candidate_id", card_id);

        let mut result = self.graph.execute(query).await
            .map_err(|e| anyhow!("Failed to get card relationships: {}", e))?;

        if let Some(row) = result.next().await? {
            let access_count: i64 = row.get("access_count").unwrap_or_default();
            let degree: i64 = row.get("degree").unwrap_or_default();
            
            // Calculate a simple value score based on relationships and access
            let value_score = (access_count as f32 * 0.1) + (degree as f32 * 0.5);
            
            Ok(vec![(card_id.to_string(), "value_assessment".to_string(), value_score)])
        } else {
            Ok(vec![])
        }
    }

    pub async fn create_relationship(&self, from_id: &str, to_id: &str, rel_type: RelationshipType) -> Result<()> {
        let rel_type_str = rel_type.to_string();
        
        let query = Query::new(
            format!(
                r#"
                MATCH (from:ProblemCard {{id: $from_id}})
                MATCH (to:ProblemCard {{id: $to_id}})
                CREATE (from)-[:{}]->(to)
                "#,
                rel_type_str
            )
        )
        .param("from_id", from_id)
        .param("to_id", to_id);

        self.graph.execute(query).await
            .map_err(|e| anyhow!("Failed to create relationship: {}", e))?;

        Ok(())
    }

    pub async fn update_access_count(&self, card_id: &str) -> Result<()> {
        let query = Query::new(
            r#"
            MATCH (pc:ProblemCard {id: $id})
            SET pc.last_accessed_at = $timestamp, pc.access_count = pc.access_count + 1
            "#.to_string()
        )
        .param("id", card_id)
        .param("timestamp", chrono::Utc::now().to_rfc3339());

        self.graph.execute(query).await
            .map_err(|e| anyhow!("Failed to update access count: {}", e))?;

        Ok(())
    }

    // Tag management methods
    pub async fn create_tag(&self, request: CreateTagRequest) -> Result<String> {
        let tag = Tag::new(
            request.name.clone(),
            request.tag_type.clone(),
            if request.parent_id.is_some() { 1 } else { 0 }, // Auto-calculate level
            request.description.clone(),
        );

        let query = Query::new(
            r#"
            CREATE (t:Tag {
                id: $id,
                name: $name,
                tag_type: $tag_type,
                level: $level,
                description: $description,
                created_at: $created_at
            })
            RETURN t.id
            "#.to_string()
        )
        .param("id", tag.id.clone())
        .param("name", tag.name.clone())
        .param("tag_type", tag.get_type_string())
        .param("level", tag.level)
        .param("description", tag.description.clone().unwrap_or_default())
        .param("created_at", tag.created_at.to_rfc3339());

        let mut result = self.graph.execute(query).await
            .map_err(|e| anyhow!("Failed to create tag: {}", e))?;

        // Create parent relationship if specified
        if let Some(parent_id) = request.parent_id {
            let rel_query = Query::new(
                r#"
                MATCH (parent:Tag {id: $parent_id})
                MATCH (child:Tag {id: $child_id})
                CREATE (parent)-[:PARENT_OF]->(child)
                CREATE (child)-[:CHILD_OF]->(parent)
                SET child.level = parent.level + 1
                "#.to_string()
            )
            .param("parent_id", parent_id)
            .param("child_id", tag.id.clone());

            self.graph.execute(rel_query).await
                .map_err(|e| anyhow!("Failed to create parent relationship: {}", e))?;
        }

        Ok(tag.id)
    }

    pub async fn get_tag_hierarchy(&self, root_tag_id: Option<String>) -> Result<Vec<TagHierarchy>> {
        let query = if let Some(root_id) = root_tag_id {
            Query::new(
                r#"
                MATCH (root:Tag {id: $root_id})
                CALL {
                    WITH root
                    MATCH path = (root)-[:PARENT_OF*0..]->(descendant:Tag)
                    RETURN descendant, length(path) as depth
                    ORDER BY depth, descendant.name
                }
                RETURN descendant as tag, depth
                "#.to_string()
            )
            .param("root_id", root_id)
        } else {
            Query::new(
                r#"
                MATCH (tag:Tag)
                WHERE NOT (tag)<-[:PARENT_OF]-()
                CALL {
                    WITH tag
                    MATCH path = (tag)-[:PARENT_OF*0..]->(descendant:Tag)
                    RETURN descendant, length(path) as depth
                    ORDER BY depth, descendant.name
                }
                RETURN descendant as tag, depth
                "#.to_string()
            )
        };

        let rows = self.execute_simple_query(query).await?;
        let mut hierarchies = Vec::new();

        // Build hierarchy structure (simplified version)
        for row in rows {
            if let (Ok(tag_node), Ok(depth)) = (row.get::<neo4rs::Node>("tag"), row.get::<i64>("depth")) {
                let tag = self.node_to_tag(tag_node)?;
                
                // For now, return a flat structure. Full hierarchy building would be more complex
                hierarchies.push(TagHierarchy {
                    tag,
                    children: Vec::new(),
                    parent: None,
                });
            }
        }

        Ok(hierarchies)
    }

    pub async fn get_tags_by_type(&self, tag_type: TagType) -> Result<Vec<Tag>> {
        let query = Query::new(
            r#"
            MATCH (t:Tag {tag_type: $tag_type})
            RETURN t
            ORDER BY t.level, t.name
            "#.to_string()
        )
        .param("tag_type", tag_type.to_string());

        let rows = self.execute_simple_query(query).await?;
        let mut tags = Vec::new();

        for row in rows {
            if let Ok(tag_node) = row.get::<neo4rs::Node>("t") {
                tags.push(self.node_to_tag(tag_node)?);
            }
        }

        Ok(tags)
    }

    fn node_to_tag(&self, node: neo4rs::Node) -> Result<Tag> {
        let id: String = node.get("id").unwrap_or_default();
        let name: String = node.get("name").unwrap_or_default();
        let tag_type_str: String = node.get("tag_type").unwrap_or_default();
        let level: i64 = node.get("level").unwrap_or(0);
        let description: String = node.get("description").unwrap_or_default();
        let created_at_str: String = node.get("created_at").unwrap_or_default();

        let tag_type = match tag_type_str.as_str() {
            "knowledge_area" => TagType::KnowledgeArea,
            "topic" => TagType::Topic,
            "concept" => TagType::Concept,
            "method" => TagType::Method,
            "difficulty" => TagType::Difficulty,
            _ => TagType::Concept,
        };

        let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
            .unwrap_or_else(|_| chrono::Utc::now().into())
            .with_timezone(&chrono::Utc);

        Ok(Tag {
            id,
            name,
            tag_type,
            level: level as i32,
            description: if description.is_empty() { None } else { Some(description) },
            created_at,
        })
    }

    // Helper method to execute queries and return results
    pub async fn execute_simple_query(&self, query: Query) -> Result<Vec<neo4rs::Row>> {
        let mut result = self.graph.execute(query).await
            .map_err(|e| anyhow!("Failed to execute query: {}", e))?;
        
        let mut rows = Vec::new();
        while let Ok(Some(row)) = result.next().await {
            rows.push(row);
        }
        Ok(rows)
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        0.0
    } else {
        dot_product / (magnitude_a * magnitude_b)
    }
}