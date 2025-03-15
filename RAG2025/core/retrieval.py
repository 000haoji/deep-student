#!/usr/bin/env python
# -*- coding: utf-8 -*-

"""
检索模块 - 处理文档检索和查询相关功能
"""

import requests
import json
from typing import List, Dict, Any, Optional
from haystack import component, Document, Pipeline
from haystack.document_stores.in_memory import InMemoryDocumentStore
from haystack.components.retrievers.in_memory import InMemoryEmbeddingRetriever

from .config import logger, LLM_API_KEY, LLM_API_URL, LLM_MODEL, USE_GO_PROXY, GO_PROXY_URL

class SiliconFlowLLM:
    """硅基流动LLM接口"""
    
    def __init__(self, api_key, api_url, model):
        self.api_key = api_key
        self.api_url = api_url
        self.model = model
        self.headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {self.api_key}"
        }
    
    def generate(self, prompt, system_prompt=None, stream=False):
        """生成回答"""
        messages = []
        
        # 添加系统提示
        if system_prompt:
            messages.append({"role": "system", "content": system_prompt})
        
        # 添加用户提示
        messages.append({"role": "user", "content": prompt})
        
        try:
            # 构建请求
            payload = {
                "model": self.model,
                "messages": messages,
                "stream": stream,
                "max_tokens": 1000
            }
            
            # 发送请求
            response = requests.post(
                self.api_url,
                headers=self.headers,
                json=payload,
                stream=stream
            )
            
            if stream:
                return response
            else:
                if response.status_code == 200:
                    result = response.json()
                    if "choices" in result and len(result["choices"]) > 0:
                        return result["choices"][0]["message"]["content"]
                    else:
                        logger.error(f"API响应中没有回答: {result}")
                        return None
                else:
                    logger.error(f"生成回答失败: {response.status_code} - {response.text}")
                    return None
        except Exception as e:
            logger.error(f"生成回答时出错: {str(e)}")
            return None

def setup_retrieval_components(documents, embedder, lazy_embedding=False):
    """设置检索组件
    
    Args:
        documents: 文档列表
        embedder: 文本嵌入器
        lazy_embedding: 是否启用懒加载模式，启用后只为缺失向量的文档生成嵌入向量
    """
    # 创建文档存储
    document_store = InMemoryDocumentStore()
    
    # 为文档添加嵌入向量
    if documents:
        # 使用 embedder 为文档添加嵌入向量
        documents_with_embeddings = []
        
        # 首先为一个文档生成嵌入向量，确定标准大小
        sample_doc = None
        standard_size = 1024  # 默认大小
        
        # 找出已有有效嵌入向量的文档作为样本
        for doc in documents:
            if hasattr(doc, "embedding") and doc.embedding is not None and doc.meta.get("embedding_valid", False):
                sample_doc = doc
                standard_size = len(doc.embedding)
                logger.info(f"使用已有文档的嵌入向量标准大小: {standard_size}")
                break
        
        # 如果没有找到有效的样本，并且不是懒加载模式，则生成一个
        if sample_doc is None and not lazy_embedding and documents and len(documents) > 0:
            sample_doc = documents[0]
            try:
                # 添加错误处理，检查嵌入向量是否为None
                sample_embedding = embedder.run(text=sample_doc.content)["embedding"]
                if sample_embedding is None:
                    logger.error("无法获取嵌入向量，API可能无法连接")
                    # 使用默认嵌入向量大小，或创建一个空的文档存储和检索器
                    retriever = EnhancedRetriever(document_store=document_store, top_k=5)
                    return document_store, retriever
                    
                standard_size = len(sample_embedding)
                sample_doc.embedding = sample_embedding
                sample_doc.meta["embedding_valid"] = True
                logger.info(f"标准嵌入向量大小: {standard_size}")
            except Exception as e:
                logger.error(f"获取嵌入向量时出错: {str(e)}")
                # 创建一个空的文档存储和检索器
                retriever = EnhancedRetriever(document_store=document_store, top_k=5)
                return document_store, retriever
        
        # 处理所有文档
        for doc in documents:
            try:
                # 如果文档已有嵌入向量且标记为有效，直接使用
                if hasattr(doc, "embedding") and doc.embedding is not None and doc.meta.get("embedding_valid", False):
                    if len(doc.embedding) == standard_size:
                        documents_with_embeddings.append(doc)
                        logger.info(f"文档 {doc.id} 已有有效嵌入向量，跳过生成")
                        continue
                    else:
                        logger.warning(f"文档 {doc.id} 的嵌入向量大小 ({len(doc.embedding)}) 与标准大小 ({standard_size}) 不一致，将重新生成")
                
                # 如果是懒加载模式且不是样本文档，跳过生成嵌入向量
                if lazy_embedding and doc != sample_doc:
                    logger.debug(f"懒加载模式：跳过为文档 {doc.id} 生成嵌入向量")
                    continue
                
                # 重新生成嵌入向量
                embedding_result = embedder.run(text=doc.content)
                embedding = embedding_result["embedding"]
                
                # 检查嵌入向量是否为None
                if embedding is None:
                    logger.warning(f"文档 {doc.id} 无法获取嵌入向量，跳过")
                    continue
                    
                # 检查嵌入向量大小
                if len(embedding) != standard_size:
                    logger.error(f"文档 {doc.id} 的嵌入向量大小 ({len(embedding)}) 与标准大小 ({standard_size}) 不一致")
                    continue
                
                doc.embedding = embedding
                doc.meta["embedding_valid"] = True
                documents_with_embeddings.append(doc)
                logger.info(f"成功为文档 {doc.id} 生成嵌入向量，大小: {len(embedding)}")
            except Exception as e:
                logger.error(f"处理文档 {doc.id} 时出错: {str(e)}")
        
        # 使用已处理的文档创建文档存储
        if documents_with_embeddings:
            document_store.write_documents(documents_with_embeddings)
            logger.info(f"将 {len(documents_with_embeddings)} 个文档添加到文档存储")
        else:
            logger.warning("没有可用的带嵌入向量的文档")
    
    # 创建检索器
    retriever = EnhancedRetriever(
        document_store=document_store,
        top_k=5
    )
    
    return document_store, retriever

