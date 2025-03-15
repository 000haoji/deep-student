#!/usr/bin/env python
# -*- coding: utf-8 -*-

"""
DeepSeek知识库查询系统
基于Haystack 2.0和FastRAG框架实现
支持多种文件类型的上传和查询
"""

import os
import json
import requests
import threading
import time
import sys
import argparse
import re
from flask import Flask, render_template, request, jsonify, send_from_directory, Response, stream_with_context
from flask_cors import CORS
from haystack import Pipeline, Document
import uuid
from datetime import datetime

# 导入核心模块
from core.config import (
    logger, DATA_DIR, EMBEDDER_API_KEY, EMBEDDER_API_URL, EMBEDDER_MODEL,
    LLM_API_KEY, LLM_API_URL, LLM_MODEL, USE_GO_PROXY, GO_PROXY_URL,
    FAST_MODE, DEBUG_MODE, HOST, PORT, AUTO_BACKUP_INTERVAL, startup_time, ENABLE_GO_PROXY
)
from core.storage import (
    save_documents, save_embeddings, load_documents_and_embeddings,
    save_metadata, load_metadata, backup_data_store, list_backups,
    restore_from_backup, delete_document as delete_from_storage
)
from core.embedders import (
    SiliconFlowBaseEmbedder, SiliconFlowDocumentEmbedder, SiliconFlowTextEmbedder
)
from core.document_processor import (
    process_file, TextSplitter, split_text_into_fragments
)
from core.retrieval import (
    SiliconFlowLLM, setup_retrieval_components, generate_rag_prompt,
    generate_sources_info, extract_json_from_text, get_llm_system_prompt,
    inspect_document_metadata
)
from core.document_processing import clean_and_split_text

# 初始化Flask应用
app = Flask(__name__)

# 全局变量
documents = []
document_store = None
embedding_model = None
llm = None
text_embedder = None
document_embedder = None
embedding_pipeline = Pipeline()
query_pipeline = None
text_splitter = None
retriever = None

def setup_embedding_components():
    """设置嵌入组件"""
    global text_embedder, document_embedder
    
    # 创建文本嵌入器
    text_embedder = SiliconFlowTextEmbedder(
        api_key=EMBEDDER_API_KEY,
        api_url=EMBEDDER_API_URL,
        model=EMBEDDER_MODEL
    )
    logger.info("初始化文本嵌入器组件...")
    
    # 创建文档嵌入器
    document_embedder = SiliconFlowDocumentEmbedder(
        api_key=EMBEDDER_API_KEY,
        api_url=EMBEDDER_API_URL,
        model=EMBEDDER_MODEL
    )
    logger.info("初始化文档嵌入器组件...")

def setup_pipelines():
    """设置处理流水线"""
    global embedding_pipeline, text_splitter
    
    # 创建文本分割器
    text_splitter = TextSplitter(chunk_size=1000, chunk_overlap=200)
    logger.info("初始化文本分割器...")
    
    # 创建文档嵌入流水线
    embedding_pipeline = Pipeline()
    embedding_pipeline.add_component("document_embedder", document_embedder)
    logger.info("创建文档嵌入流水线...")
    
    # 不再使用pipeline连接方式，改为直接调用组件以避免内部处理错误
    logger.info("不再使用pipeline连接方式，改为直接调用组件以避免内部处理错误")

# 路由定义
@app.route('/')
def index():
    """主页"""
    return render_template('index.html')

