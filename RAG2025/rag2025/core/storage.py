#!/usr/bin/env python
# -*- coding: utf-8 -*-

"""
存储模块 - 处理文档和向量的持久化存储
"""

import os
import json
import pickle
import sqlitedict
import shutil
import time
from datetime import datetime
from typing import List, Dict, Any, Optional
from haystack import Document

from .config import logger, DATA_DIR, VECTORS_PATH, DOCUMENTS_PATH, METADATA_PATH, BACKUP_DIR

# 持久化存储和加载函数
def save_documents(documents):
    """将文档保存到SQLite数据库"""
    try:
        with sqlitedict.SqliteDict(DOCUMENTS_PATH, autocommit=False) as doc_db:
            for doc in documents:
                # 使用文档ID作为键
                doc_id = doc.id
                # 将Document对象序列化
                doc_data = {
                    "id": doc.id,
                    "content": doc.content,
                    "meta": doc.meta
                }
                doc_db[doc_id] = doc_data
            doc_db.commit()
        logger.info(f"成功保存 {len(documents)} 个文档到 {DOCUMENTS_PATH}")
        return True
    except Exception as e:
        logger.error(f"保存文档时出错: {str(e)}")
        return False

def save_embeddings(documents):
    """保存文档的嵌入向量到持久化存储"""
    try:
        with sqlitedict.SqliteDict(VECTORS_PATH) as vec_db:
            for doc in documents:
                if hasattr(doc, "embedding") and doc.embedding is not None:
                    # 保存嵌入向量
                    vec_db[doc.id] = doc.embedding
                    # 更新文档元数据，标记为有效的嵌入向量
                    doc.meta["embedding_valid"] = True
            vec_db.commit()
            
        # 更新文档元数据
        with sqlitedict.SqliteDict(DOCUMENTS_PATH) as doc_db:
            for doc in documents:
                if doc.id in doc_db:
                    doc_data = doc_db[doc.id]
                    doc_data["meta"] = doc.meta
                    doc_db[doc.id] = doc_data
            doc_db.commit()
            
        logger.info(f"成功保存 {len(documents)} 个文档的嵌入向量到 {VECTORS_PATH}")
        return True
    except Exception as e:
        logger.error(f"保存嵌入向量时出错: {str(e)}")
        return False

def load_documents_and_embeddings(skip_embeddings=False):
    """从持久化存储加载文档和嵌入向量
    
    Args:
        skip_embeddings: 是否跳过加载嵌入向量，用于快速启动
    """
    documents = []
    
    try:
        # 加载文档内容
        with sqlitedict.SqliteDict(DOCUMENTS_PATH) as doc_db:
            doc_data_list = list(doc_db.values())
        
        logger.info(f"从持久化存储加载了 {len(doc_data_list)} 个文档数据")
        
        # 创建Document对象
        documents = []
        doc_ids_set = set()  # 用于跟踪已处理的文档ID
        
        for doc_data in doc_data_list:
            doc_id = doc_data["id"]
            
            # 如果已经处理过这个ID的文档，跳过
            if doc_id in doc_ids_set:
                logger.warning(f"跳过重复的文档ID: {doc_id}")
                continue
                
            # 记录已处理的文档ID
            doc_ids_set.add(doc_id)
            
            doc = Document(
                id=doc_id,
                content=doc_data["content"],
                meta=doc_data.get("meta", {})
            )
            # 如果元数据中没有embedding_valid字段，添加并设为False
            if "embedding_valid" not in doc.meta:
                doc.meta["embedding_valid"] = False
            
            # 如果元数据中没有library_id字段，添加默认值
            if "library_id" not in doc.meta:
                doc.meta["library_id"] = "default"
                
            documents.append(doc)
        
        # 如果不跳过嵌入向量，则加载它们
        if not skip_embeddings:
            with sqlitedict.SqliteDict(VECTORS_PATH) as vec_db:
                for doc in documents:
                    if doc.id in vec_db:
                        doc.embedding = vec_db[doc.id]
                        # 标记该文档已有有效的嵌入向量
                        doc.meta["embedding_valid"] = True
        else:
            logger.info("跳过加载嵌入向量，系统将在需要时生成它们")
        
        # 统计父文档和分段文档数量
        parent_docs = [doc for doc in documents if not doc.meta.get("is_fragment", False)]
        fragment_docs = [doc for doc in documents if doc.meta.get("is_fragment", False)]
        logger.info(f"从持久化存储加载了 {len(documents)} 个文档，其中父文档 {len(parent_docs)} 个，分段文档 {len(fragment_docs)} 个")
        
        return documents
    except Exception as e:
        logger.error(f"加载文档和嵌入向量时出错: {str(e)}")
        return []

