#!/usr/bin/env python
# -*- coding: utf-8 -*-

"""
嵌入器模块 - 处理文本和文档的嵌入向量生成
"""

import requests
import numpy as np
from typing import List, Dict, Any, Optional, Union
from haystack import component, Document

from .config import logger, EMBEDDER_API_KEY, EMBEDDER_API_URL, EMBEDDER_MODEL
from .storage import get_cache_key, add_to_cache, get_from_cache

class SiliconFlowBaseEmbedder:
    """硅基流动API嵌入器基类"""
    
    def __init__(self, api_key, api_url="https://api.siliconflow.cn/v1/embeddings", model="Pro/BAAI/bge-m3"):
        self.api_key = api_key
        self.api_url = api_url
        self.model = model
        self.batch_size = 16  # 批处理大小
        self.headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {self.api_key}"
        }
    
    def _get_embedding(self, text):
        """获取文本的嵌入向量"""
        if not text:
            return None
            
        # 检查缓存
        cache_key = get_cache_key(text)
        cached_embedding = get_from_cache(cache_key)
        if cached_embedding is not None:
            logger.info(f"从缓存获取嵌入向量，缓存键: {cache_key[:8]}...")
            return cached_embedding
            
        # 重试逻辑
        max_retries = 3
        retry_delay = 2  # 初始重试延迟（秒）
        
        for attempt in range(max_retries):
            try:
                # 发送请求到API
                response = requests.post(
                    self.api_url,
                    headers=self.headers,
                    json={
                        "model": self.model,
                        "input": [text]
                    },
                    timeout=30  # 设置30秒超时
                )
                
                # 检查响应状态
                if response.status_code == 200:
                    result = response.json()
                    if "data" in result and len(result["data"]) > 0:
                        embedding = result["data"][0]["embedding"]
                        
                        # 将嵌入向量添加到缓存
                        add_to_cache(cache_key, embedding)
                        
                        return embedding
                    else:
                        logger.error(f"API返回无效数据结构: {result}")
                else:
                    logger.error(f"API请求失败: {response.status_code} - {response.text}")
                
                # 如果这不是最后一次尝试，则等待后重试
                if attempt < max_retries - 1:
                    wait_time = retry_delay * (2 ** attempt)  # 指数退避
                    logger.info(f"将在 {wait_time} 秒后重试，尝试 {attempt + 1}/{max_retries}")
                    import time
                    time.sleep(wait_time)
                
            except requests.exceptions.SSLError as e:
                logger.error(f"SSL错误: {str(e)}")
                if "EOF occurred in violation of protocol" in str(e) and attempt < max_retries - 1:
                    # 特别处理SSL EOF错误，这可能是暂时性的连接问题
                    wait_time = retry_delay * (2 ** attempt)
                    logger.info(f"SSL连接错误，将在 {wait_time} 秒后重试，尝试 {attempt + 1}/{max_retries}")
                    import time
                    time.sleep(wait_time)
                else:
                    return None
            except Exception as e:
                logger.error(f"获取嵌入向量时出错: {str(e)}")
                if attempt < max_retries - 1:
                    wait_time = retry_delay * (2 ** attempt)
                    logger.info(f"将在 {wait_time} 秒后重试，尝试 {attempt + 1}/{max_retries}")
                    import time
                    time.sleep(wait_time)
                else:
                    return None
                    
        logger.error(f"在 {max_retries} 次尝试后仍未能获取嵌入向量")
        return None
    
    def get_batch_embeddings(self, texts):
        """批量获取文本的嵌入向量"""
        if not texts:
            return []
            
        # 过滤空文本
        valid_texts = [text for text in texts if text]
        if not valid_texts:
            return []
            
        try:
            response = requests.post(
                self.api_url,
                headers=self.headers,
                json={
                    "model": self.model,
                    "input": valid_texts
                }
            )
            
            if response.status_code == 200:
                data = response.json()
                if "data" in data and len(data["data"]) > 0:
                    embeddings = [item["embedding"] for item in data["data"]]
                    # 添加到缓存
                    for i, text in enumerate(valid_texts):
                        add_to_cache(get_cache_key(text), embeddings[i])
                    return embeddings
                else:
                    logger.error(f"API响应中没有嵌入向量: {data}")
                    return []
            else:
                logger.error(f"批量获取嵌入向量失败: {response.status_code} - {response.text}")
                return []
        except Exception as e:
            logger.error(f"批量获取嵌入向量时出错: {str(e)}")
            return []

