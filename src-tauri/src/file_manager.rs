use std::fs;
use std::path::{Path, PathBuf};
use std::io::Write;
use tokio::fs as async_fs;
// use tokio::io::AsyncWriteExt; // Removed unused import
use base64::{Engine as _, engine::general_purpose};
use uuid::Uuid;
use crate::models::AppError;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageStatistics {
    pub total_files: u64,
    pub total_size_bytes: u64,
    pub file_types: HashMap<String, u32>, // extension -> count
    pub oldest_file: Option<u64>,         // timestamp
    pub newest_file: Option<u64>,         // timestamp
}

pub struct FileManager {
    app_data_dir: PathBuf,
    images_dir: PathBuf,
}

impl FileManager {
    /// 创建新的文件管理器
    pub fn new(app_data_dir: PathBuf) -> Result<Self> {
        let images_dir = app_data_dir.join("images");
        
        println!("初始化文件管理器: {:?}", app_data_dir);
        
        Ok(FileManager {
            app_data_dir,
            images_dir,
        })
    }

    /// 获取数据库路径
    pub fn get_database_path(&self) -> PathBuf {
        self.app_data_dir.join("mistakes.db")
    }

    /// 保存base64编码的图片文件
    pub async fn save_image_from_base64(&self, base64_data: &str, filename: &str) -> Result<String> {
        println!("保存图片文件: {}", filename);
        
        // 确保图片目录存在
        async_fs::create_dir_all(&self.images_dir).await
            .map_err(|e| AppError::file_system(format!("创建图片目录失败: {}", e)))?;
        
        // 解析base64数据
        let data_url_prefix = "data:image/";
        let base64_start = if base64_data.starts_with(data_url_prefix) {
            base64_data.find("base64,")
                .ok_or_else(|| AppError::validation("无效的base64数据格式"))?
                + 7 // "base64,".len()
        } else {
            0
        };
        
        let base64_content = &base64_data[base64_start..];
        let image_bytes = general_purpose::STANDARD
            .decode(base64_content)
            .map_err(|e| AppError::validation(format!("base64解码失败: {}", e)))?;
        
        // 保存文件
        let file_path = self.images_dir.join(filename);
        async_fs::write(&file_path, image_bytes).await
            .map_err(|e| AppError::file_system(format!("保存图片文件失败: {}", e)))?;
        
        // 返回相对路径
        Ok(format!("images/{}", filename))
    }

    /// 读取图片文件为base64（用于统一AI接口）
    pub fn read_file_as_base64(&self, relative_path: &str) -> Result<String> {
        println!("读取图片文件为base64: {}", relative_path);
        
        let file_path = self.app_data_dir.join(relative_path);
        if !file_path.exists() {
            return Err(AppError::not_found(format!("图片文件不存在: {}", relative_path)));
        }
        
        let image_bytes = fs::read(&file_path)
            .map_err(|e| AppError::file_system(format!("读取图片文件失败: {}", e)))?;
        
        let base64_content = general_purpose::STANDARD.encode(&image_bytes);
        Ok(base64_content)
    }

    /// 读取图片文件为base64（带MIME类型）
    pub async fn get_image_as_base64(&self, relative_path: &str) -> Result<String> {
        println!("读取图片文件: {}", relative_path);
        
        let file_path = self.app_data_dir.join(relative_path);
        if !async_fs::try_exists(&file_path).await
            .map_err(|e| AppError::file_system(format!("检查文件存在性失败: {}", e)))? {
            return Err(AppError::not_found(format!("图片文件不存在: {}", relative_path)));
        }
        
        let image_bytes = async_fs::read(&file_path).await
            .map_err(|e| AppError::file_system(format!("读取图片文件失败: {}", e)))?;
        
        let base64_content = general_purpose::STANDARD.encode(&image_bytes);
        
        // 根据文件扩展名确定MIME类型
        let mime_type = if relative_path.ends_with(".png") {
            "image/png"
        } else if relative_path.ends_with(".gif") {
            "image/gif"
        } else {
            "image/jpeg"
        };
        
        Ok(format!("data:{};base64,{}", mime_type, base64_content))
    }

    /// 删除图片文件
    pub async fn delete_image(&self, relative_path: &str) -> Result<()> {
        println!("删除图片文件: {}", relative_path);
        
        let file_path = self.app_data_dir.join(relative_path);
        if async_fs::try_exists(&file_path).await
            .map_err(|e| AppError::file_system(format!("检查文件存在性失败: {}", e)))? {
            async_fs::remove_file(&file_path).await
                .map_err(|e| AppError::file_system(format!("删除图片文件失败: {}", e)))?;
        }
        
        Ok(())
    }