def save_metadata(data):
    """保存元数据到JSON文件"""
    try:
        with open(METADATA_PATH, 'w', encoding='utf-8') as f:
            json.dump(data, f, ensure_ascii=False, indent=2)
        return True
    except Exception as e:
        logger.error(f"保存元数据时出错: {str(e)}")
        return False

def load_metadata():
    """从JSON文件加载元数据"""
    if not os.path.exists(METADATA_PATH):
        return {}
    try:
        with open(METADATA_PATH, 'r', encoding='utf-8') as f:
            return json.load(f)
    except Exception as e:
        logger.error(f"加载元数据时出错: {str(e)}")
        return {}

def delete_document(doc_id):
    """从持久化存储中删除文档及其向量
    
    Args:
        doc_id: 要删除的文档ID或基础ID
    
    Returns:
        bool: 操作是否成功
    """
    try:
        # 提取基础文档ID（移除_chunk_部分）
        base_doc_id = doc_id.split("_chunk_")[0] if "_chunk_" in doc_id else doc_id
        logger.info(f"从持久化存储中删除文档和向量，基础ID: {base_doc_id}")
        
        deleted_docs = set()
        
        # 从文档存储中删除
        with sqlitedict.SqliteDict(DOCUMENTS_PATH, autocommit=False) as doc_db:
            # 获取所有文档ID，找出待删除的ID
            doc_ids = list(doc_db.keys())
            for current_id in doc_ids:
                # 判断是否为目标文档或其分块
                current_base_id = current_id.split("_chunk_")[0] if "_chunk_" in current_id else current_id
                if current_base_id == base_doc_id:
                    del doc_db[current_id]
                    deleted_docs.add(current_id)
                    logger.info(f"从文档存储中删除了文档: {current_id}")
            doc_db.commit()
        
        # 从向量存储中删除
        with sqlitedict.SqliteDict(VECTORS_PATH, autocommit=False) as vec_db:
            for doc_id in deleted_docs:
                if doc_id in vec_db:
                    del vec_db[doc_id]
                    logger.info(f"从向量存储中删除了文档向量: {doc_id}")
            vec_db.commit()
        
        logger.info(f"成功从持久化存储中删除了 {len(deleted_docs)} 个文档及其向量")
        return True
    except Exception as e:
        logger.error(f"从持久化存储中删除文档时出错: {str(e)}")
        return False

# 缓存相关函数
_embedding_cache = {}

def get_cache_key(text):
    """生成缓存键"""
    return hash(text)

def add_to_cache(key, embedding):
    """添加到缓存"""
    _embedding_cache[key] = embedding
    # 简单的缓存大小控制
    if len(_embedding_cache) > 1000:
        # 删除最早添加的项
        oldest_key = next(iter(_embedding_cache))
        del _embedding_cache[oldest_key]

def get_from_cache(key):
    """从缓存获取"""
    return _embedding_cache.get(key)

