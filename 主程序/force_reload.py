import config

# 强制重新加载配置
print("强制重新加载config.API_CONFIG...")
config.API_CONFIG = config.load_api_config_from_db()

# 检查配置
if 'api_alternatives' in config.API_CONFIG:
    api_alternatives = config.API_CONFIG['api_alternatives']
    print(f"api_alternatives类型: {type(api_alternatives)}")
    
    if isinstance(api_alternatives, dict) and 'deepseek' in api_alternatives:
        deepseek_type = type(api_alternatives['deepseek'])
        print(f"deepseek替代API类型: {deepseek_type}")
        
        if deepseek_type is list:
            print(f"列表格式的DeepSeek替代API包含 {len(api_alternatives['deepseek'])} 个项目")
            for i, item in enumerate(api_alternatives['deepseek']):
                print(f"DeepSeek替代API项目 {i+1}: {item}")
        elif deepseek_type is dict:
            print(f"字典格式的DeepSeek替代API包含 {len(api_alternatives['deepseek'])} 个键值对")

print("重新加载完成") 