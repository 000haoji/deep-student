# CogniGraph 知识图谱模块

## 概述

CogniGraph 是集成到 AI 错题管理器中的个性化知识网络系统，基于 Neo4j 图数据库实现。该模块允许用户将学习过程中的问题、灵感和解题方法结构化存储，并通过 AI 实现智能检索、关联推荐和深度洞察。

## 核心功能

### 🎯 主要特性

1. **问题卡片管理**
   - 创建和存储数学问题及解题洞察
   - 自动生成向量嵌入用于语义搜索
   - 支持标签分类和状态管理

2. **智能搜索**
   - 多路召回：向量搜索 + 全文检索
   - 融合排序：综合相似度、访问频次、时间因子
   - 语义理解：基于 AI 的内容理解

3. **AI 关系推荐**
   - 自动发现题目间的关联关系
   - 支持变题、通法、对比等关系类型
   - 基于置信度的推荐过滤

4. **知识图谱可视化**
   - 节点关系展示
   - 标签分类浏览
   - 访问统计分析

## 技术架构

### 后端架构

```
CogniGraph Module
├── models.rs           # 数据模型定义
├── handlers.rs         # Tauri 命令处理
├── config.rs          # 配置管理
└── services/
    ├── neo4j_service.rs    # Neo4j 数据库操作
    ├── graph_service.rs    # 图谱业务逻辑
    └── search_service.rs   # 搜索和推荐
```

### 数据模型

**ProblemCard 节点**
- `id`: 唯一标识符
- `content_problem`: 题目内容
- `content_insight`: 解题洞察
- `status`: 解题状态 (solved/unsolved)
- `embedding`: 向量嵌入
- `created_at`: 创建时间
- `last_accessed_at`: 最后访问时间
- `access_count`: 访问次数

**Tag 节点**
- `name`: 标签名称
- `tag_type`: 标签类型 (knowledge_point/method/auto)

**关系类型**
- `HAS_TAG`: 卡片拥有标签
- `IS_VARIATION_OF`: 题目变种关系
- `USES_GENERAL_METHOD`: 使用通用方法
- `CONTRASTS_WITH`: 对比关系

## 安装和配置

### 1. 安装 Neo4j

**本地开发**
```bash
# 下载 Neo4j Desktop
# https://neo4j.com/download/

# 或使用 Docker
docker run \
    --name neo4j \
    -p7474:7474 -p7687:7687 \
    -d \
    -v $HOME/neo4j/data:/data \
    -v $HOME/neo4j/logs:/logs \
    -v $HOME/neo4j/import:/var/lib/neo4j/import \
    -v $HOME/neo4j/plugins:/plugins \
    --env NEO4J_AUTH=neo4j/password \
    neo4j:latest
```

