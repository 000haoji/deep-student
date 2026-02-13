# 备份分级与大文件优化

## 背景
- GB 级备份文件存在，云备份需要避免内存峰值与 OOM。
- 备份范围需要可选分级，支持按数据层级精细控制。

## 分级定义
- 核心配置 + 聊天记录  
  `settings.json` / `webview_settings.json` / `config.json` / `slots_state.json` / `databases/chat_v2.db`
- 全部 VFS 内容  
  `databases/vfs.db` / `vfs_blobs/` / `documents/` / `workspaces/`
- 可重建数据  
  `lance/` / `message_queue.db` / `resources.db`
- 大文件资产  
  `images/` / `notes_assets/` / `audio/` / `videos/` / `subjects/`

## 行为说明
- 不选择任何分级：全量备份（保持原有行为）。
- 分级备份启用后会自动关闭精简备份，避免重复过滤。
- 分级备份会跳过 P0 完整性验证，避免误告警；备份元数据会记录分级信息。

## 云备份优化
- WebDAV/S3 上传与下载改为流式处理，避免一次性读入内存。
- S3 大文件（≥100MB）使用 Multipart 上传，失败时自动 abort。
- 上传/下载进度按字节回调，适配大文件展示。

## 推荐组合
- 日常备份：核心配置 + 聊天记录 + 全部 VFS 内容
- 空间紧张：核心配置 + 聊天记录
- 迁移/完整恢复：不选择分级（全量备份）