# 备份相关函数
def backup_data_store():
    """创建数据存储的备份"""
    try:
        # 创建备份目录
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        backup_path = os.path.join(BACKUP_DIR, f"backup_{timestamp}")
        os.makedirs(backup_path, exist_ok=True)
        
        # 备份向量存储
        if os.path.exists(VECTORS_PATH):
            shutil.copy2(VECTORS_PATH, os.path.join(backup_path, os.path.basename(VECTORS_PATH)))
            logger.info(f"已备份 {VECTORS_PATH} 到 {os.path.join(backup_path, os.path.basename(VECTORS_PATH))}")
        
        # 备份文档存储
        if os.path.exists(DOCUMENTS_PATH):
            shutil.copy2(DOCUMENTS_PATH, os.path.join(backup_path, os.path.basename(DOCUMENTS_PATH)))
            logger.info(f"已备份 {DOCUMENTS_PATH} 到 {os.path.join(backup_path, os.path.basename(DOCUMENTS_PATH))}")
        
        # 备份元数据
        if os.path.exists(METADATA_PATH):
            shutil.copy2(METADATA_PATH, os.path.join(backup_path, os.path.basename(METADATA_PATH)))
            logger.info(f"已备份 {METADATA_PATH} 到 {os.path.join(backup_path, os.path.basename(METADATA_PATH))}")
        
        return {"success": True, "backup_id": timestamp, "path": backup_path}
    except Exception as e:
        logger.error(f"创建备份时出错: {str(e)}")
        return {"success": False, "error": str(e)}

def list_backups():
    """列出所有可用的备份"""
    try:
        backups = []
        for item in os.listdir(BACKUP_DIR):
            item_path = os.path.join(BACKUP_DIR, item)
            if os.path.isdir(item_path) and item.startswith("backup_"):
                # 提取时间戳
                timestamp = item.replace("backup_", "")
                # 获取备份大小
                size = sum(os.path.getsize(os.path.join(item_path, f)) for f in os.listdir(item_path) if os.path.isfile(os.path.join(item_path, f)))
                # 获取备份时间
                try:
                    backup_time = datetime.strptime(timestamp, "%Y%m%d_%H%M%S").strftime("%Y-%m-%d %H:%M:%S")
                except:
                    backup_time = "未知"
                
                backups.append({
                    "id": timestamp,
                    "time": backup_time,
                    "size": size,
                    "path": item_path
                })
        
        # 按时间排序，最新的在前
        backups.sort(key=lambda x: x["id"], reverse=True)
        return {"success": True, "backups": backups}
    except Exception as e:
        logger.error(f"列出备份时出错: {str(e)}")
        return {"success": False, "error": str(e)}

def restore_from_backup(backup_id):
    """从备份恢复数据"""
    try:
        backup_path = os.path.join(BACKUP_DIR, f"backup_{backup_id}")
        if not os.path.exists(backup_path):
            return {"success": False, "error": f"备份 {backup_id} 不存在"}
        
        # 恢复向量存储
        vectors_backup = os.path.join(backup_path, os.path.basename(VECTORS_PATH))
        if os.path.exists(vectors_backup):
            shutil.copy2(vectors_backup, VECTORS_PATH)
            logger.info(f"已从 {vectors_backup} 恢复到 {VECTORS_PATH}")
        
        # 恢复文档存储
        documents_backup = os.path.join(backup_path, os.path.basename(DOCUMENTS_PATH))
        if os.path.exists(documents_backup):
            shutil.copy2(documents_backup, DOCUMENTS_PATH)
            logger.info(f"已从 {documents_backup} 恢复到 {DOCUMENTS_PATH}")
        
        # 恢复元数据
        metadata_backup = os.path.join(backup_path, os.path.basename(METADATA_PATH))
        if os.path.exists(metadata_backup):
            shutil.copy2(metadata_backup, METADATA_PATH)
            logger.info(f"已从 {metadata_backup} 恢复到 {METADATA_PATH}")
        
        return {"success": True, "message": f"成功从备份 {backup_id} 恢复数据"}
    except Exception as e:
        logger.error(f"从备份恢复时出错: {str(e)}")
        return {"success": False, "error": str(e)} 