@app.route('/upload', methods=['POST'])
def upload_document():
    """上传文档"""
    global documents
    try:
        if 'file' in request.files:
            # 处理文件上传
            file = request.files['file']
            if file.filename == '':
                return jsonify({"success": False, "error": "未选择文件"})
            
            # 获取文档库ID，如果没有提供则使用默认值
            library_id = request.form.get('library_id', 'default')
            
            # 处理文件
            result = process_file(file)
            if not result["success"]:
                return jsonify({"success": False, "error": result["error"]})
            
            # 获取处理后的文档
            doc = result["document"]
            
            # 将文档库ID添加到文档元数据
            doc.meta['library_id'] = library_id
            
            # 明确标记为父文档
            doc.meta['is_fragment'] = False
            doc.meta['has_fragments'] = True
            
            # 先保存父文档
            save_documents([doc])
            document_store.write_documents([doc])
            documents.append(doc)
            
            # 分割文档
            chunked_docs = text_splitter.split_document(doc)
            if not chunked_docs:
                return jsonify({"success": False, "error": "文档分割失败"})
                
            # 为每个分段文档添加文档库ID
            for chunk_doc in chunked_docs:
                # 跳过第一个文档(父文档)，因为我们已经处理过它
                if chunk_doc.id == doc.id:
                    continue
                chunk_doc.meta['library_id'] = library_id
                
            # 只处理分段文档，不包括父文档
            segment_docs = [d for d in chunked_docs if d.id != doc.id]
            
            # 为文档生成嵌入向量
            if segment_docs:
                result = embedding_pipeline.run({"documents": segment_docs})
                
                # 检查结果结构
                if "document_embedder" not in result:
                    logger.error(f"嵌入结果中没有 'document_embedder' 键: {result}")
                    return jsonify({"success": False, "error": "文档嵌入失败: 结果格式错误"})
                
                if "documents" not in result["document_embedder"]:
                    logger.error(f"嵌入结果中没有 'documents' 键: {result['document_embedder']}")
                    return jsonify({"success": False, "error": "文档嵌入失败: 结果格式错误"})
                
                embedded_docs = result["document_embedder"]["documents"]
                
                if not embedded_docs:
                    return jsonify({"success": False, "error": "无法为文档生成嵌入向量"})
                
                # 保存到持久化存储
                save_documents(embedded_docs)
                save_embeddings(embedded_docs)
                
                # 添加到文档存储
                document_store.write_documents(embedded_docs)
                
                # 更新全局文档列表
                documents.extend(embedded_docs)
            
            return jsonify({
                "success": True,
                "message": f"文档 '{file.filename}' 已成功添加到知识库",
                "document_id": doc.id
            })
        elif request.is_json:
            # 处理JSON请求（手动输入文本）
            data = request.json
            text = data.get("text", "").strip()
            metadata = data.get("metadata", {})
            
            if not text:
                return jsonify({"success": False, "error": "文本内容不能为空"})
            
            # 创建文档
            doc = Document(
                id=str(uuid.uuid4()),
                content=text,
                meta=metadata
            )
            
            # 分割文档
            chunked_docs = text_splitter.split_document(doc)
            if not chunked_docs:
                return jsonify({"success": False, "error": "文档分割失败"})
            
            # 为文档生成嵌入向量
            result = embedding_pipeline.run({"documents": chunked_docs})
            
            # 检查结果结构
            if "document_embedder" not in result:
                logger.error(f"嵌入结果中没有 'document_embedder' 键: {result}")
                return jsonify({"success": False, "error": "文档嵌入失败: 结果格式错误"})
            
            if "documents" not in result["document_embedder"]:
                logger.error(f"嵌入结果中没有 'documents' 键: {result['document_embedder']}")
                return jsonify({"success": False, "error": "文档嵌入失败: 结果格式错误"})
            
            embedded_docs = result["document_embedder"]["documents"]
            
            if not embedded_docs:
                return jsonify({"success": False, "error": "无法为文档生成嵌入向量"})
            
            # 保存到持久化存储
            save_documents(embedded_docs)
            save_embeddings(embedded_docs)
            
            # 添加到文档存储
            document_store.write_documents(embedded_docs)
            
            # 更新全局文档列表
            documents.extend(embedded_docs)

            return jsonify({
                "success": True,
                "message": f"文档 '{metadata.get('title', '未命名')}' 已成功添加到知识库",
                "document_id": doc.id
            })
        else:
            return jsonify({"success": False, "error": "无效的请求"})
    except Exception as e:
        logger.error(f"上传文档时出错: {str(e)}")
        return jsonify({"success": False, "error": f"处理文档时出错: {str(e)}"})

