use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Nonce, Key
};
use base64::{Engine as _, engine::general_purpose};
use keyring::Entry;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct EncryptedData {
    pub ciphertext: String,
    pub nonce: String,
}

pub struct CryptoService {
    cipher: Aes256Gcm,
    keyring_service: String,
    keyring_user: String,
}

impl CryptoService {
    /// 创建新的加密服务实例
    pub fn new(app_data_dir: &PathBuf) -> Result<Self, String> {
        let keyring_service = "ai-mistake-manager";
        let keyring_user = "master-key";
        
        // 获取或创建主密钥
        let key = Self::get_or_create_master_key(&keyring_service, &keyring_user, app_data_dir)?;
        let cipher = Aes256Gcm::new(&key);
        
        Ok(CryptoService { 
            cipher,
            keyring_service: keyring_service.to_string(),
            keyring_user: keyring_user.to_string(),
        })
    }
    
    /// 加密API密钥
    pub fn encrypt_api_key(&self, api_key: &str) -> Result<EncryptedData, String> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        
        let ciphertext = self.cipher
            .encrypt(&nonce, api_key.as_bytes())
            .map_err(|e| format!("加密失败: {}", e))?;
        
        Ok(EncryptedData {
            ciphertext: general_purpose::STANDARD.encode(&ciphertext),
            nonce: general_purpose::STANDARD.encode(&nonce),
        })
    }
    
    /// 解密API密钥
    pub fn decrypt_api_key(&self, encrypted_data: &EncryptedData) -> Result<String, String> {
        let ciphertext = general_purpose::STANDARD
            .decode(&encrypted_data.ciphertext)
            .map_err(|e| format!("Base64解码失败: {}", e))?;
        
        let nonce_bytes = general_purpose::STANDARD
            .decode(&encrypted_data.nonce)
            .map_err(|e| format!("Nonce解码失败: {}", e))?;
        
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        let plaintext = self.cipher
            .decrypt(nonce, ciphertext.as_ref())
            .map_err(|e| format!("解密失败: {}", e))?;
        
        String::from_utf8(plaintext)
            .map_err(|e| format!("UTF-8转换失败: {}", e))
    }
    
    /// 获取或创建主密钥（使用操作系统密钥存储）
    fn get_or_create_master_key(service: &str, user: &str, app_data_dir: &PathBuf) -> Result<Key<Aes256Gcm>, String> {
        // 尝试从操作系统密钥存储中获取密钥
        match Entry::new(service, user) {
            Ok(entry) => {
                // 首先尝试从密钥存储中获取现有密钥
                match entry.get_password() {
                    Ok(stored_key) => {
                        // 验证密钥格式和长度
                        if let Ok(key_bytes) = general_purpose::STANDARD.decode(&stored_key) {
                            if key_bytes.len() == 32 {
                                println!("✅ 从操作系统密钥存储中成功加载主密钥");
                                return Ok(*Key::<Aes256Gcm>::from_slice(&key_bytes));
                            }
                        }
                        println!("⚠️ 密钥存储中的密钥格式无效，重新生成新密钥");
                    }
                    Err(_) => {
                        println!("📝 密钥存储中未找到密钥，生成新的主密钥");
                    }
                }
                
                // 生成新的随机主密钥
                let mut key_bytes = [0u8; 32];
                OsRng.fill_bytes(&mut key_bytes);
                
                // 将密钥保存到操作系统密钥存储
                let key_b64 = general_purpose::STANDARD.encode(&key_bytes);
                if let Err(e) = entry.set_password(&key_b64) {
                    println!("⚠️ 无法保存密钥到操作系统密钥存储: {}", e);
                    // 回退到文件存储
                    return Self::get_or_create_file_based_key(app_data_dir);
                }
                
                println!("✅ 新主密钥已生成并保存到操作系统密钥存储");
                Ok(*Key::<Aes256Gcm>::from_slice(&key_bytes))
            }
            Err(e) => {
                println!("⚠️ 无法访问操作系统密钥存储 ({}), 回退到文件存储", e);
                // 回退到基于文件的密钥存储
                Self::get_or_create_file_based_key(app_data_dir)
            }
        }
    }
    
    /// 基于文件的密钥存储（回退方案）
    fn get_or_create_file_based_key(app_data_dir: &PathBuf) -> Result<Key<Aes256Gcm>, String> {
        let key_file_path = app_data_dir.join(".master_key");
        
        // 尝试读取现有密钥文件
        if key_file_path.exists() {
            match std::fs::read_to_string(&key_file_path) {
                Ok(key_content) => {
                    if let Ok(key_bytes) = general_purpose::STANDARD.decode(&key_content) {
                        if key_bytes.len() == 32 {
                            println!("✅ 从文件加载主密钥: {:?}", key_file_path);
                            return Ok(*Key::<Aes256Gcm>::from_slice(&key_bytes));
                        }
                    }
                    println!("⚠️ 密钥文件格式无效，重新生成");
                }
                Err(e) => {
                    println!("⚠️ 无法读取密钥文件: {}", e);
                }
            }
        }
        
        // 生成新的随机主密钥
        let mut key_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut key_bytes);
        
        // 保存到文件（设置适当的权限）
        let key_b64 = general_purpose::STANDARD.encode(&key_bytes);
        std::fs::create_dir_all(app_data_dir)
            .map_err(|e| format!("无法创建应用数据目录: {}", e))?;
        
        std::fs::write(&key_file_path, &key_b64)
            .map_err(|e| format!("无法保存密钥文件: {}", e))?;
        
        // 在Unix系统上设置文件权限为仅所有者可读
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&key_file_path)
                .map_err(|e| format!("无法获取密钥文件元数据: {}", e))?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600); // 仅所有者读写
            std::fs::set_permissions(&key_file_path, permissions)
                .map_err(|e| format!("无法设置密钥文件权限: {}", e))?;
        }
        
        println!("✅ 新主密钥已生成并保存到文件: {:?}", key_file_path);
        Ok(*Key::<Aes256Gcm>::from_slice(&key_bytes))
    }
    
    /// 检查是否为加密数据格式
    pub fn is_encrypted_format(data: &str) -> bool {
        // 简单检查是否为JSON格式且包含必要字段
        if let Ok(parsed) = serde_json::from_str::<EncryptedData>(data) {
            !parsed.ciphertext.is_empty() && !parsed.nonce.is_empty()
        } else {
            false
        }
    }
    
    /// 迁移明文API密钥到加密格式
    pub fn migrate_plaintext_key(&self, plaintext_key: &str) -> Result<String, String> {
        let encrypted_data = self.encrypt_api_key(plaintext_key)?;
        serde_json::to_string(&encrypted_data)
            .map_err(|e| format!("序列化失败: {}", e))
    }
    
    /// 轮换主密钥（高级安全操作）
    pub fn rotate_master_key(&self, app_data_dir: &PathBuf) -> Result<CryptoService, String> {
        println!("🔄 开始轮换主密钥...");
        
        // 删除现有密钥存储
        if let Ok(entry) = Entry::new(&self.keyring_service, &self.keyring_user) {
            let _ = entry.delete_password(); // 忽略删除错误
        }
        
        // 删除文件存储的密钥
        let key_file_path = app_data_dir.join(".master_key");
        if key_file_path.exists() {
            std::fs::remove_file(&key_file_path)
                .map_err(|e| format!("无法删除旧密钥文件: {}", e))?;
        }
        
        // 创建新的加密服务实例（会生成新的主密钥）
        let new_service = CryptoService::new(app_data_dir)?;
        
        println!("✅ 主密钥轮换完成");
        Ok(new_service)
    }
    
    /// 验证密钥完整性
    pub fn verify_key_integrity(&self) -> Result<bool, String> {
        // 通过加密解密测试来验证密钥完整性
        let test_data = "integrity-test-data";
        match self.encrypt_api_key(test_data) {
            Ok(encrypted) => {
                match self.decrypt_api_key(&encrypted) {
                    Ok(decrypted) => Ok(decrypted == test_data),
                    Err(e) => Err(format!("解密测试失败: {}", e)),
                }
            }
            Err(e) => Err(format!("加密测试失败: {}", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    
    #[test]
    fn test_encrypt_decrypt_cycle() {
        let temp_dir = env::temp_dir().join("crypto_test");
        let crypto = CryptoService::new(&temp_dir).unwrap();
        let original = "sk-test-api-key-12345";
        
        let encrypted = crypto.encrypt_api_key(original).unwrap();
        let decrypted = crypto.decrypt_api_key(&encrypted).unwrap();
        
        assert_eq!(original, decrypted);
        
        // 清理测试文件
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    
    #[test]
    fn test_is_encrypted_format() {
        assert!(!CryptoService::is_encrypted_format("plain-text-key"));
        
        let temp_dir = env::temp_dir().join("crypto_test_format");
        let crypto = CryptoService::new(&temp_dir).unwrap();
        let encrypted = crypto.encrypt_api_key("test-key").unwrap();
        let encrypted_json = serde_json::to_string(&encrypted).unwrap();
        
        assert!(CryptoService::is_encrypted_format(&encrypted_json));
        
        // 清理测试文件
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    
    #[test]
    fn test_key_integrity() {
        let temp_dir = env::temp_dir().join("crypto_test_integrity");
        let crypto = CryptoService::new(&temp_dir).unwrap();
        
        assert!(crypto.verify_key_integrity().unwrap());
        
        // 清理测试文件
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}