def generate_rag_prompt(query, documents):
    """生成RAG提示"""
    context_parts = []
    
    for i, doc in enumerate(documents):
        # 提取文档内容和元数据
        content = doc.content
        meta = doc.meta or {}
        
        # 构建上下文部分
        source_info = f"来源: {meta.get('source', '未知')}" if 'source' in meta else ""
        filename = f"文件名: {meta.get('filename', '未知')}" if 'filename' in meta else ""
        doc_type = f"类型: {meta.get('type', '未知')}" if 'type' in meta else ""
        
        # 组合元数据信息
        meta_info = ", ".join(filter(None, [source_info, filename, doc_type]))
        
        # 添加到上下文
        context_parts.append(f"[文档 {i+1}] {meta_info}\n{content}")
    
    # 组合完整上下文
    context = "\n\n".join(context_parts)
    
    # 构建提示
    prompt = f"""请基于以下参考文档回答用户的问题。如果无法从参考文档中找到答案，请明确说明。

参考文档:
{context}

用户问题: {query}

请使用Markdown格式提供详细、准确的回答，严格遵循以下格式要求：
1. 使用标题格式：
   - 主标题使用 # 开头
   - 次级标题使用 ## 和 ### 开头
   - 为文档增加清晰的层次结构

2. 使用列表格式：
   - 无序列表使用 - 开头
   - 有序列表使用 1. 2. 3. 开头
   - 嵌套列表注意缩进

3. 使用强调格式：
   - 重要内容使用 **加粗文本** 
   - 次要强调使用 *斜体文本*

4. 引用格式：
   - 引用参考文档内容时使用 > 开头的引用格式
   - 对于重要引用，使用 > **引用内容** 格式

5. 代码格式：
   - 使用 ```语言 和 ``` 包裹代码块
   - 行内代码使用 `代码` 格式

6. 表格格式（如需要）：
   - 使用 | 分隔列
   - 使用 --- 分隔表头和内容

回答时必须引用相关的参考文档，保持信息的准确性，并使用Markdown格式使回答结构清晰、易于阅读。"""
    
    return prompt

def generate_sources_info(documents):
    """生成来源信息"""
    sources = []
    
    for doc in documents:
        meta = doc.meta or {}
        
        # 提取元数据
        source = {
            "id": doc.id,
            "content": doc.content[:200] + "..." if len(doc.content) > 200 else doc.content,
            "title": meta.get("filename", "未知文档")
        }
        
        # 添加其他可能的元数据
        if "source" in meta:
            source["source"] = meta["source"]
        if "type" in meta:
            source["type"] = meta["type"]
        
        sources.append(source)
    
    return sources

def extract_json_from_text(text):
    """从文本中提取JSON内容
    
    如果文本中包含JSON格式的内容（通常在```json和```之间），
    这个函数会尝试提取并解析它
    """
    import json
    import re
    
    # 尝试寻找JSON代码块
    json_pattern = r"```(?:json)?\s*([\s\S]*?)```"
    matches = re.findall(json_pattern, text)
    
    if matches:
        for match in matches:
            try:
                # 尝试解析JSON
                result = json.loads(match.strip())
                return result
            except json.JSONDecodeError:
                continue
    
    # 如果没有找到有效的JSON代码块，尝试直接解析整个文本
    try:
        return json.loads(text.strip())
    except json.JSONDecodeError:
        return None

