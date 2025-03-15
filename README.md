# Deep-Students 错题分析与知识库查询系统

Deep-Students是一个集成了错题管理、AI分析和知识库查询功能的教育辅助系统。

## 系统架构

系统采用微服务架构，分为两个主要组件：

1. **主程序** - 错题分析系统(端口: 5001)
   - 提供错题录入、分析和回顾功能
   - 支持多种AI模型分析错题

2. **RAG2025** - 知识库查询系统(端口: 5002)
   - 提供知识文档管理和检索功能
   - 基于RAG (检索增强生成) 技术

## 安装与配置

### 前提条件

- Python 3.10 或更高版本
- Git

### 安装步骤

1. 克隆仓库
```bash
git clone https://github.com/000haoji/deep-student.git
cd deep-student
```

2. 为两个子系统创建虚拟环境并安装依赖

主程序:
```bash
cd 主程序
python -m venv venv
venv\Scripts\activate
pip install -r requirements.txt
```

RAG2025:
```bash
cd RAG2025
python -m venv venv
venv\Scripts\activate
pip install -r requirements.txt
```

3. 配置API密钥

- 复制示例配置文件:
  ```bash
  cp 主程序/config.json.example 主程序/config.json
  cp 主程序/config.ini.example 主程序/config.ini
  ```
- 编辑配置文件，添加您的API密钥

## 启动系统

使用集成启动脚本同时启动两个子系统:

```bash
python start_services.py
```

或分别启动各个服务:

```bash
# 启动主程序
cd 主程序
python run.py

# 启动RAG2025
cd RAG2025
python app.py
```

## 系统特性

- **错题管理**: 录入、分类和管理错题
- **AI分析**: 使用先进的AI模型分析错误原因和知识点
- **回顾功能**: 定期回顾分析过的错题，增强学习效果
- **知识库查询**: 查询相关知识点和学习资料
- **双表结构兼容**: 兼容新旧两种数据库表结构

## 技术细节

- 使用Flask作为Web框架
- 支持多种AI模型 (DeepSeek, OpenAI等)
- 使用SQLite作为数据存储
- 前端使用Bootstrap和jQuery
