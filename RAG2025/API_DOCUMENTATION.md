# RAG知识库系统 API 文档

## 概述
本文档提供RAG知识库系统所有API端点的详细信息，包括请求方法、参数、响应格式等。这些API可以用于文档管理、查询、文档库管理和系统维护等操作。

## 基础URL
所有API都基于以下基础URL：
```
http://localhost:5000
```
端口可能根据系统配置有所不同。

## 系统数据结构

系统的后台数据按逻辑分为以下几个部分，它们都存储在 `DATA_DIR`（默认为"data"目录）中：

### 文档数据 (documents.sqlite)
存储路径：`DATA_DIR/documents.sqlite`

这是一个SQLite数据库，使用 `sqlitedict` 库进行管理，存储所有上传到系统的文档。每个文档记录包括：
- 文档ID作为键
- 文档内容
- 文档元数据，包括：
  - 标题 (title)
  - 作者 (author)
  - 文件类型 (file_type)
  - 上传时间 (uploaded_at)
  - 父文档ID (parent_id) - 用于文档分段关系
  - 文档库ID (library_id) - 文档所属的库
  - 其他自定义元数据

### 向量数据 (vectors.sqlite)
存储路径：`DATA_DIR/vectors.sqlite`

另一个SQLite数据库，存储文档的嵌入向量：
- 键: 文档ID
- 值: 文档的嵌入向量数组
- 这些向量用于相似度检索，支持RAG问答功能

### 元数据 (metadata.json)
存储路径：`DATA_DIR/metadata.json`

JSON格式文件，存储系统全局元数据：
- 文档总数
- 文档库列表及其信息
- 每个文档的基本信息摘要
- 系统配置信息

### 备份数据 (backups目录)
存储路径：`DATA_DIR/backups`

每个备份都是一个单独的目录，格式为`backup_{timestamp}`，包含：
- 向量数据库的副本
- 文档数据库的副本
- 元数据文件的副本

### 数据关系图

```
DATA_DIR/
├── documents.sqlite   # 文档内容和元数据
├── vectors.sqlite     # 文档的嵌入向量
├── metadata.json      # 系统全局元数据
└── backups/           # 备份目录
    ├── backup_20250301_120000/    # 备份实例
    │   ├── documents.sqlite
    │   ├── vectors.sqlite
    │   └── metadata.json
    └── backup_20250307_093000/    # 另一个备份实例
        ├── documents.sqlite
        ├── vectors.sqlite
        └── metadata.json
```

## API端点

### 1. 文档管理

#### 1.1 上传文档
- **URL**: `/upload`
- **方法**: `POST`
- **描述**: 上传新文档到知识库
- **请求格式**:
  - 通过表单上传文件：
    ```
    Content-Type: multipart/form-data
    ```
    参数:
    - `file`: 要上传的文件
    - `library_id`: 文档库ID（可选，默认为"default"）
  
  - 通过JSON添加文本：
    ```
    Content-Type: application/json
    ```
    ```json
    {
      "text": "文档内容",
      "metadata": {
        "title": "文档标题",
        "author": "作者",
        "file_type": "文件类型"
      }
    }
    ```
- **响应**:
  ```json
  {
    "success": true,
    "message": "文档 '文件名' 已成功添加到知识库",
    "document_id": "生成的文档ID"
  }
  ```
  或错误响应:
  ```json
  {
    "success": false,
    "error": "错误信息"
  }
  ```

#### 1.2 获取文档列表
- **URL**: `/documents`
- **方法**: `GET`
- **描述**: 获取知识库中所有文档的列表
- **响应**:
  ```json
  {
    "success": true,
    "documents": [
      {
        "id": "文档ID",
        "title": "文档标题",
        "filename": "文件名",
        "file_type": "文件类型",
        "uploaded_at": "上传时间",
        "fragment_count": 分段数量,
        "library_id": "文档库ID"
      }
    ]
  }
  ```

#### 1.3 删除文档
- **URL**: `/documents/<doc_id>`
- **方法**: `DELETE`
- **描述**: 删除指定ID的文档及其所有分段
- **路径参数**:
  - `doc_id`: 要删除的文档ID
- **响应**:
  ```json
  {
    "success": true,
    "deleted": 删除的文档数量,
    "ids": ["已删除的文档ID列表"],
    "message": "成功删除了 X 个文档片段"
  }
  ```

### 2. 查询功能