@app.route('/query', methods=['POST'])
def query():
    """查询知识库"""
    try:
        data = request.json
        query_text = data.get("query", "").strip()
        
        if not query_text:
            return jsonify({"success": False, "error": "查询不能为空"})
        
        logger.info(f"收到查询请求: '{query_text}'")
        
        # 获取文档库ID，如果没有提供则使用默认值
        library_id = data.get("library_id", "default")
        logger.info(f"查询文档库: '{library_id}'")
            
        # 检查必要组件是否已初始化
        if retriever is None or document_store is None:
            logger.error("查询所需组件未初始化")
            return jsonify({"success": False, "error": "系统组件未正确初始化，请重新启动应用"})
            
        try:
            # 使用文本嵌入器生成嵌入向量
            logger.info(f"为查询生成嵌入向量: '{query_text}'")
            
            # 首先执行文本嵌入，检查是否成功
            embedding_result = text_embedder.run(text=query_text)
            if not embedding_result or "embedding" not in embedding_result or embedding_result["embedding"] is None:
                logger.error("无法生成查询文本的嵌入向量，可能是API连接问题")
                return jsonify({"success": False, "error": "无法生成查询的嵌入向量，请稍后再试"})
                
            # 设置过滤条件，按文档库ID过滤
            filters = None
            if library_id and library_id != "all":
                logger.info(f"正在设置文档库过滤条件: {library_id}")
                filters = {"library_id": library_id}
            else:
                logger.info("不使用文档库过滤")
            
            # 使用检索器直接检索相关文档
            retriever_result = retriever.run(
                query_embedding=embedding_result["embedding"],
                filters=filters
            )
            
            # 构建完整的查询结果
            query_result = {
                "retriever": retriever_result
            }
            
            # 检查结果结构
            if "retriever" not in query_result:
                logger.error(f"查询结果中没有 'retriever' 键: {query_result}")
                return jsonify({
                    "success": False,
                    "error": "查询处理失败: 结果格式错误"
                })
            
            if "documents" not in query_result["retriever"]:
                logger.error(f"查询结果中没有 'documents' 键: {query_result['retriever']}")
                return jsonify({
                    "success": False,
                    "error": "查询处理失败: 结果格式错误"
                })
            
            retrieved_documents = query_result["retriever"]["documents"]
            logger.info(f"检索到的文档数量: {len(retrieved_documents) if retrieved_documents else 0}")
            
            if not retrieved_documents:
                return jsonify({
                    "success": True,
                    "answer": "对不起，我没有找到与您问题相关的信息。请尝试用不同的方式提问，或者上传更多相关文档。",
                    "sources": []
                })
            
            # 生成RAG提示
            prompt = generate_rag_prompt(query_text, retrieved_documents)
            
            # 使用LLM生成回答
            answer = llm.generate(
                prompt=prompt,
                system_prompt="你是一个专业的知识库助手，使用中文回答用户问题。"
            )
            
            if not answer:
                return jsonify({
                    "success": False,
                    "error": "生成回答时出错"
                })
            
            # 生成来源信息
            sources = generate_sources_info(retrieved_documents)
            
            return jsonify({
                "success": True,
                "answer": answer,
                "sources": sources
            })
        except Exception as e:
            logger.error(f"查询处理时出错: {str(e)}")
            return jsonify({"success": False, "error": f"查询处理时出错: {str(e)}"})
    except Exception as e:
        logger.error(f"查询处理时出错: {str(e)}")
        return jsonify({"success": False, "error": f"查询处理时出错: {str(e)}"})