@component
class SiliconFlowDocumentEmbedder:
    """为文档生成嵌入向量的组件"""
    
    def __init__(self, api_key, api_url="https://api.siliconflow.cn/v1/embeddings", model="Pro/BAAI/bge-m3"):
        self.embedder = SiliconFlowBaseEmbedder(api_key=api_key, api_url=api_url, model=model)
    
    @component.output_types(documents=List[Document])
    def run(self, documents: List[Document]):
        """为文档生成嵌入向量"""
        logger.info(f"SiliconFlowDocumentEmbedder开始处理 {len(documents)} 个文档")
        if not documents:
            logger.warning("没有提供文档")
            return {"documents": []}
            
        processed_docs = []
        
        # 按照批次处理文档
        batch_size = self.embedder.batch_size
        for i in range(0, len(documents), batch_size):
            batch = documents[i:i+batch_size]
            logger.info(f"处理文档批次 {i//batch_size + 1}/{(len(documents)-1)//batch_size + 1}, 包含 {len(batch)} 个文档")
            
            # 提取每个文档的内容
            batch_contents = [doc.content for doc in batch]
            
            try:
                # 批量获取嵌入向量
                batch_embeddings = self.embedder.get_batch_embeddings(batch_contents)
                
                if batch_embeddings and len(batch_embeddings) == len(batch):
                    # 将嵌入向量分配给对应的文档
                    for j, embedding in enumerate(batch_embeddings):
                        if embedding:
                            batch[j].embedding = embedding
                            processed_docs.append(batch[j])
                            logger.info(f"成功生成文档 {batch[j].id} 的嵌入向量，维度: {len(embedding)}")
                        else:
                            logger.error(f"无法为文档 {batch[j].id} 生成嵌入向量")
                else:
                    # 如果批量处理失败，退回到单个处理
                    logger.warning("批量嵌入生成失败，退回到逐个文档处理")
                    for doc in batch:
                        try:
                            embedding = self.embedder._get_embedding(doc.content)
                            if embedding:
                                logger.info(f"成功生成文档 {doc.id} 的嵌入向量，维度: {len(embedding)}")
                                doc.embedding = embedding
                                processed_docs.append(doc)
                            else:
                                logger.error(f"无法为文档 {doc.id} 生成嵌入向量")
                        except Exception as e:
                            logger.error(f"处理文档 {doc.id} 时出错: {str(e)}")
            except Exception as e:
                logger.error(f"批量处理文档时出错: {str(e)}")
                # 如果批量处理失败，退回到单个处理
                for doc in batch:
                    try:
                        embedding = self.embedder._get_embedding(doc.content)
                        if embedding:
                            logger.info(f"成功生成文档 {doc.id} 的嵌入向量，维度: {len(embedding)}")
                            doc.embedding = embedding
                            processed_docs.append(doc)
                        else:
                            logger.error(f"无法为文档 {doc.id} 生成嵌入向量")
                    except Exception as e:
                        logger.error(f"处理文档 {doc.id} 时出错: {str(e)}")
        
        logger.info(f"SiliconFlowDocumentEmbedder成功处理 {len(processed_docs)}/{len(documents)} 个文档")
        return {"documents": processed_docs}

@component
class SiliconFlowTextEmbedder:
    """为文本生成嵌入向量的组件"""
    
    def __init__(self, api_key, api_url="https://api.siliconflow.cn/v1/embeddings", model="Pro/BAAI/bge-m3"):
        self.embedder = SiliconFlowBaseEmbedder(api_key=api_key, api_url=api_url, model=model)
    
    @component.output_types(embedding=List[float])
    def run(self, text: str):
        """为文本生成嵌入向量"""
        # 严格验证输入类型
        if not text or not isinstance(text, str):
            logger.warning(f"收到无效的文本输入: {type(text)}")
            # 返回空向量数组而不是None，避免下游组件错误
            return {"embedding": [0.0] * 1024}
            
        # 尝试获取嵌入向量
        embedding = self.embedder._get_embedding(text)
        
        # 严格验证嵌入向量是否为列表类型且不为空
        if embedding is None or not isinstance(embedding, list) or len(embedding) == 0:
            logger.warning(f"无法为文本生成有效的嵌入向量，返回默认向量")
            # 返回全0向量而不是空列表，保持一致的维度
            return {"embedding": [0.0] * 1024}
            
        # 正常情况直接返回
        return {"embedding": embedding} 