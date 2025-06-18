# Chat Core Styles

这个目录包含了从现有CSS文件中提取和整理的聊天相关样式，提供了一套完整的聊天界面样式系统。

## 文件结构

```
styles/
├── index.css           # 主入口文件，导入所有样式和定义CSS变量
├── chat.css           # 聊天界面核心样式
├── markdown.css       # Markdown渲染和思维链样式
├── analysis.css       # 分析相关样式（回顾分析、错题选择等）
└── README.md          # 本文档
```

## 样式文件说明

### 1. index.css
- **主入口文件**，统一导入所有样式
- 定义CSS变量系统，便于主题定制
- 提供工具类和通用组件样式
- 包含响应式断点和深色主题支持

### 2. chat.css
- **聊天界面核心样式**，从App.css和DeepStudent.css中提取
- 包含以下主要样式：
  - 聊天容器和布局
  - 聊天头部和工具栏
  - 消息显示（用户消息、助手消息）
  - 流式处理指示器
  - 聊天输入区域
  - 全屏模式支持
  - 聊天界面测试页面样式

### 3. markdown.css
- **Markdown渲染样式**，包含思维链和RAG来源显示
- 包含以下主要样式：
  - 基础Markdown元素（标题、段落、列表等）
  - 代码块和行内代码
  - 表格样式
  - 思维链显示（thinking-section）
  - 推理链条显示（chain-of-thought）
  - RAG来源显示（rag-sources）
  - 流式渲染效果
  - 特殊内容块（信息框、警告框等）

### 4. analysis.css
- **分析相关样式**，从ReviewAnalysis.css中提取
- 包含以下主要样式：
  - 分析容器和布局
  - 分析步骤指示器
  - 分析表单和输入控件
  - 错题选择和筛选界面
  - 分析库卡片展示
  - 分析结果预览
  - 分析模板选择
  - 响应式分析界面设计

## 样式提取来源

这些样式主要从以下文件中提取和整理：

1. **App.css** - 主要的聊天界面样式
   - 流式聊天界面（.streaming-chat-interface）
   - 消息样式（.message, .user-message-content）
   - 聊天容器（.chat-container, .chat-header, .chat-input）
   - Markdown渲染样式
   - 流式处理样式

2. **DeepStudent.css** - 侧边栏和布局样式
   - 聊天容器布局和全屏模式
   - 聊天头部和操作按钮

3. **ReviewAnalysis.css** - 回顾分析相关样式（已提取到analysis.css）
   - 分析容器和布局样式
   - 分析步骤指示器
   - 错题选择界面
   - 分析库卡片展示
   - 表单和按钮样式
   - 筛选和搜索控件

4. **RagQueryView.css** - RAG查询界面样式
   - RAG结果显示
   - 查询配置样式

## 主要功能特性

### 1. 响应式设计
- 支持桌面端、平板和移动端
- 自适应布局和字体大小调整
- 移动端优化的交互体验

### 2. 主题支持
- 浅色主题（默认）
- 深色主题支持（通过 prefers-color-scheme）
- CSS变量系统便于主题定制

### 3. 聊天功能
- 用户和助手消息区分显示
- 流式文本渲染效果
- 打字指示器动画
- 全屏聊天模式
- 消息时间戳显示

### 4. 思维链显示
- 思维过程可视化
- 推理步骤展示
- 可折叠的思维内容
- RAG来源信息显示

### 5. Markdown增强
- 完整的Markdown语法支持
- 代码高亮（多语言支持）
- 表格美化
- 数学公式支持
- 特殊内容块（信息、警告、成功、错误）

### 6. 无障碍支持
- 键盘导航支持
- 屏幕阅读器友好
- 高对比度支持
- 焦点指示器

### 7. 动画效果
- 平滑的过渡动画
- 加载指示器
- 悬停效果
- 流式渲染动画

## 使用方法

### 基础使用
```tsx
import './chat-core/styles/index.css';

// 在你的组件中使用样式类
<div className="chat-container">
  <div className="chat-header">
    <h4>AI 助手</h4>
  </div>
  <div className="chat-history">
    {/* 消息内容 */}
  </div>
  <div className="chat-input">
    {/* 输入区域 */}
  </div>
</div>
```

### 思维链使用
```tsx
<div className="thinking-section">
  <div className="thinking-header">
    🤔 思考过程
  </div>
  <div className="thinking-content">
    <p>让我分析一下这个问题...</p>
  </div>
</div>
```

### RAG来源显示
```tsx
<div className="rag-sources">
  <div className="rag-sources-header">
    📚 参考来源
  </div>
  <div className="rag-source-item">
    <div className="rag-source-header">
      <span className="rag-source-title">文档标题</span>
      <span className="rag-source-score">0.95</span>
    </div>
    <div className="rag-source-content">
      相关内容摘要...
    </div>
  </div>
</div>
```

### 工具类使用
```tsx
<div className="chat-flex chat-items-center chat-gap-md">
  <button className="chat-btn chat-btn-primary">
    发送消息
  </button>
  <span className="chat-badge chat-badge-success">
    在线
  </span>
</div>
```

## CSS变量定制

你可以通过重写CSS变量来定制主题：

```css
:root {
  --chat-primary: #your-color;
  --chat-bg: #your-bg-color;
  --chat-text: #your-text-color;
  /* 更多变量见 index.css */
}
```

## 注意事项

1. **样式隔离**：所有样式都使用了明确的类名前缀，避免与其他样式冲突
2. **性能优化**：使用了CSS变量和工具类，减少重复代码
3. **兼容性**：支持现代浏览器，包括Chrome、Firefox、Safari、Edge
4. **维护性**：样式按功能分类，便于维护和扩展

## 更新日志

- **v1.0.0** - 初始版本，从现有CSS文件中提取和整理聊天相关样式
- 包含完整的聊天界面、Markdown渲染、思维链显示功能
- 支持响应式设计和深色主题
- 提供丰富的工具类和组件样式

## 贡献指南

如果需要添加新的聊天相关样式：

1. 确定样式的功能分类（聊天界面 vs Markdown渲染）
2. 添加到对应的CSS文件中
3. 遵循现有的命名规范和代码风格
4. 确保添加响应式和深色主题支持
5. 更新本README文档