@app.route('/query/stream', methods=['POST'])
def query_stream():
    """流式查询知识库"""
    try:
        data = request.json
        query_text = data.get("query", "").strip()
        
        if not query_text:
            return jsonify({"success": False, "error": "查询不能为空"})
        
        logger.info(f"收到流式查询请求: '{query_text}'")
        
        # 获取文档库ID，如果没有提供则使用默认值
        library_id = data.get("library_id", "default")
        logger.info(f"查询文档库: '{library_id}'")
        
        # 检查必要组件是否已初始化
        if retriever is None or document_store is None or llm is None:
            logger.error("查询所需组件未初始化")
            return jsonify({"success": False, "error": "系统组件未正确初始化，请重新启动应用"})
        
        def generate():
            # 记录开始时间
            start_time = time.time()
            
            try:
                # 使用文本嵌入器生成嵌入向量
                logger.info(f"为查询生成嵌入向量: '{query_text}'")
                
                # 首先执行文本嵌入，检查是否成功
                embedding_result = text_embedder.run(text=query_text)
                if not embedding_result or "embedding" not in embedding_result or embedding_result["embedding"] is None:
                    logger.error("无法生成查询文本的嵌入向量，可能是API连接问题")
                    yield f"data: {json.dumps({'error': '无法生成查询的嵌入向量，请稍后再试'})}\n\n"
                    yield "data: [DONE]\n\n"
                    return
                    
                # 设置过滤条件，按文档库ID过滤
                filters = None
                if library_id and library_id != "all":
                    logger.info(f"正在设置文档库过滤条件: {library_id}")
                    filters = {"library_id": library_id}
                else:
                    logger.info("不使用文档库过滤")
                
                # 使用检索器直接检索相关文档
                retriever_result = retriever.run(
                    query_embedding=embedding_result["embedding"],
                    filters=filters
                )
                
                # 构建完整的查询结果
                query_result = {
                    "retriever": retriever_result
                }
                
                # 检查结果结构
                if "retriever" not in query_result or "documents" not in query_result["retriever"]:
                    logger.error(f"查询结果中没有 'retriever' 键或 'documents' 键: {query_result}")
                    yield f"data: {json.dumps({'error': '查询处理失败: 结果格式错误'})}\n\n"
                    yield "data: [DONE]\n\n"
                    return
                
                retrieved_documents = query_result["retriever"]["documents"]
                logger.info(f"检索到的文档数量: {len(retrieved_documents) if retrieved_documents else 0}")
                
                if not retrieved_documents:
                    yield f"data: {json.dumps({'answer': '对不起，我没有找到与您问题相关的信息。请尝试用不同的方式提问，或者上传更多相关文档。', 'sources': []})}\n\n"
                    yield "data: [DONE]\n\n"
                    return
                
                # 生成RAG提示
                prompt = generate_rag_prompt(query_text, retrieved_documents)
                
                # 使用Go代理或直接调用API
                if USE_GO_PROXY:
                    # 使用Go代理进行流式生成
                    response = requests.post(
                        GO_PROXY_URL,
                        json={
                            "api_key": LLM_API_KEY,
                            "api_url": LLM_API_URL,  # 传递原始API URL
                            "messages": [
                                {"role": "system", "content": "你是一个专业的知识库助手，使用中文回答用户问题。请使用Markdown格式输出，包括标题、列表、加粗、引用等，以提高回答的可读性。"},
                                {"role": "user", "content": prompt}
                            ],
                            "max_tokens": 1000
                        }
                    )
                    
                    # 检查Go代理响应状态
                    if response.status_code != 200:
                        error_msg = f"Go代理API请求失败: {response.status_code}"
                        try:
                            error_data = response.json()
                            if "error" in error_data:
                                error_msg = f"Go代理API错误: {error_data['error']}"
                        except:
                            pass
                        
                        yield f"data: {json.dumps({'error': error_msg})}\n\n"
                        yield "data: [DONE]\n\n"
                        return
                    
                    # 处理Go代理响应
                    try:
                        # Go代理响应可能是整个结果，而不是流式的
                        proxy_data = response.json()
                        if "content" in proxy_data:
                            # 直接传递完整内容
                            yield f"data: {json.dumps({'choices': [{'delta': {'content': proxy_data['content']}}]})}\n\n"
                        elif "choices" in proxy_data and len(proxy_data["choices"]) > 0:
                            # 标准格式响应
                            delta = proxy_data["choices"][0].get("delta", {})
                            if "content" in delta and delta["content"]:
                                yield f"data: {json.dumps(proxy_data)}\n\n"
                        else:
                            # 未识别的响应格式
                            yield f"data: {json.dumps({'error': '未能解析Go代理响应'})}\n\n"
                    except Exception as e:
                        logger.error(f"解析Go代理响应时出错: {str(e)}")
                        yield f"data: {json.dumps({'error': f'解析Go代理响应出错: {str(e)}'})}\n\n"
                    
                    # Go代理处理完成
                    yield "data: [DONE]\n\n"
                else:
                    # 直接调用API
                    response = requests.post(
                        LLM_API_URL,
                        headers={
                            "Content-Type": "application/json",
                            "Authorization": f"Bearer {LLM_API_KEY}"
                        },
                        json={
                            "model": LLM_MODEL,
                            "messages": [
                                {"role": "system", "content": "你是一个专业的知识库助手，使用中文回答用户问题。请使用Markdown格式输出，包括标题、列表、加粗、引用等，以提高回答的可读性。"},
                                {"role": "user", "content": prompt}
                            ],
                            "stream": True,
                            "max_tokens": 1000
                        },
                        stream=True
                    )
                    
                    # 检查响应状态
                    if response.status_code != 200:
                        error_msg = f"API请求失败: {response.status_code}"
                        try:
                            error_data = response.json()
                            if "error" in error_data:
                                error_msg = f"API错误: {error_data['error']}"
                        except:
                            pass
                        
                        yield f"data: {json.dumps({'error': error_msg})}\n\n"
                        yield "data: [DONE]\n\n"
                        return
                    
                    # 处理流式响应
                    for line in response.iter_lines():
                        if line:
                            line = line.decode('utf-8')
                            if line.startswith('data: '):
                                data = line[6:]  # 去掉 'data: ' 前缀
                                if data == '[DONE]':
                                    break
                                
                                try:
                                    json_data = json.loads(data)
                                    if isinstance(json_data, dict) and "choices" in json_data:
                                        choices = json_data["choices"]
                                        if isinstance(choices, list) and len(choices) > 0:
                                            delta = choices[0].get("delta", {})
                                            if isinstance(delta, dict) and "content" in delta and delta["content"]:
                                                # 直接传递API响应
                                                yield f"data: {data}\n\n"
                                except Exception as e:
                                    logger.error(f"解析流式响应时出错: {str(e)}")
                
                # 生成来源信息只在直接调用API且未提前返回的情况下执行
                try:
                    sources = generate_sources_info(retrieved_documents)
                    if sources:  # 确保 sources 不是 None
                        yield f"data: {json.dumps({'type': 'sources', 'sources': sources})}\n\n"
                except Exception as e:
                    logger.error(f"生成来源信息时出错: {str(e)}")
                    yield f"data: {json.dumps({'error': f'生成来源信息时出错: {str(e)}'})}\n\n"
                
                # 发送完成标记
                yield "data: [DONE]\n\n"
                
                # 记录总时间
                total_time = time.time() - start_time
                logger.info(f"查询总耗时: {total_time:.2f}秒")
                
            except Exception as e:
                logger.error(f"流式生成时出错: {str(e)}")
                yield f"data: {json.dumps({'error': f'处理查询时出错: {str(e)}'})}\n\n"
                yield "data: [DONE]\n\n"
        
        return Response(stream_with_context(generate()), mimetype='text/event-stream')
    except Exception as e:
        logger.error(f"流式查询处理时出错: {str(e)}")
        return jsonify({"success": False, "error": f"流式查询处理时出错: {str(e)}"})

