# Deep-Students 错题分析与知识库查询系统

Deep-Students致力于使用LLM与RAG对错题进行分析与管理，
减少学习者在错题整理与复习上的无效努力，提高学习效率。
项目仍然处于早期阶段，开发中可能存在一些BUG。

BILIBILI 演示视频
【Deep-Student 开源AI错题管理软件】 
https://www.bilibili.com/video/BV1rcQVY3EDo/?share_source=copy_web&vd_source=1c0ff08e122edb92a89179c0a64878aa

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

## 目前已经实现的功能有：

### 1.使用Deepseek-V3以及bge-m3的RAG知识库
可以分多个文档库对文档进行管理，对上传的文档进行分段，以及调用Deepseek-V3进行对知识库的查询，支持备份与恢复。
（暂时作为单独运行的模块，与主程序分离进行调用，部分实现参考了Cherry Studio的知识库）

### 2.错题初次分析
主程序支持上传多道题目，每道题目可以**上传多个图片并且给出补充信息**，可以对错题进行**批量初次分析**。
目前使用Qwen-VL作为图像分析模型进行OCR，提取图片中的题目，并且生成标签等相关信息。
上一流程将OCR后的文字传给Deepseek-R1进行进一步分析，支持显示推理过程。
（~~黏合国~~黏合实现多模态）

**初次分析适用于错题的首次整理**。



### 3.错题回顾分析
主程序支持创建**多个学科**每个学科都有自己独立的错题库，
用户可以在错题库中通过标签筛选想要复习的内容，选择多个错题使用DeepseekR1进行**回顾分析**，
回顾分析将致力于**从多个不同的错题中查找出错误共性以及潜藏的问题**。

**回顾分析适用于错题的按章节复习或总复习**。

### 4.自定义三阶段prompt
主程序支持调整Qwen-VL阶段,Deepseek R1阶段,回顾分析阶段三个阶段的提示词，
用户可以**自行调整以适应自身的学习风格**。

### 5.按照优先级选择Deepseek API
Deepseek-R1可以设置三个API并设置优先级，防止某个API不可用导致堵塞。

### 6.支持备份与恢复功能。



由于个人能力原因，项目还存在许多BUG，希望对此有兴趣的朋友能一同参与该项目的开发。

### 尚未实现的todolist：
- 多线程批量分析
- 主页初次分析重新生成选项
- RAG知识库的前端配置
- 使用LLM进行Anki制卡
- 前端自由调整LLM的其他参数如temperature等
- 前端自由替换模型为其他LLM如Gemini，Claude
- 调整prompt以及对错题数据的管理，实现更好的利用错题
- 将RAG知识库整合进主程序流程中，通过RAG降低分析错题时LLM的幻觉
- 同一张图片内多道题目的分割
- 完善RAG模块，实现更高效优雅的知识库
- 复习计划功能暂未实现，未来或使用FSRS算法？
- 桌面端与安卓端的实现
