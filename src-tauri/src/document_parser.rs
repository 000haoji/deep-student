/**
 * 文档解析核心模块
 * 
 * 提供统一的文档文本提取功能，支持DOCX和PDF格式
 * 支持文件路径、字节流和Base64编码三种输入方式
 */

use std::io::Cursor;
use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};
use base64::{Engine, engine::general_purpose};

/// 文档文件大小限制 (100MB)
const MAX_DOCUMENT_SIZE: usize = 100 * 1024 * 1024;

/// 流式处理时的缓冲区大小 (1MB)
const BUFFER_SIZE: usize = 1024 * 1024;

/// 文档解析错误枚举
#[derive(Debug, Serialize, Deserialize)]
pub enum ParsingError {
    /// 文件不存在或无法访问
    FileNotFound(String),
    /// IO错误
    IoError(String),
    /// 不支持的文件格式
    UnsupportedFormat(String),
    /// DOCX解析错误
    DocxParsingError(String),
    /// PDF解析错误
    PdfParsingError(String),
    /// Base64解码错误
    Base64DecodingError(String),
    /// 文件过大错误
    FileTooLarge(String),
    /// 其他错误
    Other(String),
}

impl std::fmt::Display for ParsingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParsingError::FileNotFound(msg) => write!(f, "文件未找到: {}", msg),
            ParsingError::IoError(msg) => write!(f, "IO错误: {}", msg),
            ParsingError::UnsupportedFormat(msg) => write!(f, "不支持的文件格式: {}", msg),
            ParsingError::DocxParsingError(msg) => write!(f, "DOCX解析错误: {}", msg),
            ParsingError::PdfParsingError(msg) => write!(f, "PDF解析错误: {}", msg),
            ParsingError::Base64DecodingError(msg) => write!(f, "Base64解码错误: {}", msg),
            ParsingError::FileTooLarge(msg) => write!(f, "文件过大: {}", msg),
            ParsingError::Other(msg) => write!(f, "其他错误: {}", msg),
        }
    }
}

impl std::error::Error for ParsingError {}

/// 从IO错误转换
impl From<std::io::Error> for ParsingError {
    fn from(error: std::io::Error) -> Self {
        ParsingError::IoError(error.to_string())
    }
}

/// 从Base64解码错误转换
impl From<base64::DecodeError> for ParsingError {
    fn from(error: base64::DecodeError) -> Self {
        ParsingError::Base64DecodingError(error.to_string())
    }
}

/// 文档解析器结构体
pub struct DocumentParser;

impl DocumentParser {
    /// 创建新的文档解析器实例
    pub fn new() -> Self {
        DocumentParser
    }

    /// 检查文件大小是否超出限制
    fn check_file_size(&self, size: usize) -> Result<(), ParsingError> {
        if size > MAX_DOCUMENT_SIZE {
            return Err(ParsingError::FileTooLarge(
                format!("文件大小 {}MB 超过限制 {}MB", 
                    size / (1024 * 1024), 
                    MAX_DOCUMENT_SIZE / (1024 * 1024))
            ));
        }
        Ok(())
    }

    /// 安全地检查并读取文件
    fn read_file_safely(&self, file_path: &str) -> Result<Vec<u8>, ParsingError> {
        let metadata = fs::metadata(file_path)?;
        let file_size = metadata.len() as usize;
        
        self.check_file_size(file_size)?;
        
        let bytes = fs::read(file_path)?;
        Ok(bytes)
    }

    /// 从文件路径提取文本
    pub fn extract_text_from_path(&self, file_path: &str) -> Result<String, ParsingError> {
        let path = Path::new(file_path);
        
        // 检查文件是否存在
        if !path.exists() {
            return Err(ParsingError::FileNotFound(file_path.to_string()));
        }

        // 根据文件扩展名确定处理方式
        let extension = path.extension()
            .and_then(|ext| ext.to_str())
            .ok_or_else(|| ParsingError::UnsupportedFormat("无法确定文件扩展名".to_string()))?
            .to_lowercase();

        match extension.as_str() {
            "docx" => self.extract_docx_from_path(file_path),
            "pdf" => self.extract_pdf_from_path(file_path),
            "txt" => self.extract_txt_from_path(file_path),
            "md" => self.extract_md_from_path(file_path),
            _ => Err(ParsingError::UnsupportedFormat(format!("不支持的文件格式: .{}", extension))),
        }
    }

    /// 从字节流提取文本
    pub fn extract_text_from_bytes(&self, file_name: &str, bytes: Vec<u8>) -> Result<String, ParsingError> {
        // 检查文件大小
        self.check_file_size(bytes.len())?;
        
        // 从文件名确定文件类型
        let extension = Path::new(file_name)
            .extension()
            .and_then(|ext| ext.to_str())
            .ok_or_else(|| ParsingError::UnsupportedFormat("无法确定文件扩展名".to_string()))?
            .to_lowercase();

        match extension.as_str() {
            "docx" => self.extract_docx_from_bytes(bytes),
            "pdf" => self.extract_pdf_from_bytes(bytes),
            "txt" => self.extract_txt_from_bytes(bytes),
            "md" => self.extract_md_from_bytes(bytes),
            _ => Err(ParsingError::UnsupportedFormat(format!("不支持的文件格式: .{}", extension))),
        }
    }

    /// 从Base64编码内容提取文本
    pub fn extract_text_from_base64(&self, file_name: &str, base64_content: &str) -> Result<String, ParsingError> {
        // 解码Base64内容
        let bytes = general_purpose::STANDARD.decode(base64_content)?;
        
        // 调用字节流处理方法
        self.extract_text_from_bytes(file_name, bytes)
    }