@app.route('/documents', methods=['GET'])
def list_documents():
    """获取所有文档列表"""
    try:
        # 始终从持久化存储中重新加载文档，确保获取最新状态
        documents = load_documents_and_embeddings(skip_embeddings=True)
        logger.info(f"重新加载了 {len(documents)} 个文档")
        
        if documents is None or len(documents) == 0:
            logger.info("没有找到文档，返回空列表")
            return jsonify({"success": True, "documents": []})
        
        # 基本文档信息
        doc_list = []
        parent_doc_ids = set()  # 用于跟踪已处理的父文档ID
        
        # 记录所有父文档信息，用于调试
        parent_docs = [doc for doc in documents if not doc.meta.get("is_fragment", False)]
        # 对父文档进行排序，确保返回结果的稳定性
        parent_docs.sort(key=lambda x: x.id)
        
        logger.info(f"找到 {len(parent_docs)} 个父文档")
        for i, doc in enumerate(parent_docs):
            logger.info(f"父文档 {i+1}: ID={doc.id}, 文件名={doc.meta.get('filename', '无文件名')}")
        
        # 只处理父文档，不处理分段
        for doc in parent_docs:
            # 如果已经处理过这个ID的文档，跳过
            if doc.id in parent_doc_ids:
                logger.warning(f"跳过重复的父文档ID: {doc.id}, 文件名: {doc.meta.get('filename', '无文件名')}")
                continue
            
            # 记录已处理的父文档ID
            parent_doc_ids.add(doc.id)
            
            doc_meta = doc.meta.copy() if doc.meta else {}
            
            # 统计该文档的分段数量
            fragment_count = sum(1 for d in documents if d.meta.get("parent_id") == doc.id)
            
            # 优先使用filename作为标题，如果没有则使用title，都没有则显示"未命名文档"
            title = doc_meta.get("filename", doc_meta.get("title", "未命名文档"))
            
            doc_info = {
                "id": doc.id,
                "title": title,
                "filename": doc_meta.get("filename", ""),
                "file_type": doc_meta.get("file_type", ""),
                "uploaded_at": doc_meta.get("uploaded_at", ""),
                "fragment_count": fragment_count,
                "library_id": doc_meta.get("library_id", "default")  # 确保返回文档库ID
            }
            doc_list.append(doc_info)
            logger.info(f"添加文档到列表: ID={doc.id}, 标题={title}")
        
        # 对最终结果再次排序，确保返回结果的稳定性
        doc_list.sort(key=lambda x: x["id"])
        
        logger.info(f"返回 {len(doc_list)} 个文档")
        return jsonify({"success": True, "documents": doc_list})
    except Exception as e:
        logger.error(f"获取文档列表时出错: {str(e)}")
        return jsonify({"success": False, "error": f"获取文档列表时出错: {str(e)}"})

@app.route('/libraries', methods=['GET'])
def list_libraries():
    """获取所有文档库列表"""
    try:
        global documents
        
        # 从文档中统计使用中的文档库
        library_counts = {}
        for doc in documents:
            # 只计算父文档，不计算分段
            if not doc.meta.get("is_fragment", False):
                lib_id = doc.meta.get("library_id", "default")
                if lib_id not in library_counts:
                    library_counts[lib_id] = 0
                library_counts[lib_id] += 1
        
        # 获取元数据中记录的所有文档库
        metadata = load_metadata()
        metadata_libraries = metadata.get("libraries", {})
        
        # 合并文档库信息
        library_info = {}
        
        # 先添加使用中的文档库
        for lib_id, doc_count in library_counts.items():
            library_info[lib_id] = {
                "id": lib_id,
                "name": lib_id.capitalize() if lib_id != "default" else "默认库",
                "document_count": doc_count
            }
        
        # 添加/更新元数据中的文档库
        for lib_id, lib_data in metadata_libraries.items():
            if lib_id in library_info:
                # 更新名称（保留文档计数）
                library_info[lib_id]["name"] = lib_data.get("name", lib_id.capitalize())
            else:
                # 添加新的文档库
                library_info[lib_id] = {
                    "id": lib_id,
                    "name": lib_data.get("name", lib_id.capitalize()),
                    "document_count": 0
                }
        
        # 转换为列表
        library_list = list(library_info.values())
        
        # 确保默认库排在首位
        library_list.sort(key=lambda x: 0 if x["id"] == "default" else 1)
        
        return jsonify({"success": True, "libraries": library_list})
    except Exception as e:
        logger.error(f"获取文档库列表时出错: {str(e)}")
        return jsonify({"success": False, "error": f"获取文档库列表时出错: {str(e)}"})