**云端部署**
1. 注册 [Neo4j AuraDB](https://neo4j.com/aura/)
2. 创建免费实例
3. 获取连接信息

### 2. 配置连接

复制配置文件模板：
```bash
cp cogni-graph-config.example.toml cogni-graph-config.toml
```

编辑配置文件：
```toml
[neo4j]
uri = "bolt://localhost:7687"
username = "neo4j"
password = "your-password"

[vector]
dimensions = 1536
similarity_threshold = 0.7
```

### 3. 初始化数据库

启动应用后，在知识图谱界面：
1. 输入 Neo4j 连接信息
2. 点击"测试连接"验证配置
3. 点击"初始化图谱"创建约束和索引

## 使用指南

### 创建问题卡片

1. 填写题目描述
2. 输入解题洞察和方法
3. 添加相关标签
4. 系统自动生成嵌入向量并建立关联

### 智能搜索

1. 输入搜索关键词
2. 系统执行多路召回
3. 查看排序后的搜索结果
4. 点击卡片查看详情和推荐

### AI 推荐

- 选择任意卡片后自动生成推荐
- 基于内容相似度和关系类型
- 显示推荐理由和置信度

## API 接口

### Tauri 命令

```typescript
// 初始化知识图谱
invoke('initialize_knowledge_graph', { config: GraphConfig })

// 创建问题卡片
invoke('create_problem_card', { request: CreateCardRequest })

// 搜索知识图谱
invoke('search_knowledge_graph', { request: SearchRequest })

// 获取 AI 推荐
invoke('get_ai_recommendations', { request: RecommendationRequest })

// 测试 Neo4j 连接
invoke('test_neo4j_connection', { config: GraphConfig })
```

### 数据类型

```typescript
interface CreateCardRequest {
  content_problem: string;
  content_insight: string;
  tags: string[];
  source_excalidraw_path?: string;
}

interface SearchRequest {
  query: string;
  limit?: number;
  libraries?: string[];
}
```

## 开发指南

### 添加新的关系类型

1. 在 `models.rs` 中扩展 `RelationshipType` 枚举
2. 更新 `graph_service.rs` 中的关系分析逻辑
3. 修改前端界面显示

### 扩展搜索算法

1. 在 `search_service.rs` 中添加新的召回方法
2. 更新 `rerank_results` 函数的权重计算
3. 调整配置参数

### 集成新的 AI 模型

系统已集成现有的 LLMManager，支持：
- OpenAI GPT 系列
- Google Gemini
- DeepSeek
- 其他兼容模型

## 性能优化

### 数据库优化

1. **索引策略**
   - 唯一约束：确保 ID 和标签名唯一性
   - 全文索引：支持快速内容搜索
   - 向量索引：加速相似度查询

2. **查询优化**
   - 使用参数化查询防止注入
   - 批量操作减少网络开销
   - 结果分页控制内存使用

### 向量搜索优化

1. **维度选择**
   - 使用 1536 维 OpenAI 嵌入
   - 支持自定义维度配置

2. **相似度计算**
   - 余弦相似度：标准化后的点积
   - 批量计算：优化大规模搜索性能

## 部署方案

### 本地开发到云端的无缝迁移

1. **开发阶段**
   - Neo4j Desktop 本地实例
   - 本地文件配置管理

2. **生产部署**
   - Neo4j AuraDB 云实例
   - 环境变量配置
   - Docker 容器化部署

### 扩展性考虑

- **数据量增长**：AuraDB 支持无缝扩容
- **并发访问**：连接池和查询优化
- **跨区域部署**：多区域实例同步

## 故障排除

### 常见问题

1. **连接失败**
   - 检查 Neo4j 服务状态
   - 验证网络连通性
   - 确认认证信息

2. **向量索引不可用**
   - 需要 Neo4j 5.11+ 版本
   - 安装向量搜索插件
   - 降级到手动计算模式

3. **性能问题**
   - 检查数据库约束和索引
   - 优化查询语句
   - 调整批量操作大小

### 日志调试

```bash
# 查看 Neo4j 日志
docker logs neo4j

# 查看应用日志
# 在应用内查看控制台输出
```

## 未来规划

### 短期目标 (v1.1)
- [ ] 手写输入识别集成
- [ ] 图谱可视化界面
- [ ] 导入/导出功能
- [ ] 批量操作支持

### 中期目标 (v1.2)
- [ ] 多媒体内容支持
- [ ] 协作学习功能
- [ ] 学习路径推荐
- [ ] 统计分析仪表板

### 长期目标 (v2.0)
- [ ] 分布式图谱
- [ ] 实时协作编辑
- [ ] 个性化学习算法
- [ ] 跨平台数据同步

## 贡献指南

欢迎提交 Issue 和 Pull Request！

### 开发环境设置

1. 克隆项目
2. 安装 Rust 和 Node.js
3. 配置 Neo4j 实例
4. 运行测试套件

### 代码规范

- Rust: 使用 `cargo fmt` 和 `cargo clippy`
- TypeScript: 使用 ESLint 和 Prettier
- 提交信息: 遵循 Conventional Commits

## 许可证

本项目遵循与主项目相同的许可证。

## 支持和反馈

如有问题或建议，请通过以下方式联系：

- GitHub Issues
- 项目讨论区
- 技术支持邮箱

---

**CogniGraph - 让知识连接，让学习更智能** 🧠✨