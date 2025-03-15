# DeepSeek FastRAG 知识库查询系统

基于DeepSeek V3大模型和FastRAG框架的高效知识库查询系统。

## 功能特点

- 使用DeepSeek V3大模型进行自然语言理解和生成
- 基于FastRAG框架实现高效检索增强生成
- 支持文档上传、管理和检索
- 提供简洁美观的Web界面
- 支持中文文档处理和查询

## 系统要求

- Python 3.8+
- 有效的DeepSeek API密钥
- 互联网连接（用于API调用）

## 快速开始

### 1. 安装依赖

```bash
pip install -r requirements.txt
```

### 2. 配置环境变量

创建`.env`文件并填入以下内容：

```
DEEPSEEK_API_KEY=你的DeepSeek_API密钥
DEEPSEEK_API_URL=https://api.siliconflow.cn/v1/chat/completions
DEEPSEEK_MODEL_NAME=Pro/deepseek-ai/DeepSeek-V3
EMBEDDING_MODEL=Pro/BAAI/bge-m3
EMBEDDING_API_URL=https://api.siliconflow.cn/v1/embeddings
```

### 3. 启动系统

使用以下命令启动系统：

```bash
python run.py
```

系统将自动检查环境、安装依赖并启动Web服务。

### 4. 访问Web界面

打开浏览器，访问：

```
http://localhost:5000
```

## 使用指南

### 上传文档

1. 在Web界面中，点击"上传"部分的"选择文件"按钮
2. 选择要上传的文本文件（支持.txt、.md、.pdf格式）
3. 点击"上传"按钮
4. 等待系统处理和索引文档

### 查询知识库

1. 在查询输入框中输入您的问题
2. 点击"查询"按钮
3. 系统将返回基于知识库内容的回答，以及相关的参考来源

### 管理文档

- 在"文档列表"部分可以查看所有已上传的文档
- 点击"刷新列表"按钮可以更新文档列表
- 点击文档旁边的"删除"按钮可以移除不需要的文档

## 故障排除

### 导入错误

如果遇到模块导入错误，请尝试：

```bash
python startup.py
```

此脚本会自动检查并修复常见的导入问题。

### API连接问题

如果遇到API连接问题：

1. 检查您的API密钥是否正确
2. 确认网络连接是否正常
3. 验证API URL是否正确

### Haystack版本兼容性

本系统使用farm-haystack 1.20.0版本，如果遇到Haystack相关错误，请确保安装了正确的版本：

```bash
pip uninstall -y farm-haystack
pip install farm-haystack==1.20.0
```

### FastRAG安装问题

如果FastRAG安装失败，可以尝试：

```bash
pip install --upgrade pip
pip install git+https://github.com/IntelLabs/fastRAG.git
```

## 技术架构

- **前端**：HTML/CSS/JavaScript
- **后端**：Python Flask
- **检索框架**：FastRAG + Haystack
- **大模型**：DeepSeek V3
- **嵌入模型**：BGE-M3

## 参考资源

