# 文件格式注册表 (File Format Registry)

> **SSOT (Single Source of Truth)** - 本文档是系统支持的所有文件格式的唯一真源。
> 修改格式支持时，请同步更新本文档和所有实现位置。

## 概述

本文档定义了系统支持的所有文件格式，包括 MIME 类型、扩展名映射、预览类型和索引能力。
前后端实现应以本文档为准，确保格式处理的一致性。

## 格式定义

### 图片格式

| 扩展名 | MIME 类型 | 预览类型 | 可索引 | 可注入 | 备注 |
|--------|-----------|----------|--------|--------|------|
| jpg/jpeg | image/jpeg | image | ✅ | ✅ | 常用照片格式 |
| png | image/png | image | ✅ | ✅ | 支持透明通道 |
| gif | image/gif | image | ✅ | ✅ | 支持动画 |
| webp | image/webp | image | ✅ | ✅ | 现代高效格式 |
| bmp | image/bmp | image | ✅ | ✅ | 位图格式 |
| svg | image/svg+xml | image | ❌ | ✅ | 矢量图（仅拖拽支持） |
| heic/heif | image/heic, image/heif | image | ❌ | ✅ | Apple 格式（仅拖拽支持） |

### 文档格式

| 扩展名 | MIME 类型 | 预览类型 | 可索引 | 可注入 | 备注 |
|--------|-----------|----------|--------|--------|------|
| pdf | application/pdf | pdf | ✅ | ✅ | 支持页级预渲染 |
| docx | application/vnd.openxmlformats-officedocument.wordprocessingml.document | docx | ✅ | ✅ | Word 2007+ |
| xlsx | application/vnd.openxmlformats-officedocument.spreadsheetml.sheet | xlsx | ✅ | ✅ | Excel 2007+ |
| xls | application/vnd.ms-excel | xlsx | ✅ | ✅ | 旧版 Excel |
| xlsb | application/vnd.ms-excel.sheet.binary.macroEnabled.12 | xlsx | ✅ | ✅ | 二进制 Excel |
| ods | application/vnd.oasis.opendocument.spreadsheet | xlsx | ✅ | ✅ | OpenDocument 表格 |
| pptx | application/vnd.openxmlformats-officedocument.presentationml.presentation | pptx | ✅ | ✅ | PowerPoint 2007+ |

> 旧版 Office 格式（`.doc`/`.ppt`）暂无可靠解析与预览能力，因此未列入支持列表。

### 文本格式

| 扩展名 | MIME 类型 | 预览类型 | 可索引 | 可注入 | 备注 |
|--------|-----------|----------|--------|--------|------|
| txt | text/plain | text | ✅ | ✅ | 纯文本 |
| md | text/markdown | text | ✅ | ✅ | Markdown |
| html/htm | text/html | text | ✅ | ✅ | HTML 网页 |
| csv | text/csv | text | ✅ | ✅ | 逗号分隔值 |
| json | application/json | text | ✅ | ✅ | JSON 数据 |
| xml | application/xml, text/xml | text | ✅ | ✅ | XML 数据 |

### 电子书格式

| 扩展名 | MIME 类型 | 预览类型 | 可索引 | 可注入 | 备注 |
|--------|-----------|----------|--------|--------|------|
| epub | application/epub+zip | text | ✅ | ✅ | 电子书 |
| rtf | application/rtf, text/rtf | text | ✅ | ✅ | 富文本格式 |

### 压缩格式（仅拖拽支持）

| 扩展名 | MIME 类型 | 预览类型 | 可索引 | 可注入 | 备注 |
|--------|-----------|----------|--------|--------|------|
| zip | application/zip | none | ❌ | ❌ | ZIP 压缩 |
| tar | application/x-tar | none | ❌ | ❌ | TAR 归档 |
| gz | application/gzip | none | ❌ | ❌ | GZIP 压缩 |
| rar | application/x-rar-compressed | none | ❌ | ❌ | RAR 压缩 |
| 7z | application/x-7z-compressed | none | ❌ | ❌ | 7-Zip 压缩 |

## 预览类型说明

| 预览类型 | 描述 | 渲染组件 |
|----------|------|----------|
| image | 图片预览 | ImagePreview |
| pdf | PDF 文档预览（支持页级渲染） | PdfPreview |
| docx | Word 文档预览 | DocxPreview |
| xlsx | Excel 表格预览 | XlsxPreview |
| pptx | PowerPoint 预览 | PptxPreview |
| text | 纯文本预览 | TextPreview |
| none | 不支持预览 | - |

## 大小限制

| 限制项 | 值 | 说明 |
|--------|-----|------|
| 单个附件最大 | 50MB | 与后端 VFS get_max_size_bytes() 一致 |
| 单个图片最大 | 10MB | 推荐限制，实际由 maxFileSize 控制 |
| 最大附件数量 | 20 | 单次会话最大附件数 |
| 小文件阈值 | 1MB | <1MB 使用 inline 模式，>=1MB 使用 external 模式 |

## 实现位置

以下文件需要与本文档保持同步：

### 前端

| 文件 | 用途 |
|------|------|
| `src/chat-v2/core/constants.ts` | 附件上传常量定义（MIME 类型、扩展名列表） |
| `src/hooks/useAttachmentSettings.ts` | 附件设置默认值 |
| `src/components/shared/UnifiedDragDropZone.tsx` | 扩展名到 MIME 类型映射（EXTENSION_TO_MIME） |

### 后端

| 文件 | 用途 |
|------|------|
| `src-tauri/src/vfs/repos/attachment_repo.rs` | MIME 类型到扩展名推断（infer_extension） |
| `src-tauri/src/dstu/handler_utils/node_converters.rs` | 扩展名到预览类型映射（get_textbook_preview_type） |

## 扩展名到 MIME 类型映射表

供前后端实现参考的完整映射表：

```
# 图片格式
jpg     -> image/jpeg
jpeg    -> image/jpeg
png     -> image/png
gif     -> image/gif
bmp     -> image/bmp
webp    -> image/webp
svg     -> image/svg+xml
heic    -> image/heic
heif    -> image/heif

# PDF
pdf     -> application/pdf

# Office 文档
docx    -> application/vnd.openxmlformats-officedocument.wordprocessingml.document
xlsx    -> application/vnd.openxmlformats-officedocument.spreadsheetml.sheet
xls     -> application/vnd.ms-excel
xlsb    -> application/vnd.ms-excel.sheet.binary.macroEnabled.12
ods     -> application/vnd.oasis.opendocument.spreadsheet
pptx    -> application/vnd.openxmlformats-officedocument.presentationml.presentation

# 文本格式
txt     -> text/plain
md      -> text/markdown
csv     -> text/csv
json    -> application/json
xml     -> application/xml (或 text/xml)
html    -> text/html
htm     -> text/html

# 电子书与富文本
epub    -> application/epub+zip
rtf     -> application/rtf (或 text/rtf)

# 压缩格式
zip     -> application/zip
tar     -> application/x-tar
gz      -> application/gzip
rar     -> application/x-rar-compressed
7z      -> application/x-7z-compressed
```

## 变更历史

| 日期 | 变更内容 | 作者 |
|------|----------|------|
| 2026-01-27 | 更新文档格式：移除 doc/ppt，补齐 htm/xlsb/xml 等 | - |
| 2026-01-27 | 创建文档，统一格式定义 | - |
