declare module 'neovis.js' {
  export interface Neo4jConfig {
    serverUrl: string;
    serverUser: string;
    serverPassword: string;
    serverDatabase?: string;
    driverConfig?: {
      encrypted?: string;
      trust?: string;
    };
  }

  export interface NeovisConfig {
    containerId: string;
    neo4j: Neo4jConfig;
    labels?: {
      [key: string]: {
        label?: string;
        value?: string;
        color?: string;
        size?: string;
        community?: string;
        caption?: string | string[];
        title_properties?: string[];
        font?: {
          color?: string;
          size?: number;
          face?: string;
        };
      };
    };
    relationships?: {
      [key: string]: {
        thickness?: string;
        color?: string;
        caption?: boolean | string;
      };
    };
    initialCypher?: string;
    consoleDebug?: boolean;
    visConfig?: any;
  }

  export interface NeovisNode {
    id: string;
    labels: string[];
    properties: Record<string, any>;
  }

  export interface NeovisRelationship {
    id: string;
    type: string;
    startNode: string;
    endNode: string;
    properties: Record<string, any>;
  }

  export class NeoVis {
    constructor(config: NeovisConfig);
    render(): void;
    reload(): void;
    clearNetwork(): void;
    stabilize(): void;
    renderWithCypher(cypher: string): void;
    registerOnEvent(event: string, callback: (event: any) => void): void;
    _addNode(nodeId: string, node: NeovisNode): void;
    _addRelationship(relationshipId: string, relationship: NeovisRelationship): void;
    
    // 网络实例（可选，有些版本可能暴露）
    _network?: any;
    _nodes?: any;
    _edges?: any;
  }

  export default NeoVis;
}