- [FastRAG GitHub仓库](https://github.com/IntelLabs/fastRAG)
- [DeepSeek API文档](https://platform.deepseek.com/)
- [Haystack文档](https://docs.haystack.deepset.ai/)

# RAG 知识库查询系统优化说明

## 系统优化

基于Cherry studio的知识库实现分析，我们对当前RAG系统进行了以下优化：

### 1. 文本分割器优化
- 调整了 `chunk_size` 从256到512，找到更合适的文档分块大小
- 优化了 `chunk_overlap` 从50到128，确保语义连贯性
- 扩展了 `separators` 列表，添加了更多中文分隔符（如"；"和"："）

### 2. 嵌入生成优化
- 实现了批量嵌入处理，减少API调用次数
- 添加了嵌入向量缓存机制，避免重复计算相同文本的嵌入
- 实现了简单的LRU缓存策略，控制缓存大小

### 3. 检索优化
- 调整了 `InMemoryEmbeddingRetriever` 的 `top_k` 参数为12
- 设置了 `similarity_threshold` 为0.2，过滤掉相关性较低的文档片段
- 在处理检索结果时增加了额外的相似度过滤

### 4. 上下文增强
- 增加了相邻片段的智能连接，确保语义连贯性
- 实现了文档片段排序和组织，按照文档结构组织上下文
- 优化了上下文格式，更清晰地标识不同文档来源

### 5. 提示工程优化
- 优化了提示模板，使其更符合中文表达习惯
- 增加了对定义和概念的优先处理指导
- 增强了回答格式化的说明，提高回答质量

## 性能提升
- **响应速度**: 通过批量处理和缓存机制，减少了API调用次数
- **检索准确性**: 通过优化分块策略和相邻片段处理，提高了检索结果的相关性
- **回答质量**: 通过提示工程的优化，使回答更加准确和自然

## 参考自Cherry studio的实践
- 批量处理文档嵌入
- 智能化的任务队列管理
- 高效的缓存策略
- 文档处理的异步机制

这些优化使系统在查询效果和性能方面都得到了显著提升。

# DeepSeek知识库查询系统

这是一个基于DeepSeek API的知识库查询系统，支持上传文档并使用大语言模型对文档内容进行智能查询。

## 主要功能

- 支持多种文件类型的上传和处理
- 支持中文文档的语义搜索
- 提供流式响应的查询接口
- 支持文档管理和系统备份
- 支持通过Go代理优化API请求性能

## 系统要求

- Python 3.8 或更高版本
- DeepSeek API 密钥（用于嵌入和LLM）
- 可选：Go 1.16 或更高版本（如果需要使用Go代理功能）

## 安装和配置

1. 克隆或下载项目代码
2. 安装依赖：`pip install -r requirements.txt`
3. 创建 `.env` 文件并设置以下环境变量：
   ```
   EMBEDDER_API_KEY=your_deepseek_api_key
   LLM_API_KEY=your_deepseek_api_key
   ```

## 使用Go代理优化API请求

为了降低API请求的首个token延迟，本系统提供了通过Go代理发送API请求的功能。使用Go代理可以显著减少首个token的响应时间。

### 方法一：启动Go代理服务

1. 安装Go运行环境（如果尚未安装）
2. 进入 `deepseek-proxy` 目录
3. 运行 `go mod tidy` 安装依赖
4. 运行 `go run main.go` 启动代理服务器

### 方法二：使用Docker运行Go代理服务

如果您的系统没有安装Go，可以使用Docker运行代理服务：

1. 进入 `deepseek-proxy` 目录
2. 构建Docker镜像：`docker build -t deepseek-proxy .`
3. 运行Docker容器：`docker run -p 8000:8000 deepseek-proxy`

### 启用Go代理功能

启用Go代理功能有两种方式：

1. 使用命令行参数：
   ```
   python app.py --use-go-proxy
   ```

2. 设置环境变量（在`.env`文件中添加）：
   ```
   USE_GO_PROXY=true
   GO_PROXY_URL=http://localhost:8000/proxy/stream
   ```

您还可以自定义Go代理的URL：
```
python app.py --use-go-proxy --go-proxy-url http://localhost:8000/proxy/stream
```

### 验证Go代理状态

系统提供了状态检查API，您可以通过以下方式验证Go代理是否正常工作：

```
curl http://localhost:5000/system/status
```

返回的JSON数据中将包含Go代理的启用状态和连接状态。

## 性能测试界面

系统还提供了一个用于测试API性能的页面，您可以在浏览器中打开此页面来比较直接调用API和通过Go代理调用API的性能差异：

```
http://localhost:8000/test_interface.html
```

该界面可以清晰地展示首个token延迟和总响应时间的对比。

# 系统启动优化

为了解决系统启动缓慢和嵌入向量生成时间长的问题，我们添加了以下性能优化功能：

## 快速启动模式

系统现在支持快速启动模式，通过以下命令使用：

```bash
python app.py --fast
```

快速启动模式的工作原理：

1. **跳过嵌入向量加载**：启动时不加载文档的嵌入向量，大幅减少启动时间
2. **懒加载嵌入向量**：仅在需要使用文档时才生成其嵌入向量
3. **嵌入向量缓存**：自动记录哪些文档已有有效的嵌入向量，避免重复生成

## 其它启动参数

系统还支持以下启动参数：

```bash
python app.py --help
```

常用参数：
- `--fast`：启用快速启动模式，跳过嵌入向量预加载
- `--debug`：启用调试模式，自动重载代码更改
- `--host`：指定主机地址，默认为0.0.0.0
- `--port`：指定端口号，默认为5000

## 使用场景建议

- **开发测试环境**：使用 `python app.py --fast --debug` 加快启动速度并启用调试
- **生产环境首次启动**：使用 `python app.py --fast` 快速启动系统
- **生产环境稳定运行**：在系统使用一段时间后（大多数文档已有嵌入向量），可使用 `python app.py` 常规启动

## 注意事项

1. 快速启动模式下，首次查询可能会比后续查询慢，因为系统需要为相关文档生成嵌入向量
2. 系统会自动在文档元数据中记录嵌入向量的有效性，避免重复生成
3. 即使在API连接不稳定的情况下，系统也能继续运行，并在API服务恢复后自动使用完整功能

## 嵌入向量生成优化

系统优化了嵌入向量的生成和缓存策略：

1. **批量处理**：对多个文档的嵌入向量生成进行批量处理，减少API调用次数
2. **智能缓存**：使用基于内容哈希的缓存机制避免重复生成相同内容的嵌入向量
3. **有效性标记**：自动标记和记录哪些文档已有有效的嵌入向量
4. **错误恢复**：增强了连接错误和API限制的处理能力，实现指数退避重试

这些优化显著减少了系统启动时间和资源消耗，提高了系统的响应速度和稳定性。 