@app.route('/libraries', methods=['POST'])
def create_library():
    """创建新的文档库"""
    try:
        global documents
        
        data = request.json
        library_id = data.get("library_id", "").strip()
        library_name = data.get("library_name", "").strip()
        
        if not library_id:
            return jsonify({"success": False, "error": "文档库ID不能为空"})
        
        # 检查ID是否合法（只允许字母、数字、下划线和连字符）
        if not re.match(r'^[a-zA-Z0-9_-]+$', library_id):
            return jsonify({"success": False, "error": "文档库ID只能包含字母、数字、下划线和连字符"})
        
        # 检查是否已存在同名文档库
        existing_libraries = []
        for doc in documents:
            lib_id = doc.meta.get("library_id", "default")
            if lib_id not in existing_libraries:
                existing_libraries.append(lib_id)
        
        if library_id in existing_libraries:
            return jsonify({"success": False, "error": f"已存在ID为 '{library_id}' 的文档库"})
        
        # 创建一个空的文档库（实际上只需在元数据中记录即可）
        library_metadata = {
            "id": library_id,
            "name": library_name if library_name else library_id,
            "created_at": datetime.now().strftime("%Y-%m-%d %H:%M:%S"),
            "document_count": 0
        }
        
        # 保存到元数据
        metadata = load_metadata()
        if "libraries" not in metadata:
            metadata["libraries"] = {}
        metadata["libraries"][library_id] = library_metadata
        save_metadata(metadata)
        
        return jsonify({
            "success": True, 
            "message": f"文档库 '{library_id}' 创建成功",
            "library": library_metadata
        })
    except Exception as e:
        logger.error(f"创建文档库时出错: {str(e)}")
        return jsonify({"success": False, "error": f"创建文档库时出错: {str(e)}"})

@app.route('/libraries/<library_id>', methods=['DELETE'])
def delete_library(library_id):
    """删除文档库及其所有文档"""
    try:
        global documents
        
        if library_id == "default":
            return jsonify({"success": False, "error": "不能删除默认文档库"})
        
        # 获取该库中的所有文档ID
        doc_ids_to_delete = []
        for doc in documents:
            if doc.meta.get("library_id") == library_id and not doc.meta.get("is_fragment", False):
                doc_ids_to_delete.append(doc.id)
        
        # 删除所有文档
        deleted_count = 0
        for doc_id in doc_ids_to_delete:
            success = delete_from_storage(doc_id)
            if success:
                deleted_count += 1
        
        # 从元数据中删除文档库记录
        metadata = load_metadata()
        if "libraries" in metadata and library_id in metadata["libraries"]:
            del metadata["libraries"][library_id]
            save_metadata(metadata)
        
        # 重新加载文档
        documents = load_documents_and_embeddings()
        
        # 重新设置检索组件
        global document_store, retriever
        document_store, retriever = setup_retrieval_components(documents, text_embedder)
        
        return jsonify({
            "success": True,
            "message": f"文档库 '{library_id}' 已删除，共删除 {deleted_count} 个文档"
        })
    except Exception as e:
        logger.error(f"删除文档库时出错: {str(e)}")
        return jsonify({"success": False, "error": f"删除文档库时出错: {str(e)}"})

