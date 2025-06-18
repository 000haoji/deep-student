export interface ProblemCard {
  id: string;
  content_problem: string;
  content_insight: string;
  status: 'unsolved' | 'solved';
  embedding?: number[];
  created_at: string;
  last_accessed_at: string;
  access_count: number;
  source_excalidraw_path?: string;
}

export interface Tag {
  id: string;
  name: string;
  tag_type: TagType;
  level: number;
  description?: string;
  created_at: string;
}

export type TagType = 
  | 'KnowledgeArea'
  | 'Topic' 
  | 'Concept'
  | 'Method'
  | 'Difficulty';

export type RelationshipType = 
  | 'HAS_TAG'
  | 'IS_VARIATION_OF'
  | 'USES_GENERAL_METHOD'
  | 'CONTRASTS_WITH'
  | 'PARENT_OF'
  | 'CHILD_OF'
  | 'RELATED_TO'
  | 'PREREQUISITE_OF';

export interface Relationship {
  from_id: string;
  to_id: string;
  relationship_type: RelationshipType;
}

export interface SearchRequest {
  query: string;
  limit?: number;
  libraries?: string[];
}

export interface SearchResult {
  card: ProblemCard;
  score: number;
  matched_by: string[];
}

export interface RecommendationRequest {
  card_id: string;
  limit?: number;
}

export interface Recommendation {
  card: ProblemCard;
  relationship_type: RelationshipType;
  confidence: number;
  reasoning: string;
}

export interface CreateCardRequest {
  content_problem: string;
  content_insight: string;
  tags: string[];
  source_excalidraw_path?: string;
}

export interface CreateTagRequest {
  name: string;
  tag_type: TagType;
  parent_id?: string;
  description?: string;
}

export interface TagHierarchy {
  tag: Tag;
  children: TagHierarchy[];
  parent?: Tag;
}

export interface GraphNode {
  id: string;
  label: string;
  type: 'ProblemCard' | 'Tag';
  properties: Record<string, any>;
}

export interface GraphEdge {
  id: string;
  from: string;
  to: string;
  type: RelationshipType;
  properties?: Record<string, any>;
}

export interface GraphData {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export interface Neo4jConfig {
  uri: string;
  username: string;
  password: string;
  database?: string;
}

export interface GraphConfig {
  neo4j: Neo4jConfig;
  vector_dimensions: number;
  similarity_threshold: number;
  max_search_results: number;
  recommendation_limit: number;
}

// Tauri API types
export interface KnowledgeGraphAPI {
  initializeKnowledgeGraph: (config: GraphConfig) => Promise<string>;
  createProblemCard: (request: CreateCardRequest) => Promise<string>;
  getProblemCard: (cardId: string) => Promise<ProblemCard | null>;
  searchKnowledgeGraph: (request: SearchRequest) => Promise<SearchResult[]>;
  getAiRecommendations: (request: RecommendationRequest) => Promise<Recommendation[]>;
  searchSimilarCards: (cardId: string, limit?: number) => Promise<SearchResult[]>;
  getCardsByTag: (tagName: string, limit?: number) => Promise<ProblemCard[]>;
  getAllTags: () => Promise<Tag[]>;
  getGraphConfig: () => Promise<GraphConfig>;
  updateGraphConfig: (config: GraphConfig) => Promise<string>;
  testNeo4jConnection: (config: GraphConfig) => Promise<string>;
  processHandwrittenInput: (imageData: string) => Promise<CreateCardRequest>;
  
  // Tag management APIs
  createTag: (request: CreateTagRequest) => Promise<string>;
  getTagHierarchy: (rootTagId?: string) => Promise<TagHierarchy[]>;
  getTagsByType: (tagType: TagType) => Promise<Tag[]>;
  initializeDefaultTagHierarchy: () => Promise<string>;
}