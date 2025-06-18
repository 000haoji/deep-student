# 归档组件说明

本目录包含被统一组件 `UnifiedAnalysisView` 替换的原有组件备份。

## 归档时间
2024年6月5日

## 归档原因
为了提高代码复用性和可维护性，将三个功能相似的分析组件统一为一个通用组件，并新增了图片追问功能。

## 归档的组件

### 1. MistakeDetail.tsx
- **原用途**: 单条错题的详细分析和追问
- **替换为**: `UnifiedAnalysisView` 组件，使用 `analysisType="singleMistake"`
- **主要功能**: 
  - 错题信息编辑
  - 图片查看
  - AI分析对话
  - 错题删除

### 2. BatchTaskDetailView.tsx  
- **原用途**: 批量分析任务中单个条目的详情展示和追问
- **替换为**: `UnifiedAnalysisView` 组件，使用 `analysisType="batchDetail"`
- **主要功能**:
  - 批量任务状态展示
  - OCR结果显示
  - AI解答对话
  - 保存到错题库

### 3. ReviewAnalysisDetailView.tsx
- **原用途**: 回顾分析会话的详情展示和追问
- **替换为**: `UnifiedAnalysisView` 组件，使用 `analysisType="reviewDetail"`
- **主要功能**:
  - 回顾会话信息展示
  - 自动开始分析
  - AI分析对话
  - 保存到分析库

## 新增功能
统一组件新增了以下功能：
- 图片追问：用户可以上传图片与文字一同发送给AI
- 多模态内容渲染：支持文本和图片混合显示
- 统一的流式处理逻辑
- 更好的错误处理和状态管理

## 恢复说明
原始组件文件已通过git提交保存在版本控制中。如果需要恢复，可以：
1. 使用 `git checkout` 恢复特定版本的文件
2. 恢复相应的导入语句
3. 恢复原有的组件使用方式

**注意**: 为避免编译错误，tsx文件已从此目录移除，但可以通过git历史恢复。

## 兼容性说明
- 新的统一组件完全兼容原有的功能
- API调用方式保持不变
- 数据结构保持向后兼容
- 原有的样式类名大部分保留

## 迁移映射

| 原组件属性 | 统一组件属性 | 备注 |
|----------|------------|------|
| `mistake` | `initialData` | 数据对象 |
| `taskData` | `initialData` | 数据对象 |
| `sessionData` | `initialData` | 数据对象 |
| `onBack` | `onBack` | 仅 singleMistake |
| `onUpdate` | `onMistakeUpdate` | 仅 singleMistake |
| `onDelete` | `onMistakeDelete` | 仅 singleMistake |
| `onSaveRequested` | `onRequestSaveBatchItemAsMistake` 或 `onSaveReviewSession` | 根据类型 |
| `onChatHistoryUpdated` | `onBatchChatUpdate` 或 `onReviewChatUpdate` | 根据类型 |
| `isPageView` | `isPageView` | 保持不变 |

## 注意事项
- 这些组件的备份仅用于紧急恢复，不建议在新开发中使用
- 如有问题，请优先修复统一组件而不是回退到旧组件
- 统一组件的功能会持续迭代和优化