@app.route('/documents/<doc_id>', methods=['DELETE'])
def delete_document(doc_id):
    """删除文档"""
    global documents
    try:
        # 从文档存储中获取所有文档
        all_docs = document_store.filter_documents()
        logger.info(f"开始删除文档 {doc_id}，当前文档总数: {len(all_docs)}")
        
        # 首先检查元数据中是否有这个文档ID
        metadata = load_metadata()
        metadata_updated = False
        
        # 提取基础文档ID（移除_chunk_部分）
        base_doc_id = doc_id.split("_chunk_")[0] if "_chunk_" in doc_id else doc_id
        logger.info(f"提取的基础文档ID: {base_doc_id}")
        
        # 更新元数据中相关的记录
        if 'documents' in metadata:
            # 创建新的文档列表，排除要删除的文档
            new_documents = []
            for doc_meta in metadata['documents']:
                meta_id = doc_meta.get('id', '')
                meta_base_id = meta_id.split("_chunk_")[0] if "_chunk_" in meta_id else meta_id
                
                # 如果元数据中的文档ID与要删除的基础ID不同，则保留
                if meta_base_id != base_doc_id:
                    new_documents.append(doc_meta)
                else:
                    metadata_updated = True
                    logger.info(f"从元数据中删除文档 {meta_id}")
            
            if metadata_updated:
                metadata['documents'] = new_documents
                metadata['document_count'] = len(new_documents)
                save_metadata(metadata)
                logger.info(f"已从元数据中删除相关的文档记录")
        
        # 找出要删除的文档
        docs_to_delete = []
        doc_ids_to_delete = set()
        
        # 找出所有与该基础ID相关的文档片段
        for doc in all_docs:
            meta = doc.meta or {}
            doc_id_to_check = doc.id
            
            # 如果是目标文档或其相关片段
            if doc_id_to_check == doc_id or doc_id_to_check.startswith(f"{base_doc_id}_chunk_"):
                docs_to_delete.append(doc)
                doc_ids_to_delete.add(doc_id_to_check)
                logger.info(f"找到要删除的文档片段: {doc_id_to_check}")
                continue
            
            # 检查元数据中的parent_id是否匹配
            parent_id = meta.get("parent_id")
            if parent_id and (parent_id == base_doc_id):
                docs_to_delete.append(doc)
                doc_ids_to_delete.add(doc_id_to_check)
                logger.info(f"通过parent_id找到要删除的文档片段: {doc_id_to_check}")
        
        if not docs_to_delete:
            logger.warning(f"找不到ID为 {doc_id} 的文档或相关片段")
            return jsonify({"success": False, "error": f"找不到ID为 {doc_id} 的文档或相关片段"})
        
        logger.info(f"找到 {len(docs_to_delete)} 个要删除的文档片段")
        
        # 从文档存储中删除
        document_store.delete_documents([doc.id for doc in docs_to_delete])
        logger.info(f"从文档存储中删除了 {len(docs_to_delete)} 个文档片段")
        
        # 持久化删除文档
        delete_result = delete_from_storage(base_doc_id)
        if not delete_result:
            logger.warning(f"从持久化存储中删除文档 {base_doc_id} 失败，可能导致重启后文档重新出现")
        else:
            logger.info(f"成功从持久化存储中删除文档 {base_doc_id} 及其相关片段")
        
        # 更新全局文档列表 - 从持久化存储中重新加载，确保获取最新状态
        documents = load_documents_and_embeddings(skip_embeddings=True)
        logger.info(f"更新了全局文档列表，剩余 {len(documents)} 个文档")
        
        return jsonify({
            "success": True, 
            "deleted": len(docs_to_delete),
            "ids": sorted(list(doc_ids_to_delete)),
            "message": f"成功删除了 {len(docs_to_delete)} 个文档片段"
        })
    except Exception as e:
        logger.error(f"删除文档时出错: {str(e)}")
        return jsonify({"success": False, "error": f"删除文档时出错: {str(e)}"})

@app.route('/favicon.ico')
def favicon():
    """提供网站图标"""
    return send_from_directory(os.path.join(app.root_path, 'static'), 'favicon.ico')

@app.route('/system/backup', methods=['POST'])
def create_backup():
    """创建备份"""
    try:
        result = backup_data_store()
        if result["success"]:
            return jsonify({
                "success": True,
                "message": f"成功创建备份 {result['backup_id']}",
                "backup_id": result["backup_id"]
            })
        else:
            return jsonify({
                "success": False,
                "error": result["error"]
            })
    except Exception as e:
        logger.error(f"创建备份时出错: {str(e)}")
        return jsonify({"success": False, "error": f"创建备份时出错: {str(e)}"})

@app.route('/system/backups', methods=['GET'])
def get_backups():
    """获取备份列表"""
    try:
        result = list_backups()
        if result["success"]:
            return jsonify({
                "success": True,
                "backups": result["backups"]
            })
        else:
            return jsonify({
                "success": False,
                "error": result["error"]
            })
    except Exception as e:
        logger.error(f"获取备份列表时出错: {str(e)}")
        return jsonify({"success": False, "error": f"获取备份列表时出错: {str(e)}"})

@app.route('/system/restore/<backup_id>', methods=['POST'])
def restore_backup(backup_id):
    """从备份恢复"""
    try:
        result = restore_from_backup(backup_id)
        if result["success"]:
            # 重新初始化应用
            initialize_app()
            return jsonify({
                "success": True,
                "message": result["message"]
            })
        else:
            return jsonify({
                "success": False,
                "error": result["error"]
            })
    except Exception as e:
        logger.error(f"从备份恢复时出错: {str(e)}")
        return jsonify({"success": False, "error": f"从备份恢复时出错: {str(e)}"})