    /// 从DOCX文件路径提取文本
    fn extract_docx_from_path(&self, file_path: &str) -> Result<String, ParsingError> {
        let bytes = self.read_file_safely(file_path)?;
        self.extract_docx_from_bytes(bytes)
    }

    /// 从DOCX字节流提取文本
    fn extract_docx_from_bytes(&self, bytes: Vec<u8>) -> Result<String, ParsingError> {
        let docx = docx_rs::read_docx(&bytes)
            .map_err(|e| ParsingError::DocxParsingError(e.to_string()))?;
        
        Ok(self.extract_docx_text(&docx))
    }

    /// 从DOCX文档对象提取文本内容（优化版本）
    fn extract_docx_text(&self, docx: &docx_rs::Docx) -> String {
        // 预估容量以减少重新分配
        let mut text_content = String::with_capacity(8192);
        
        // 遍历文档的所有子元素
        for child in &docx.document.children {
            match child {
                docx_rs::DocumentChild::Paragraph(para) => {
                    let mut has_content = false;
                    
                    // 提取段落中的所有文本
                    for child in &para.children {
                        if let docx_rs::ParagraphChild::Run(run) = child {
                            for run_child in &run.children {
                                if let docx_rs::RunChild::Text(text) = run_child {
                                    if !text.text.trim().is_empty() {
                                        text_content.push_str(&text.text);
                                        has_content = true;
                                    }
                                }
                            }
                        }
                    }
                    
                    // 如果段落有内容，添加换行符
                    if has_content {
                        text_content.push('\n');
                    }
                }
                _ => {
                    // 处理其他类型的文档子元素，如表格等
                    // 这里简化处理，只处理段落
                }
            }
        }
        
        text_content.trim().to_string()
    }

    /// 从PDF文件路径提取文本
    fn extract_pdf_from_path(&self, file_path: &str) -> Result<String, ParsingError> {
        // 先检查文件大小
        let metadata = fs::metadata(file_path)?;
        let file_size = metadata.len() as usize;
        self.check_file_size(file_size)?;
        
        let text = pdf_extract::extract_text(file_path)
            .map_err(|e| ParsingError::PdfParsingError(e.to_string()))?;
        
        Ok(text.trim().to_string())
    }

    /// 从PDF字节流提取文本
    fn extract_pdf_from_bytes(&self, bytes: Vec<u8>) -> Result<String, ParsingError> {
        let text = pdf_extract::extract_text_from_mem(&bytes)
            .map_err(|e| ParsingError::PdfParsingError(e.to_string()))?;
        
        Ok(text.trim().to_string())
    }

    /// 从TXT文件路径提取文本
    fn extract_txt_from_path(&self, file_path: &str) -> Result<String, ParsingError> {
        let bytes = self.read_file_safely(file_path)?;
        self.extract_txt_from_bytes(bytes)
    }

    /// 从TXT字节流提取文本
    fn extract_txt_from_bytes(&self, bytes: Vec<u8>) -> Result<String, ParsingError> {
        // 尝试UTF-8解码，先不消费bytes
        match std::str::from_utf8(&bytes) {
            Ok(text) => Ok(text.trim().to_string()),
            Err(_) => {
                // 如果UTF-8失败，使用lossy转换
                let text = String::from_utf8_lossy(&bytes);
                Ok(text.trim().to_string())
            }
        }
    }

    /// 从MD文件路径提取文本
    fn extract_md_from_path(&self, file_path: &str) -> Result<String, ParsingError> {
        let bytes = self.read_file_safely(file_path)?;
        self.extract_md_from_bytes(bytes)
    }

    /// 从MD字节流提取文本
    fn extract_md_from_bytes(&self, bytes: Vec<u8>) -> Result<String, ParsingError> {
        // Markdown文件本质上也是文本文件，使用相同的处理方式
        // 未来可以考虑解析Markdown语法，但目前保持简单
        self.extract_txt_from_bytes(bytes)
    }
}

/// 默认实例化
impl Default for DocumentParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_parser_creation() {
        let parser = DocumentParser::new();
        assert_eq!(std::mem::size_of_val(&parser), 0); // 零大小类型
    }

    #[test]
    fn test_txt_support() {
        let parser = DocumentParser::new();
        let test_content = "Hello, World!".as_bytes().to_vec();
        let result = parser.extract_text_from_bytes("test.txt", test_content);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello, World!");
    }

    #[test]
    fn test_txt_with_unicode() {
        let parser = DocumentParser::new();
        let test_content = "中文测试 English Test 123".as_bytes().to_vec();
        let result = parser.extract_text_from_bytes("test.txt", test_content);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "中文测试 English Test 123");
    }

    #[test]
    fn test_md_support() {
        let parser = DocumentParser::new();
        let test_content = "# 标题\n\n这是**Markdown**内容。".as_bytes().to_vec();
        let result = parser.extract_text_from_bytes("test.md", test_content);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "# 标题\n\n这是**Markdown**内容。");
    }

    #[test]
    fn test_file_too_large() {
        let parser = DocumentParser::new();
        let large_content = vec![0u8; MAX_DOCUMENT_SIZE + 1];
        let result = parser.extract_text_from_bytes("test.txt", large_content);
        assert!(matches!(result, Err(ParsingError::FileTooLarge(_))));
    }

    #[test]
    fn test_base64_decoding_error() {
        let parser = DocumentParser::new();
        let result = parser.extract_text_from_base64("test.docx", "invalid_base64!");
        assert!(matches!(result, Err(ParsingError::Base64DecodingError(_))));
    }
}