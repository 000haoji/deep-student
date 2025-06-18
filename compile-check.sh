#!/bin/bash

echo "🔍 检查RAG模块编译状态..."

cd src-tauri

# 检查主要错误是否已修复
echo "✅ 检查 vector_store.rs 语法..."
if grep -q "chunks.len()" src/vector_store.rs; then
    echo "❌ 发现chunks.len()错误 - 需要修复borrow checker问题"
else
    echo "✅ chunks borrow checker 问题已修复"
fi

echo "✅ 检查 rag_manager.rs 导入..."
if grep -q "use crate::vector_store::{SqliteVectorStore, VectorStore};" src/rag_manager.rs; then
    echo "✅ VectorStore trait 导入已修复"
else
    echo "❌ VectorStore trait 导入缺失"
fi

echo "✅ 检查 llm_manager.rs 响应处理..."
if grep -q "let status = response.status();" src/llm_manager.rs; then
    echo "✅ Response borrow checker 问题已修复"
else
    echo "❌ Response borrow checker 问题未修复"
fi

echo ""
echo "🚀 开始快速编译检查..."
timeout 30 cargo check --message-format short 2>&1 | head -10

echo ""
echo "📋 RAG模块编译检查完成"