    /// 删除多个图片文件
    pub fn delete_images(&self, relative_paths: &[String]) -> Result<()> {
        println!("删除多个图片文件: {} 个", relative_paths.len());
        for path in relative_paths {
            let file_path = self.app_data_dir.join(path);
            if file_path.exists() {
                fs::remove_file(&file_path)
                    .map_err(|e| AppError::file_system(format!("删除图片文件失败: {}", e)))?;
            }
        }
        Ok(())
    }

    /// 清理孤立的图片文件
    pub async fn cleanup_orphaned_images(&self, database: &crate::database::Database) -> Result<Vec<String>> {
        println!("开始清理孤立图片文件");
        
        if !async_fs::try_exists(&self.images_dir).await
            .map_err(|e| AppError::file_system(format!("检查图片目录存在性失败: {}", e)))? {
            println!("图片目录不存在，跳过清理");
            return Ok(vec![]);
        }
        
        let mut cleaned_files = Vec::new();
        
        // 1. 收集所有物理图片文件
        let mut all_physical_files = std::collections::HashSet::new();
        let mut entries = async_fs::read_dir(&self.images_dir).await
            .map_err(|e| AppError::file_system(format!("读取图片目录失败: {}", e)))?;
        
        while let Some(entry) = entries.next_entry().await
            .map_err(|e| AppError::file_system(format!("读取目录条目失败: {}", e)))? {
            
            let path = entry.path();
            if path.is_file() {
                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                    // 构建相对路径（相对于app_data_dir）
                    let relative_path = format!("images/{}", filename);
                    all_physical_files.insert(relative_path);
                }
            }
        }
        
        println!("发现 {} 个物理图片文件", all_physical_files.len());
        
        // 2. 从数据库获取所有被引用的图片路径
        let referenced_images = self.get_referenced_images(database)?;
        println!("数据库中引用了 {} 个图片文件", referenced_images.len());
        
        // 3. 找出孤立的图片文件
        for physical_file in &all_physical_files {
            if !referenced_images.contains(physical_file) {
                println!("发现孤立图片文件: {}", physical_file);
                
                // 删除孤立文件
                let full_path = self.app_data_dir.join(physical_file);
                match async_fs::remove_file(&full_path).await {
                    Ok(()) => {
                        cleaned_files.push(physical_file.clone());
                        println!("已删除孤立图片: {}", physical_file);
                    }
                    Err(e) => {
                        println!("删除孤立图片失败: {} - {}", physical_file, e);
                    }
                }
            }
        }
        
        // 4. 清理空的子目录
        self.cleanup_empty_directories().await?;
        
