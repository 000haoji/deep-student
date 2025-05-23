# 硅基流动错题系统 API 文档

本文档提供硅基流动错题系统所有API的详细说明，包括请求方法、URL、参数和返回值。

## 目录

1. [错题管理 API](#错题管理-api)
2. [回顾管理 API](#回顾管理-api)
3. [标签管理 API](#标签管理-api)
4. [备份管理 API](#备份管理-api)
5. [设置管理 API](#设置管理-api)
6. [AI 分析 API](#ai-分析-api)
7. [FastGPT 集成 API](#fastgpt-集成-api)
8. [CherryStudio 集成 API](#cherrystudio-集成-api)

## 错题管理 API

### 上传错题图片并分析

- **URL**: `/api/upload`
- **方法**: `POST`
- **请求体**:
  - `file`: 图片文件
  - `model_type`: 模型类型，默认为 'openai'，可选值：'openai'、'deepseek'、'multimodal_gpt4v' 等
  - `notes`: 用户补充说明（可选）
- **成功响应**:
  ```json
  {
    "success": true,
    "problem_id": "问题唯一ID",
    "analysis": {
      "题目原文": "...",
      "错误分析": "...",
      "题目类型": "...",
      "具体分支": "...",
      "错误类型": "...",
      "难度评估": 3,
      "正确解法": "...",
      "知识点标签": ["标签1", "标签2"]
    }
  }
  ```
- **错误响应**:
  ```json
  {
    "success": false,
    "error": "错误信息"
  }
  ```

### 批量上传错题图片并分析

- **URL**: `/api/upload-multi`
- **方法**: `POST`
- **请求体**:
  - `files`: 多个图片文件
  - `model_type`: 模型类型，默认为 'openai'
  - `notes`: 用户补充说明（可选）
- **成功响应**: 同上传单个错题响应

### 获取所有错题

- **URL**: `/api/problems`
- **方法**: `GET`
- **查询参数**:
  - `sort`: 排序方式，可选值： 'date'、'typicality'、'difficulty'
  - `order`: 排序顺序，可选值： 'asc'、'desc'
  - `limit`: 返回数量限制
  - `offset`: 起始偏移量
  - `category`: 按题目类型筛选
- **成功响应**:
  ```json
  [
    {
      "id": "问题ID",
      "image_path": "图片路径",
      "problem_content": "题目原文",
      "error_analysis": "错误分析",
      "problem_category": "题目类型",
      "problem_subcategory": "具体分支",
      "error_type": "错误类型",
      "difficulty": 3,
      "correct_solution": "正确解法",
      "tags": ["标签1", "标签2"],
      "created_at": "创建时间",
      "typicality": 4,
      "notes": "用户补充说明"
    }
  ]
  ```

### 获取错题详情

- **URL**: `/api/problem/<problem_id>`
- **方法**: `GET`
- **成功响应**:
  ```json
  {
    "id": "问题ID",
    "image_path": "图片路径",
    "image_url": "/uploads/文件名.jpg",
    "problem_content": "题目原文",
    "error_analysis": "错误分析",
    "problem_category": "题目类型",
    "problem_subcategory": "具体分支",
    "error_type": "错误类型",
    "difficulty": 3,
    "correct_solution": "正确解法",
    "tags": ["标签1", "标签2"],
    "created_at": "创建时间",
    "typicality": 4,
    "notes": "用户补充说明",
    "additional_images": ["图片路径1", "图片路径2"]
  }
  ```

### 更新错题信息

- **URL**: `/api/problem/<problem_id>`
- **方法**: `PUT`
- **请求体**:
  ```json
  {
    "problem_content": "题目原文",
    "error_analysis": "错误分析",
    "problem_category": "题目类型",
    "problem_subcategory": "具体分支",
    "error_type": "错误类型",
    "difficulty": 3,
    "correct_solution": "正确解法",
    "tags": ["标签1", "标签2"],
    "notes": "用户补充说明"
  }
  ```
- **成功响应**:
  ```json
  {
    "success": true,
    "message": "错题更新成功"
  }
  ```

### 删除错题

- **URL**: `/api/problem/<problem_id>`
- **方法**: `DELETE`
- **成功响应**:
  ```json
  {
    "success": true,
    "message": "错题删除成功"
  }
  ```

### 批量删除错题

- **URL**: `/api/problems/batch-delete`
- **方法**: `POST`
- **请求体**:
  ```json
  {
    "problem_ids": ["问题ID1", "问题ID2", ...]
  }
  ```
- **成功响应**:
  ```json
  {
    "success": true,
    "message": "成功删除 n 个错题"
  }
  ```

### 更新错题典型度评分

- **URL**: `/api/problem/<problem_id>/typicality`
- **方法**: `POST`
- **请求体**:
  ```json
  {
    "typicality": 4
  }
  ```
- **成功响应**:
  ```json
  {
    "success": true,
    "message": "典型度评分已更新"
  }
  ```

### 更新错题图片

- **URL**: `/api/problem/<problem_id>/image`
- **方法**: `POST`
- **请求体**:
  - `file`: 新的图片文件
- **成功响应**:
  ```json
  {
    "success": true,
    "message": "图片已更新",
    "image_path": "新图片路径",
    "image_url": "/uploads/新文件名.jpg"
  }
  ```

## 回顾管理 API

### 创建回顾分析

- **URL**: `/api/review`
- **方法**: `POST`
- **请求体**:
  ```json
  {
    "problem_ids": ["问题ID1", "问题ID2", ...]
  }
  ```
- **成功响应**:
  ```json
  {
    "success": true,
    "review_id": "回顾ID"
  }
  ```

### 获取所有回顾记录

- **URL**: `/api/reviews`
- **方法**: `GET`
- **成功响应**: 回顾记录列表

### 获取回顾详情

- **URL**: `/api/review/<review_id>`
- **方法**: `GET`
- **成功响应**:
  ```json
  {
    "id": "回顾ID",
    "problems_included": ["问题ID1", "问题ID2", ...],
    "review_analysis": "回顾分析内容",
    "improvement_strategy": "改进策略",
    "created_at": "创建时间",
    "problems": [
      {
        "id": "问题ID",
        "image_url": "/uploads/文件名.jpg",
        "problem_content": "题目原文",
        "error_analysis": "错误分析",
        ...
      }
    ]
  }
  ```

### 获取标签树

- **URL**: `/api/tags/tree`
- **方法**: `GET`
- **成功响应**:
  ```json
  {
    "success": true,
    "tag_tree": {
      "数列": {
        "count": 10,
        "subcategories": {
          "等差数列": {
            "count": 5,
            "tags": {
              "通项公式": 3,
              "求和公式": 2
            }
          },
          "等比数列": {
            "count": 5,
            "tags": {
              "通项公式": 3,
              "求和公式": 2
            }
          }
        }
      },
      ...
    },
    "tag_list": [
      {
        "name": "数列-等差数列-通项公式",
        "count": 3
      },
      ...
    ]
  }
  ```

### 创建复习计划

- **URL**: `/api/review/plan`
- **方法**: `POST`
- **请求体**:
  ```json
  {
    "title": "复习计划标题",
    "description": "复习计划描述",
    "included_tags": ["标签1", "标签2"],
    "excluded_tags": ["标签3"],
    "difficulty_range": [2, 4],
    "problem_count": 10,
    "duration_days": 7
  }
  ```
- **成功响应**:
  ```json
  {
    "success": true,
    "plan_id": "复习计划ID"
  }
  ```

### 获取复习计划列表

- **URL**: `/api/review-plans`
- **方法**: `GET`
- **成功响应**: 
  ```json
  {
    "success": true,
    "reviews": [
      {
        "id": "复习计划ID",
        "problem_id": "关联问题ID",
        "status": "计划状态",
        "review_date": "复习日期",
        "notes": "笔记",
        "created_at": "创建时间"
      },
      ...
    ]
  }
  ```

### 保存回顾分析记录

- **URL**: `/api/review-analysis`
- **方法**: `POST`
- **请求体**:
  ```json
  {
    "title": "分析标题",
    "problem_ids": ["问题ID1", "问题ID2"],
    "analysis_text": "分析文本"
  }
  ```
- **成功响应**:
  ```json
  {
    "success": true,
    "analysis_id": "分析ID"
  }
  ```

### 获取回顾分析记录

- **URL**: `/api/review-analysis/<analysis_id>`
- **方法**: `GET`
- **成功响应**: 回顾分析记录详情

### 获取所有回顾分析记录

- **URL**: `/api/review-analyses`
- **方法**: `GET`
- **成功响应**: 回顾分析记录列表

### 删除回顾分析记录

- **URL**: `/api/review-analysis/<analysis_id>`
- **方法**: `DELETE`
- **成功响应**:
  ```json
  {
    "success": true,
    "message": "分析记录删除成功"
  }
  ```

## 标签管理 API

### 获取所有标签

- **URL**: `/api/tags`
- **方法**: `GET`
- **成功响应**: 标签列表

### 创建标签

- **URL**: `/api/tag`
- **方法**: `POST`
- **请求体**:
  ```json
  {
    "name": "标签名称",
    "category": "标签类别"
  }
  ```
- **成功响应**:
  ```json
  {
    "success": true,
    "tag_id": "标签ID"
  }
  ```

### 更新标签

- **URL**: `/api/tag/<tag_id>`
- **方法**: `PUT`
- **请求体**:
  ```json
  {
    "name": "新标签名称",
    "category": "新标签类别"
  }
  ```
- **成功响应**:
  ```json
  {
    "success": true,
    "message": "标签更新成功"
  }
  ```

### 删除标签

- **URL**: `/api/tag/<tag_id>`
- **方法**: `DELETE`
- **成功响应**:
  ```json
  {
    "success": true,
    "message": "标签删除成功"
  }
  ```

## 备份管理 API

### 创建备份

- **URL**: `/api/backup`
- **方法**: `POST`
- **成功响应**:
  ```json
  {
    "success": true,
    "backup_id": "备份ID",
    "backup_path": "备份文件路径"
  }
  ```

### 获取所有备份

- **URL**: `/api/backups`
- **方法**: `GET`
- **成功响应**: 备份列表

### 还原备份

- **URL**: `/api/backup/<backup_id>/restore`
- **方法**: `POST`
- **成功响应**:
  ```json
  {
    "success": true,
    "message": "备份还原成功"
  }
  ```

### 删除备份

- **URL**: `/api/backup/<backup_id>`
- **方法**: `DELETE`
- **成功响应**:
  ```json
  {
    "success": true,
    "message": "备份删除成功"
  }
  ```

## 设置管理 API

### 获取API设置

- **URL**: `/api/settings`
- **方法**: `GET`
- **成功响应**:
  ```json
  {
    "success": true,
    "data": {
      "default_extraction_model": "multimodal_qwen-vl",
      "default_analysis_model": "deepseek",
      "openai": {
        "api_key": "...",
        "api_url": "...",
        "model": "..."
      },
      "deepseek": {
        "api_key": "...",
        "api_url": "...",
        "model": "..."
      },
      "qwen_vl": {
        "api_key": "...",
        "api_url": "..."
      },
      "gemini": {
        "api_key": "...",
        "api_url": "..."
      },
      "claude": {
        "api_key": "...",
        "api_url": "..."
      },
      "aliyun_ocr": {
        "access_key_id": "...",
        "access_key_secret": "...",
        "region_id": "..."
      },
      "api_alternatives": {
        "deepseek": {
          "api_1": {
            "api_key": "...",
            "api_url": "...",
            "model": "...",
            "priority": 1
          }
        }
      }
    }
  }
  ```

### 更新API设置

- **URL**: `/api/settings`
- **方法**: `POST`
- **请求体**: 与上述响应的`data`字段结构相同
- **成功响应**:
  ```json
  {
    "success": true,
    "message": "API设置已更新"
  }
  ```

### 重新加载配置

- **URL**: `/api/reload_config`
- **方法**: `POST`
- **成功响应**:
  ```json
  {
    "success": true,
    "message": "配置已重新加载"
  }
  ```

### 测试API连接

- **URL**: `/api/test_connection`
- **方法**: `POST`
- **请求体**:
  ```json
  {
    "api_type": "openai",
    "api_key": "sk-xxx",
    "api_url": "https://api.openai.com/v1",
    "model": "gpt-4"
  }
  ```
- **成功响应**:
  ```json
  {
    "success": true,
    "message": "连接成功",
    "details": {
      "response_time": 0.85,
      "model_info": {...}
    }
  }
  ```

### 重置API使用统计

- **URL**: `/api/reset_usage`
- **方法**: `POST`
- **成功响应**:
  ```json
  {
    "success": true,
    "message": "API使用统计已重置"
  }
  ```

## AI 分析 API

### DeepSeek分析

- **URL**: `/api/ai/deepseek-analysis`
- **方法**: `POST`
- **请求体**:
  ```json
  {
    "problems": [
      {
        "id": "问题ID",
        "title": "题目标题",
        "content": "题目内容",
        "user_comments": "用户补充说明",
        "error_cause": "错因分析",
        "knowledge_tags": ["标签1", "标签2"]
      },
      ...
    ]
  }
  ```
- **成功响应**:
  ```json
  {
    "success": true,
    "result": {
      "overview": "综合分析",
      "patterns": "错题模式识别",
      "recommendations": "针对性学习建议"
    }
  }
  ```

## FastGPT 集成 API

FastGPT 集成 API用于与FastGPT服务进行交互，详细实现请参考`fastgpt_integration.py`文件。

## CherryStudio 集成 API

CherryStudio 集成 API用于与CherryStudio服务进行交互，详细实现请参考`cherrystudio_integration.py`文件。