def get_llm_system_prompt():
    """获取LLM系统提示
    
    返回用于LLM的系统提示，指导模型以正确的格式和风格回答问题
    """
    return """你是一个专业的知识库助手，可以根据提供的参考文档回答用户的问题。
    
请遵循以下指导原则：

1. 基于参考文档提供准确的回答，不要编造信息
2. 如果参考文档中没有足够信息回答问题，请诚实地说明
3. 使用Markdown格式组织回答，确保层次清晰
4. 回答问题时，引用相关参考文档的内容作为支持
5. 使用中文回答问题，除非特别要求使用其他语言
6. 确保回答专业、客观、有帮助

在适当的情况下，使用以下Markdown元素提高回答的可读性：
- 使用标题和子标题组织内容
- 使用列表条目呈现步骤或项目
- 使用代码块展示代码或结构化内容
- 使用引用块引用重要内容
- 使用表格对比或组织信息

如果用户问题不明确或需要更多信息，请礼貌地询问更多细节。"""

# 增强型检索器，支持文档库过滤
class EnhancedRetriever(InMemoryEmbeddingRetriever):
    """支持过滤条件的检索器"""
    
    @component.output_types(documents=List[Document])
    def run(self, query_embedding: List[float], filters: Optional[Dict[str, Any]] = None):
        """
        检索与查询向量最相关的文档
        
        Args:
            query_embedding: 查询文本的嵌入向量
            filters: 过滤条件，如{"library_id": "lib1"}
            
        Returns:
            包含检索到的文档的字典
        """
        # 验证嵌入向量的有效性
        if not query_embedding or not isinstance(query_embedding, list):
            logger.error(f"收到无效的查询嵌入向量: {type(query_embedding)}")
            return {"documents": []}
            
        # 验证嵌入向量的长度
        if len(query_embedding) == 0:
            logger.error("查询嵌入向量长度为0")
            return {"documents": []}
            
        # 记录原始过滤条件
        logger.info(f"原始过滤条件: {filters}")
        
        # 转换过滤条件为Haystack 2.0格式
        haystack_filters = None
        if filters is not None and "library_id" in filters:
            library_id = filters.pop("library_id")
            
            # 确保library_id不为None或空字符串
            if library_id and library_id != "all":
                logger.info(f"为库 {library_id} 创建Haystack 2.0格式的过滤条件")
                # 使用Haystack 2.0的过滤语法
                haystack_filters = {
                    "operator": "==",
                    "field": "meta.library_id",
                    "value": library_id
                }
                logger.info(f"Haystack 2.0格式的过滤条件: {haystack_filters}")
        
        # 使用父类方法检索文档，传递Haystack 2.0格式的过滤条件
        try:
            result = super().run(query_embedding=query_embedding, filters=haystack_filters)
            
            # 验证结果格式
            if not isinstance(result, dict) or "documents" not in result:
                logger.error(f"检索结果格式不正确: {result}")
                return {"documents": []}
                
            # 记录检索结果数量
            doc_count = len(result["documents"])
            logger.info(f"检索到 {doc_count} 个文档")
            
            # 记录每个文档的库ID，帮助调试
            if doc_count > 0:
                logger.info("检索到的文档库ID:")
                for i, doc in enumerate(result["documents"]):
                    doc_lib_id = doc.meta.get("library_id", "unknown")
                    logger.info(f"文档 {i+1}/{doc_count}: ID={doc.id}, 库ID={doc_lib_id}")
            
            return result
        except Exception as e:
            logger.error(f"检索文档时出错: {str(e)}")
            # 返回空结果
            return {"documents": []}

def inspect_document_metadata(documents):
    """
    检查文档的元数据结构，特别是library_id字段
    
    Args:
        documents: 要检查的文档列表
        
    Returns:
        统计信息字符串
    """
    if not documents:
        return "没有文档可供检查"
    
    total_docs = len(documents)
    docs_with_library_id = 0
    library_id_counts = {}
    
    for doc in documents:
        lib_id = None
        
        # 检查meta字段是否存在
        if not hasattr(doc, "meta") or doc.meta is None:
            logger.warning(f"文档 {doc.id} 没有meta字段")
            continue
            
        # 检查library_id在meta中的位置
        if "library_id" in doc.meta:
            lib_id = doc.meta["library_id"]
            docs_with_library_id += 1
        elif isinstance(doc.meta.get("meta"), dict) and "library_id" in doc.meta["meta"]:
            # 如果library_id嵌套在meta.meta中，这是一种错误的结构
            lib_id = doc.meta["meta"]["library_id"]
            logger.warning(f"文档 {doc.id} 的library_id嵌套在meta.meta中，这可能导致过滤问题")
            # 修复结构
            doc.meta["library_id"] = lib_id
            if "meta" in doc.meta:
                del doc.meta["meta"]
        
        # 统计不同的library_id
        if lib_id:
            if lib_id not in library_id_counts:
                library_id_counts[lib_id] = 0
            library_id_counts[lib_id] += 1
    
    # 生成统计信息
    stats = f"文档总数: {total_docs}\n"
    stats += f"具有library_id的文档数: {docs_with_library_id} ({docs_with_library_id/total_docs*100:.1f}%)\n"
    stats += "各library_id的文档数量:\n"
    
    for lib_id, count in library_id_counts.items():
        stats += f"  - {lib_id}: {count} 个文档\n"
    
    logger.info(stats)
    return stats 