def initialize_app(fast_mode=False):
    """初始化应用
    
    Args:
        fast_mode: 是否启用快速启动模式，启用后将跳过嵌入向量加载并使用懒加载
    """
    global document_store, retriever, documents, llm
    
    logger.info(f"开始初始化应用...{'(快速模式)' if fast_mode else ''}")
    
    # 确保数据目录存在
    os.makedirs(DATA_DIR, exist_ok=True)
    logger.info(f"确保数据目录结构存在: {DATA_DIR}")
    
    # 加载文档和嵌入向量 - 加载已有的嵌入向量
    documents = load_documents_and_embeddings(skip_embeddings=False)
    logger.info(f"从持久化存储加载了 {len(documents)} 个文档")
    
    # 检查文档元数据结构
    inspect_document_metadata(documents)
    
    try:
        # 设置嵌入组件
        setup_embedding_components()
        logger.info("嵌入组件设置完成")
        
        # 检查API密钥是否已配置
        if not EMBEDDER_API_KEY:
            logger.warning("警告: 未配置EMBEDDER_API_KEY环境变量，将无法进行嵌入向量操作")
            print("\n警告: 未配置EMBEDDER_API_KEY环境变量，请在.env文件中添加或设置环境变量。将使用空的文档存储继续。\n")
    
        # 设置检索组件
        document_store, retriever = setup_retrieval_components(documents, text_embedder, lazy_embedding=False)
        logger.info(f"检索组件设置完成，document_store类型: {type(document_store)}, retriever类型: {type(retriever)}")
        
        # 设置流水线
        setup_pipelines()
        logger.info("处理流水线创建完成")
        
    except Exception as e:
        logger.error(f"初始化应用时出错: {str(e)}")
        print(f"\n初始化应用时发生错误: {str(e)}")
        print("尝试使用备选方案继续启动...")
        
        # 创建一个空的文档存储和检索器
        document_store = InMemoryDocumentStore()
        retriever = InMemoryEmbeddingRetriever(document_store=document_store, top_k=5)
        
        # 尝试设置流水线
        try:
            setup_pipelines()
            logger.info("使用备选方案设置流水线完成")
        except Exception as e:
            logger.error(f"设置备选流水线时出错: {str(e)}")
            print(f"设置备选流水线时出错: {str(e)}")
            print("应用可能无法正常工作，请检查日志和配置。")
            
    # 创建LLM实例
    llm = SiliconFlowLLM(
        api_key=LLM_API_KEY,
        api_url=LLM_API_URL,
        model=LLM_MODEL
    )
    
    # 创建应用启动时的初始备份
    logger.info("正在创建应用启动时的初始备份...")
    backup_data_store()
    
    # 设置自动备份
    if AUTO_BACKUP_INTERVAL > 0:
        schedule_automatic_backup()
        logger.info(f"已设置自动备份，间隔为 {AUTO_BACKUP_INTERVAL} 秒")

def schedule_automatic_backup():
    """调度自动备份"""
    def backup_task():
        while True:
            # 等待指定的时间间隔
            time.sleep(AUTO_BACKUP_INTERVAL)
            
            # 执行备份
            try:
                logger.info("执行自动备份...")
                result = backup_data_store()
                if result["success"]:
                    logger.info(f"自动备份成功: {result['backup_id']}")
                else:
                    logger.error(f"自动备份失败: {result.get('error', '未知错误')}")
            except Exception as e:
                logger.error(f"执行自动备份时出错: {str(e)}")
    
    # 在后台线程中运行
    backup_thread = threading.Thread(target=backup_task, daemon=True)
    backup_thread.start()

if __name__ == "__main__":
    # 输出系统配置信息
    print(f"\n{'-'*40}")
    print(f"RAG知识库系统启动中...")
    print(f"{'-'*40}")
    print(f"- 嵌入模型: {EMBEDDER_MODEL}")
    print(f"- LLM模型: {LLM_MODEL}")
    print(f"- 数据目录: {DATA_DIR}")
    print(f"- 文档向量存储: {os.path.join(DATA_DIR, 'vectors.sqlite')}")
    print(f"- 文档内容存储: {os.path.join(DATA_DIR, 'documents.sqlite')}")
    print(f"- 元数据文件: {os.path.join(DATA_DIR, 'metadata.json')}")
    print(f"- Go代理状态: {'启用' if USE_GO_PROXY else '禁用'}")
    print(f"- 快速启动模式: {'启用' if FAST_MODE else '禁用'}")
    print(f"{'-'*40}")
    
    if FAST_MODE:
        print("快速启动模式已启用：系统将跳过嵌入向量预加载并使用懒加载")
        print("注意：首次查询可能会较慢，因为系统需要为相关文档生成嵌入向量")
    
    print("系统具备增强的错误处理能力，即使API连接失败，系统也将继续启动")
    print("请查看界面上的API状态指示器，了解当前API连接状态")
    print(f"{'-'*40}\n")
    
    # 初始化应用
    initialize_app(fast_mode=FAST_MODE)
    
    # 启动Flask应用
    app.run(host=HOST, port=PORT, debug=DEBUG_MODE) 