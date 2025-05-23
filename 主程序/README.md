# 硅基流动 RAG增强流式请求

这是一个集成了LangChain和RAG（检索增强生成）功能的流式请求模块，为大语言模型赋予知识库检索能力。

## 功能特性

- 支持多种文档格式（TXT, PDF, DOCX, MD）上传到知识库
- 基于LangChain实现RAG检索增强
- 支持流式输出，实时显示生成结果
- MathJax支持，能够正确渲染LaTeX公式
- 支持多种嵌入模型选择
- 灵活的参数配置（分块大小、重叠度、相似度阈值等）
- 独立模块设计，不影响主系统运行

## 安装

1. 克隆代码库:
```bash
git clone <repository_url>
cd <repository_directory>
```

2. 安装依赖:
```bash
pip install -r requirements.txt
```

3. 配置环境变量:
```
OPENAI_API_KEY=your_openai_api_key
```

## 运行

启动应用:
```bash
python app.py
```

访问 http://localhost:5000/langchain_rag 开始使用RAG增强的流式请求。

## 使用方法

1. **上传文档**:
   - 点击"文档管理"选项卡
   - 选择并上传知识库文档

2. **配置参数**:
   - 点击"设置"选项卡
   - 设置分块大小、嵌入模型等参数

3. **对话交互**:
   - 在"对话"选项卡输入提问
   - 勾选"启用RAG增强"以使用知识库检索

## 项目结构

- `app.py`: 主应用入口
- `langchain_rag.py`: RAG功能核心实现
- `routes.py`: 路由定义
- `templates/`: 模板文件
  - `langchain_rag_stream.html`: RAG前端界面
- `data/`: 数据目录
  - `rag_documents/`: 上传的文档存储目录
  - `vector_db/`: 向量数据库存储目录

## 依赖

- Flask: Web框架
- LangChain: RAG实现
- OpenAI API: 大语言模型和嵌入模型
- FAISS/Chroma: 向量存储
- MathJax: LaTeX渲染

## 注意事项

- 本模块设计为独立功能，不会影响主项目运行
- 大文件处理可能需要较长时间
- 确保OpenAI API密钥配置正确 