#### 2.1 标准查询
- **URL**: `/query`
- **方法**: `POST`
- **描述**: 查询知识库并获取答案
- **请求**:
  ```json
  {
    "query": "查询问题",
    "library_id": "文档库ID"  // 可选，默认为"default"，使用"all"查询所有库
  }
  ```
- **响应**:
  ```json
  {
    "success": true,
    "answer": "基于知识库生成的回答",
    "sources": [
      {
        "id": "源文档ID",
        "content": "源文档内容摘录",
        "title": "源文档标题",
        "score": 匹配分数
      }
    ]
  }
  ```

#### 2.2 流式查询
- **URL**: `/query/stream`
- **方法**: `POST`
- **描述**: 流式查询知识库，以SSE格式逐步返回结果
- **请求**:
  ```json
  {
    "query": "查询问题",
    "library_id": "文档库ID"  // 可选，默认为"default"，使用"all"查询所有库
  }
  ```
- **响应**: Server-Sent Events (SSE) 流，包含以下事件:
  - 模型生成的内容片段:
    ```
    data: {"choices":[{"delta":{"content":"内容片段"}}]}
    ```
  - 参考来源:
    ```
    data: {"type":"sources","sources":[源文档信息]}
    ```
  - 完成标记:
    ```
    data: [DONE]
    ```

### 3. 文档库管理

#### 3.1 获取文档库列表
- **URL**: `/libraries`
- **方法**: `GET`
- **描述**: 获取所有文档库的列表
- **响应**:
  ```json
  {
    "success": true,
    "libraries": [
      {
        "id": "文档库ID",
        "name": "文档库名称",
        "document_count": 文档数量
      }
    ]
  }
  ```

#### 3.2 创建文档库
- **URL**: `/libraries`
- **方法**: `POST`
- **描述**: 创建新的文档库
- **请求**:
  ```json
  {
    "library_id": "文档库ID",
    "library_name": "文档库名称"
  }
  ```
- **响应**:
  ```json
  {
    "success": true,
    "message": "文档库 'library_id' 创建成功",
    "library": {
      "id": "文档库ID",
      "name": "文档库名称",
      "created_at": "创建时间",
      "document_count": 0
    }
  }
  ```

#### 3.3 删除文档库
- **URL**: `/libraries/<library_id>`
- **方法**: `DELETE`
- **描述**: 删除指定的文档库及其所有文档
- **路径参数**:
  - `library_id`: 要删除的文档库ID
- **响应**:
  ```json
  {
    "success": true,
    "message": "文档库 'library_id' 已删除，共删除 X 个文档"
  }
  ```
  注意: 不能删除默认文档库 ("default")

### 4. 系统管理

#### 4.1 创建备份
- **URL**: `/system/backup`
- **方法**: `POST`
- **描述**: 创建系统数据的备份
- **响应**:
  ```json
  {
    "success": true,
    "message": "成功创建备份 backup_id",
    "backup_id": "备份ID"
  }
  ```

#### 4.2 获取备份列表
- **URL**: `/system/backups`
- **方法**: `GET`
- **描述**: 获取所有可用备份的列表
- **响应**:
  ```json
  {
    "success": true,
    "backups": [
      {
        "id": "备份ID",
        "timestamp": "创建时间",
        "size": "备份大小"
      }
    ]
  }
  ```

#### 4.3 从备份恢复
- **URL**: `/system/restore/<backup_id>`
- **方法**: `POST`
- **描述**: 从指定的备份恢复系统
- **路径参数**:
  - `backup_id`: 要恢复的备份ID
- **响应**:
  ```json
  {
    "success": true,
    "message": "成功从备份恢复"
  }
  ```

### 5. 其他

#### 5.1 获取网站图标
- **URL**: `/favicon.ico`
- **方法**: `GET`
- **描述**: 获取网站图标

## 错误处理
所有API端点都会在发生错误时返回一个包含 `success: false` 和 `error` 字段的JSON响应，例如:
```json
{
  "success": false,
  "error": "错误详细信息"
}
```

## 特别说明

1. 文档上传流程包括:
   - 文件处理和文本提取
   - 文本分段
   - 为分段生成嵌入向量
   - 保存父文档和分段到持久化存储

2. 查询流程包括:
   - 生成查询的嵌入向量
   - 通过向量相似度检索相关文档
   - 使用检索到的文档生成RAG提示
   - 调用语言模型生成回答

3. 流式查询使用Server-Sent Events (SSE)协议，允许服务器逐步发送生成的回答。

4. 文档删除操作会同时删除父文档和所有相关分段，并从持久化存储中移除对应的嵌入向量。