        println!("清理完成，删除了 {} 个孤立图片文件", cleaned_files.len());
        Ok(cleaned_files)
    }

    /// 从数据库获取所有被引用的图片路径
    fn get_referenced_images(&self, database: &crate::database::Database) -> Result<std::collections::HashSet<String>> {
        use rusqlite::params;
        
        let conn = database.conn().lock().map_err(|_| AppError::database("获取数据库连接失败".to_string()))?;
        let mut referenced_images = std::collections::HashSet::new();
        
        // 查询所有错题的图片路径
        let mut stmt = conn.prepare("SELECT question_images, analysis_images FROM mistakes")
            .map_err(|e| AppError::database(format!("准备查询语句失败: {}", e)))?;
        
        let rows = stmt.query_map(params![], |row| {
            let question_images: String = row.get(0)?;
            let analysis_images: String = row.get(1)?;
            Ok((question_images, analysis_images))
        }).map_err(|e| AppError::database(format!("执行查询失败: {}", e)))?;

        for row_result in rows {
            let (question_images_json, analysis_images_json) = row_result
                .map_err(|e| AppError::database(format!("读取行数据失败: {}", e)))?;
            
            // 解析JSON数组 - 改进错误处理以防止数据丢失
            match serde_json::from_str::<Vec<String>>(&question_images_json) {
                Ok(question_paths) => {
                    for path in question_paths {
                        referenced_images.insert(path);
                    }
                }
                Err(e) => {
                    println!("警告: 解析question_images JSON失败: {} - 数据: {}", e, question_images_json);
                    // 不忽略错误，中止清理过程以防止数据丢失
                    return Err(AppError::validation(format!(
                        "解析错题图片路径JSON失败，中止孤立图片清理以防止数据丢失: {}", e
                    )));
                }
            }
            
            match serde_json::from_str::<Vec<String>>(&analysis_images_json) {
                Ok(analysis_paths) => {
                    for path in analysis_paths {
                        referenced_images.insert(path);
                    }
                }
                Err(e) => {
                    println!("警告: 解析analysis_images JSON失败: {} - 数据: {}", e, analysis_images_json);
                    // 不忽略错误，中止清理过程以防止数据丢失
                    return Err(AppError::validation(format!(
                        "解析分析图片路径JSON失败，中止孤立图片清理以防止数据丢失: {}", e
                    )));
                }
            }
        }
        
        Ok(referenced_images)
    }

    /// 清理空的子目录
    async fn cleanup_empty_directories(&self) -> Result<()> {
        println!("清理空目录");
        
        let mut entries = async_fs::read_dir(&self.images_dir).await
            .map_err(|e| AppError::file_system(format!("读取图片目录失败: {}", e)))?;
        
        let mut directories_to_check = Vec::new();
        
        while let Some(entry) = entries.next_entry().await
            .map_err(|e| AppError::file_system(format!("读取目录条目失败: {}", e)))? {
            
            let path = entry.path();
            if path.is_dir() {
                directories_to_check.push(path);
            }
        }
        
        // 检查并删除空目录
        for dir_path in directories_to_check {
            match self.is_directory_empty(&dir_path).await {
                Ok(true) => {
                    if let Err(e) = async_fs::remove_dir(&dir_path).await {
                        println!("删除空目录失败: {:?} - {}", dir_path, e);
                    } else {
                        println!("已删除空目录: {:?}", dir_path);
                    }
                }
                Ok(false) => {
                    // 目录不为空，跳过
                }
                Err(e) => {
                    println!("检查目录是否为空失败: {:?} - {}", dir_path, e);
                }
            }
        }
        
        Ok(())
    }

    /// 检查目录是否为空
    async fn is_directory_empty(&self, dir_path: &Path) -> Result<bool> {
        let mut entries = async_fs::read_dir(dir_path).await
            .map_err(|e| AppError::file_system(format!("读取目录失败: {}", e)))?;
        
        // 如果能读取到第一个条目，说明目录不为空
        match entries.next_entry().await {
            Ok(Some(_)) => Ok(false), // 有条目，不为空
            Ok(None) => Ok(true),     // 没有条目，为空
            Err(e) => Err(AppError::file_system(format!("检查目录内容失败: {}", e)))
        }
    }

    /// 获取图片文件统计信息
    pub async fn get_image_statistics(&self) -> Result<ImageStatistics> {
        let mut stats = ImageStatistics {
            total_files: 0,
            total_size_bytes: 0,
            file_types: std::collections::HashMap::new(),
            oldest_file: None,
            newest_file: None,
        };
        
        if !async_fs::try_exists(&self.images_dir).await
            .map_err(|e| AppError::file_system(format!("检查图片目录存在性失败: {}", e)))? {
            return Ok(stats);
        }
        
        let mut entries = async_fs::read_dir(&self.images_dir).await
            .map_err(|e| AppError::file_system(format!("读取图片目录失败: {}", e)))?;
        
        while let Some(entry) = entries.next_entry().await
            .map_err(|e| AppError::file_system(format!("读取目录条目失败: {}", e)))? {
            
            let path = entry.path();
            if path.is_file() {
                // 获取文件元数据
                let metadata = async_fs::metadata(&path).await
                    .map_err(|e| AppError::file_system(format!("获取文件元数据失败: {}", e)))?;
                
                stats.total_files += 1;
                stats.total_size_bytes += metadata.len();
                
                // 统计文件类型
                if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
                    *stats.file_types.entry(extension.to_lowercase()).or_insert(0) += 1;
                }
                
                // 获取修改时间
                if let Ok(modified) = metadata.modified() {
                    let modified_timestamp = modified.duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default().as_secs();
                    
                    if stats.oldest_file.is_none() || modified_timestamp < stats.oldest_file.unwrap() {
                        stats.oldest_file = Some(modified_timestamp);
                    }
                    
                    if stats.newest_file.is_none() || modified_timestamp > stats.newest_file.unwrap() {
                        stats.newest_file = Some(modified_timestamp);
                    }
                }
            }
        }
        
        Ok(stats)
    }


    /// 验证图片格式 (基于路径的存根)
    pub fn validate_image_format_from_path_stub(&self, relative_path: &str) -> Result<bool> {
        println!("验证图片格式 (存根): {}", relative_path);
        // TODO: 实现基于文件路径的实际验证逻辑
        Ok(true) 
    }

    // 第一个 extract_extension_from_base64 (占位符) 已被移除，保留下面的实际实现

    /// 保存图片文件（从字节数据）
    pub fn save_image_from_bytes(&self, image_data: &[u8], file_extension: &str) -> Result<String> {
        // 确保图片目录存在
        fs::create_dir_all(&self.images_dir)
            .map_err(|e| AppError::file_system(format!("创建图片目录失败: {}", e)))?;
        
        // 生成唯一文件名
        let file_id = Uuid::new_v4().to_string();
        let filename = format!("{}.{}", file_id, file_extension);
        let file_path = self.images_dir.join(&filename);

        // 写入文件
        let mut file = fs::File::create(&file_path)
            .map_err(|e| AppError::file_system(format!("创建图片文件失败: {}", e)))?;
        file.write_all(image_data)
            .map_err(|e| AppError::file_system(format!("写入图片文件失败: {}", e)))?;

        // 返回相对路径
        Ok(format!("images/{}", filename))
    }

    /// 获取图片文件的绝对路径
    pub fn get_image_absolute_path(&self, relative_path: &str) -> PathBuf {
        self.app_data_dir.join(relative_path)
    }

    /// 检查图片文件是否存在
    pub fn image_exists(&self, relative_path: &str) -> bool {
        let file_path = self.app_data_dir.join(relative_path);
        file_path.exists()
    }

    /// 获取图片文件大小
    pub fn get_image_size(&self, relative_path: &str) -> Result<u64> {
        let file_path = self.app_data_dir.join(relative_path);
        let metadata = fs::metadata(&file_path)
            .map_err(|e| AppError::file_system(format!("获取文件元数据失败: {}", e)))?;
        Ok(metadata.len())
    }

    /// 验证图片格式 (基于Base64)
    pub fn validate_image_format_from_base64(&self, base64_data: &str) -> Result<String> {
        // 解析MIME类型
        let mime_type = self.extract_mime_type_from_base64(base64_data)?;
        
        // 支持的图片格式
        let supported_formats = vec![
            "image/jpeg", "image/jpg", "image/png", "image/gif", 
            "image/webp", "image/bmp", "image/tiff"
        ];
        
        if !supported_formats.contains(&mime_type.as_str()) {
            return Err(AppError::validation(format!(
                "不支持的图片格式: {}，支持的格式: {}", 
                mime_type, 
                supported_formats.join(", ")
            )));
        }
        
        // 尝试解码base64以验证数据完整性
        let base64_start = if base64_data.starts_with("data:") {
            base64_data.find("base64,")
                .ok_or_else(|| AppError::validation("无效的base64数据格式"))?
                + 7
        } else {
            0
        };
        
        let base64_content = &base64_data[base64_start..];
        let image_bytes = general_purpose::STANDARD
            .decode(base64_content)
            .map_err(|e| AppError::validation(format!("base64解码失败，数据可能损坏: {}", e)))?;
        
        // 基本的文件大小检查
        if image_bytes.is_empty() {
            return Err(AppError::validation("图片数据为空"));
        }
        
        if image_bytes.len() > 50 * 1024 * 1024 { // 50MB限制
            return Err(AppError::validation("图片文件过大，超过50MB限制"));
        }
        
        println!("图片格式验证通过: {} ({} bytes)", mime_type, image_bytes.len());
        Ok(mime_type)
    }

    /// 从base64数据中提取文件扩展名
    pub fn extract_extension_from_base64(&self, base64_data: &str) -> Result<String> {
        let mime_type = self.extract_mime_type_from_base64(base64_data)?;
        
        let extension = match mime_type.as_str() {
            "image/jpeg" | "image/jpg" => "jpg",
            "image/png" => "png",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "image/bmp" => "bmp",
            "image/tiff" => "tiff",
            _ => return Err(AppError::validation(format!("无法确定文件扩展名，未知MIME类型: {}", mime_type)))
        };
        
        Ok(extension.to_string())
    }

    /// 从base64 Data URL中提取MIME类型
    fn extract_mime_type_from_base64(&self, base64_data: &str) -> Result<String> {
        if base64_data.starts_with("data:") {
            if let Some(semicolon_pos) = base64_data.find(';') {
                let mime_type = &base64_data[5..semicolon_pos]; // 跳过 "data:"
                if mime_type.starts_with("image/") {
                    return Ok(mime_type.to_string());
                }
            }
            return Err(AppError::validation("无效的Data URL格式"));
        } else {
            // 如果不是Data URL，尝试从文件头部识别
            self.detect_image_type_from_content(base64_data)
        }
    }

    /// 从文件内容检测图片类型
    fn detect_image_type_from_content(&self, base64_data: &str) -> Result<String> {
        let image_bytes = general_purpose::STANDARD
            .decode(base64_data)
            .map_err(|e| AppError::validation(format!("base64解码失败: {}", e)))?;
        
        if image_bytes.len() < 8 {
            return Err(AppError::validation("图片数据太短，无法识别格式"));
        }
        
        // 检查文件头部魔术字节
        match &image_bytes[0..8] {
            [0xFF, 0xD8, 0xFF, ..] => Ok("image/jpeg".to_string()),
            [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] => Ok("image/png".to_string()),
            [0x47, 0x49, 0x46, 0x38, 0x37, 0x61, ..] | [0x47, 0x49, 0x46, 0x38, 0x39, 0x61, ..] => Ok("image/gif".to_string()),
            [0x52, 0x49, 0x46, 0x46, _, _, _, _] if &image_bytes[8..12] == b"WEBP" => Ok("image/webp".to_string()),
            [0x42, 0x4D, ..] => Ok("image/bmp".to_string()),
            _ => Err(AppError::validation("无法识别图片格式"))
        }
    }

    /// 创建完整的系统备份
    pub fn create_backup(&self) -> Result<PathBuf> {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let backup_dir = self.app_data_dir.join("backups");
        fs::create_dir_all(&backup_dir)
            .map_err(|e| AppError::file_system(format!("创建备份目录失败: {}", e)))?;
        
        let backup_file = backup_dir.join(format!("backup_{}.zip", timestamp));
        
        println!("创建系统备份: {:?}", backup_file);
        
        // 实现真正的备份逻辑
        use std::io::Write;
        let file = fs::File::create(&backup_file)
            .map_err(|e| AppError::file_system(format!("创建备份文件失败: {}", e)))?;
        
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o755);
        
        // 备份数据库文件
        let db_path = self.get_database_path();
        if db_path.exists() {
            zip.start_file("mistakes.db", options)?;
            let db_content = fs::read(&db_path)
                .map_err(|e| AppError::file_system(format!("读取数据库文件失败: {}", e)))?;
            zip.write_all(&db_content)?;
            println!("已备份数据库文件: {} bytes", db_content.len());
        }
        
        // 备份图片目录
        if self.images_dir.exists() {
            self.add_directory_to_zip(&mut zip, &self.images_dir, "images", options)?;
        }
        
        // 备份配置文件（如果存在）
        let config_file = self.app_data_dir.join("config.json");
        if config_file.exists() {
            zip.start_file("config.json", options)?;
            let config_content = fs::read(&config_file)
                .map_err(|e| AppError::file_system(format!("读取配置文件失败: {}", e)))?;
            zip.write_all(&config_content)?;
            println!("已备份配置文件: {} bytes", config_content.len());
        }
        
        zip.finish().map_err(|e| AppError::file_system(format!("完成备份文件失败: {}", e)))?;
        
        let backup_size = fs::metadata(&backup_file)
            .map_err(|e| AppError::file_system(format!("获取备份文件大小失败: {}", e)))?
            .len();
        
        println!("备份创建成功: {:?} ({} bytes)", backup_file, backup_size);
        Ok(backup_file)
    }

    /// 将目录添加到ZIP文件中
    fn add_directory_to_zip<W: Write + std::io::Seek>(
        &self, 
        zip: &mut zip::ZipWriter<W>, 
        dir_path: &Path, 
        prefix: &str,
        options: zip::write::FileOptions
    ) -> Result<()> {
        let entries = fs::read_dir(dir_path)
            .map_err(|e| AppError::file_system(format!("读取目录失败: {}", e)))?;
        
        for entry in entries {
            let entry = entry.map_err(|e| AppError::file_system(format!("读取目录条目失败: {}", e)))?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string(); // Convert Cow<str> to String
            let zip_path = format!("{}/{}", prefix, name);
            
            if path.is_file() {
                zip.start_file(&zip_path, options)?;
                let content = fs::read(&path)
                    .map_err(|e| AppError::file_system(format!("读取文件失败: {}", e)))?;
                zip.write_all(&content)?;
                println!("已添加文件到备份: {} ({} bytes)", zip_path, content.len());
            } else if path.is_dir() {
                // 递归处理子目录
                self.add_directory_to_zip(zip, &path, &zip_path, options)?;
            }
        }
        
        Ok(())
    }

    /// 获取应用数据目录路径
    pub fn get_app_data_dir(&self) -> &Path {
        &self.app_data_dir
    }
}
