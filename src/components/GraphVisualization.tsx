import React, { useState, useRef, useEffect } from 'react';
import { GraphConfig, GraphNode, GraphRelationship } from '../types/cogni-graph';
import './GraphVisualization.css';

interface GraphVisualizationProps {
  config: GraphConfig;
  isInitialized: boolean;
}

const GraphVisualization: React.FC<GraphVisualizationProps> = ({ config, isInitialized }) => {
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedNode, setSelectedNode] = useState<any>(null);
  const [isInitializing, setIsInitializing] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const neovisRef = useRef<any>(null);

  useEffect(() => {
    if (isInitialized && containerRef.current) {
      initializeVisualization();
    }
    
    return () => {
      if (neovisRef.current) {
        try {
          neovisRef.current.clearNetwork();
        } catch (e) {
          console.warn('Error clearing network:', e);
        }
      }
    };
  }, [isInitialized, config]);

  const initializeVisualization = async () => {
    if (!containerRef.current || isInitializing) return;

    // 检查配置是否完整
    if (!config.neo4j.uri || !config.neo4j.username || !config.neo4j.password) {
      setError('请先完成 Neo4j 连接配置（URI、用户名、密码都不能为空）');
      return;
    }

    try {
      setIsInitializing(true);
      setIsLoading(true);
      setError(null);

      const NeoVis = (await import('neovis.js')).default;
      
      // 清理之前的实例
      if (neovisRef.current) {
        try {
          neovisRef.current.clearNetwork();
        } catch (e) {
          console.warn('Error clearing previous network:', e);
        }
      }

      const neovisConfig = {
        containerId: 'neo4j-viz',
        neo4j: {
          serverUrl: config.neo4j.uri,
          serverUser: config.neo4j.username,
          serverPassword: config.neo4j.password,
          serverDatabase: config.neo4j.database || 'neo4j'
        },
        labels: {
          ProblemCard: {
            label: 'content_problem',
            value: 'access_count',
            color: '#68CCE5',
            size: 'access_count',
            caption: ['content_problem'],
            title_properties: ['content_problem', 'content_insight', 'status'],
            font: {
              color: '#2c3e50',
              size: 12,
              face: 'Arial'
            }
          },
          Tag: {
            label: 'name',
            value: 'level',
            color: '#FFA500',
            size: 'level',
            caption: ['name'],
            title_properties: ['name', 'tag_type', 'description'],
            font: {
              color: '#2c3e50',
              size: 10,
              face: 'Arial'
            }
          }
        },
        relationships: {
          HAS_TAG: {
            thickness: '2',
            color: '#3498db',
            caption: false
          },
          PARENT_OF: {
            thickness: '3',
            color: '#e74c3c',
            caption: false
          },
          CHILD_OF: {
            thickness: '2',
            color: '#f39c12',
            caption: false
          },
          IS_VARIATION_OF: {
            thickness: '2',
            color: '#9b59b6',
            caption: false
          },
          USES_GENERAL_METHOD: {
            thickness: '2',
            color: '#27ae60',
            caption: false
          },
          CONTRASTS_WITH: {
            thickness: '2',
            color: '#e67e22',
            caption: false
          },
          RELATED_TO: {
            thickness: '1',
            color: '#95a5a6',
            caption: false
          },
          PREREQUISITE_OF: {
            thickness: '2',
            color: '#34495e',
            caption: false
          }
        },
        initialCypher: getInitialCypher(),
        consoleDebug: false
      };

      console.log('Initializing NeoVis with config:', neovisConfig);
      const viz = new NeoVis(neovisConfig);
      neovisRef.current = viz;

      // 检查可用的方法
      console.log('NeoVis instance created:', viz);

      // 安全的事件注册
      try {
        if (typeof viz.registerOnEvent === 'function') {
          viz.registerOnEvent('clickNode', (event: any) => {
            console.log('Node clicked:', event);
            setSelectedNode(event);
          });

          viz.registerOnEvent('clickEdge', (event: any) => {
            console.log('Edge clicked:', event);
          });

          // 尝试多种完成事件
          const completionEvents = ['completed', 'complete', 'finished', 'ready', 'renderComplete'];
          let eventRegistered = false;
          
          for (const eventName of completionEvents) {
            try {
              viz.registerOnEvent(eventName, () => {
                setIsLoading(false);
                setIsInitializing(false);
                console.log(`Graph visualization rendered (${eventName})`);
              });
              eventRegistered = true;
              console.log(`Successfully registered ${eventName} event`);
              break;
            } catch (e) {
              console.warn(`${eventName} event not supported`);
            }
          }

          // 如果没有完成事件，使用超时
          if (!eventRegistered) {
            console.log('No completion events available, using timeout fallback');
            setTimeout(() => {
              setIsLoading(false);
              setIsInitializing(false);
              console.log('Graph visualization completed (timeout)');
            }, 3000);
          }

          // 错误事件
          viz.registerOnEvent('error', (error: any) => {
            console.error('Visualization error:', error);
            setError(`可视化错误: ${error.message || error}`);
            setIsLoading(false);
            setIsInitializing(false);
          });
        } else {
          console.warn('registerOnEvent not available, using timeout');
          setTimeout(() => {
            setIsLoading(false);
            setIsInitializing(false);
          }, 3000);
        }
      } catch (e) {
        console.warn('Event registration failed:', e);
        setTimeout(() => {
          setIsLoading(false);
          setIsInitializing(false);
        }, 3000);
      }

      // 渲染图形
      viz.render();

    } catch (err) {
      console.error('Failed to initialize visualization:', err);
      setError(`可视化初始化失败: ${err}`);
      setIsLoading(false);
    } finally {
      setIsInitializing(false);
    }
  };

  const getInitialCypher = (): string => {
    return `
      MATCH (n)
      OPTIONAL MATCH (n)-[r]->(m)
      RETURN n, r, m
      LIMIT 50
    `;
  };

  const refreshVisualization = () => {
    if (neovisRef.current) {
      try {
        setIsLoading(true);
        neovisRef.current.reload();
        setTimeout(() => setIsLoading(false), 2000);
      } catch (err) {
        console.error('Failed to refresh visualization:', err);
        setError(`刷新失败: ${err}`);
        setIsLoading(false);
      }
    } else {
      initializeVisualization();
    }
  };

  const clearVisualization = () => {
    if (neovisRef.current) {
      try {
        neovisRef.current.clearNetwork();
        setSelectedNode(null);
      } catch (err) {
        console.error('Failed to clear visualization:', err);
      }
    }
  };

  const customQuery = (cypher: string) => {
    if (neovisRef.current && cypher.trim()) {
      try {
        setIsLoading(true);
        neovisRef.current.renderWithCypher(cypher);
        setTimeout(() => setIsLoading(false), 2000);
      } catch (err) {
        console.error('Failed to execute custom query:', err);
        setError(`查询执行失败: ${err}`);
        setIsLoading(false);
      }
    }
  };

  if (!isInitialized) {
    return (
      <div className="graph-visualization-container">
        <div className="not-initialized">
          <p>图谱未初始化，请先完成Neo4j连接配置</p>
        </div>
      </div>
    );
  }

  return (
    <div className="graph-visualization-container">
      <div className="visualization-header">
        <h3>图谱可视化</h3>
        <div className="visualization-controls">
          <button onClick={refreshVisualization} disabled={isLoading}>
            🔄 刷新
          </button>
          <button onClick={clearVisualization} disabled={isLoading}>
            🗑️ 清空
          </button>
          <button 
            onClick={() => customQuery('MATCH (n:ProblemCard) RETURN n LIMIT 20')} 
            disabled={isLoading}
          >
            📚 显示问题卡片
          </button>
          <button 
            onClick={() => customQuery('MATCH (n:Tag) RETURN n LIMIT 20')} 
            disabled={isLoading}
          >
            🏷️ 显示标签
          </button>
        </div>
      </div>

      {error && (
        <div className="error-message">
          {error}
          <button onClick={() => setError(null)}>✕</button>
        </div>
      )}

      <div className="visualization-content">
        <div className="graph-container">
          <div 
            id="neo4j-viz" 
            ref={containerRef}
            className="neo4j-visualization"
          />
          {isLoading && (
            <div className="loading-overlay">
              <div className="loading-spinner">加载图谱中...</div>
            </div>
          )}
        </div>

        {selectedNode && (
          <div className="node-details">
            <h4>节点详情</h4>
            <div className="node-info">
              <strong>ID:</strong> {selectedNode.id}<br/>
              <strong>Labels:</strong> {selectedNode.labels?.join(', ')}<br/>
              <strong>Properties:</strong>
              <pre>{JSON.stringify(selectedNode.properties, null, 2)}</pre>
            </div>
            <button onClick={() => setSelectedNode(null)}>关闭</button>
          </div>
        )}
      </div>
    </div>
  );
};

export default GraphVisualization;