# OCR 适配器模块文档

## 概述

OCR 适配器模块提供了可扩展的多 OCR 引擎支持架构，允许用户在不同的 OCR 引擎之间切换。

## 支持的 OCR 引擎

### 1. DeepSeek-OCR（默认）

- **模型名称**：`deepseek-ai/DeepSeek-OCR`
- **特点**：
  - 支持 Grounding 坐标输出
  - 专业 OCR 模型，适合题目集识别
  - 使用特殊的 `<|grounding|>` prompt 格式
- **输出格式**：
  ```
  <|ref|>标签文本<|/ref|><|det|>[[x1,y1,x2,y2]]<|/det|>
  ```
- **坐标系统**：0-999 归一化 xyxy 坐标

### 2. PaddleOCR-VL-1.5

- **模型名称**：`PaddlePaddle/PaddleOCR-VL-1.5`
- **版本**：1.5（2026-01-29 发布）
- **特点**：
  - 百度开源 OCR 视觉语言模型
  - **完全免费**
  - 支持 109 种语言
  - 精度达 94.5%（OmniDocBench v1.5）
  - 可输出结构化结果或纯 Markdown
  - 1.5 新增：异形框定位、印章识别、文本检测识别
- **硅基流动调用**：通过 OpenAI 兼容 API 调用
- **Prompt 格式**（简单任务前缀）：
  - `OCR:` - 通用文本识别
  - `Table Recognition:` - 表格识别
  - `Formula Recognition:` - 公式识别
  - `Chart Recognition:` - 图表解析
  - `Seal Recognition:` - 印章识别（1.5 新增）
  - `Text Spotting:` - 文本检测识别（1.5 新增）
  - **Grounding 结构化输出**：在 `OCR:` 后追加 JSON 输出要求（blocks + bbox）

### 3. 通用多模态模型

- **推荐模型**：`Qwen/Qwen2.5-VL-7B-Instruct`
- **特点**：
  - 使用标准 VLM 进行 OCR
  - 不支持 Grounding 坐标输出
  - 适合简单文档识别

## 架构设计

```
┌─────────────────────────────────────────────────────────────┐
│                     OcrAdapter trait                         │
├─────────────────────────────────────────────────────────────┤
│  + build_prompt(&self, mode) -> String                      │
│  + parse_response(&self, resp, img_size) -> OcrPageResult   │
│  + engine_type(&self) -> OcrEngineType                      │
│  + supports_mode(&self, mode) -> bool                       │
└─────────────────────────────────────────────────────────────┘
           ▲                              ▲
           │                              │
┌──────────┴──────────┐      ┌───────────┴───────────┐
│  DeepSeekOcrAdapter │      │  PaddleOcrVlAdapter   │
├─────────────────────┤      ├─────────────────────────┤
│ • grounding prompt  │      │ • standard prompt       │
│ • 0-999 坐标解析    │      │ • 像素坐标/Markdown    │
│ • ref/det 标记解析  │      │ • 版面检测+识别        │
└─────────────────────┘      └─────────────────────────┘
```

## 文件结构

```
src-tauri/src/ocr_adapters/
├── mod.rs          # 模块定义和 OcrAdapter trait
├── types.rs        # 公共类型定义
├── deepseek.rs     # DeepSeek-OCR 适配器
├── paddle.rs       # PaddleOCR-VL 适配器
└── factory.rs      # 适配器工厂
```

## 配置方式

### UI 配置入口

OCR 引擎配置已整合到「设置」→「模型分配」页面中的「OCR 识别引擎」卡片，支持：
- 查看已配置的 OCR 模型列表
- 切换当前使用的 OCR 引擎
- 引擎对比测试

「OCR 识别设置」（应用设置中）仅保留 OCR 策略配置：
- 启用/禁用自动 OCR
- 多模态模型跳过 OCR
- PDF 文本阈值
- 图片/扫描 PDF 识别设置

### 数据库配置

**引擎类型**存储在 settings 表中：

```
key: ocr.engine_type
value: deepseek_ocr | paddle_ocr_vl | generic_vlm
```

**已配置的 OCR 模型列表**（支持多引擎）：

```
key: ocr.available_models
value: JSON 数组
[
  {
    "configId": "模型配置ID",
    "model": "deepseek-ai/DeepSeek-OCR",
    "engineType": "deepseek_ocr",
    "name": "SiliconFlow - DeepSeek-OCR",
    "isFree": false
  },
  {
    "configId": "模型配置ID",
    "model": "PaddlePaddle/PaddleOCR-VL-1.5",
    "engineType": "paddle_ocr_vl",
    "name": "SiliconFlow - PaddleOCR-VL-1.5",
    "isFree": true
  }
]
```

### 硅基流动一键分配

使用「硅基流动一键分配」功能时，会自动配置以下 OCR 模型：

| 模型 | 引擎类型 | 是否免费 |
|------|----------|----------|
| `deepseek-ai/DeepSeek-OCR` | `deepseek_ocr` | 否 |
| `PaddlePaddle/PaddleOCR-VL-1.5` | `paddle_ocr_vl` | 是 |

分配完成后，用户可在「OCR 识别设置」中选择使用哪个引擎。

### 前端设置

在「设置 → OCR 识别设置」中可以选择 OCR 引擎：

- 显示引擎名称、描述、推荐模型
- 标注免费引擎和支持坐标定位的引擎
- 一键切换，立即生效

## API 参考

### Tauri 命令

| 命令 | 描述 |
|------|------|
| `get_ocr_engines` | 获取所有可用的 OCR 引擎列表 |
| `get_ocr_engine_type` | 获取当前配置的 OCR 引擎类型 |
| `set_ocr_engine_type` | 设置 OCR 引擎类型 |
| `infer_ocr_engine_from_model` | 根据模型名称推断引擎类型 |
| `validate_ocr_model` | 验证模型是否适合指定引擎 |
| `get_ocr_prompt_template` | 获取引擎的 prompt 模板 |

### Rust API

```rust
// 获取适配器
let adapter = OcrAdapterFactory::create(OcrEngineType::DeepSeekOcr);

// 构建 prompt
let prompt = adapter.build_prompt(OcrMode::Grounding);

// 解析响应
let result = adapter.parse_response(
    &response_text,
    image_width,
    image_height,
    page_index,
    image_path,
    OcrMode::Grounding,
)?;
```

## 扩展新引擎

要添加新的 OCR 引擎支持：

1. 在 `types.rs` 中添加枚举值：
   ```rust
   pub enum OcrEngineType {
       DeepSeekOcr,
       PaddleOcrVl,
       GenericVlm,
       NewEngine,  // 新增
   }
   ```

2. 创建适配器实现：
   ```rust
   // src/ocr_adapters/new_engine.rs
   pub struct NewEngineAdapter;
   
   impl OcrAdapter for NewEngineAdapter {
       fn engine_type(&self) -> OcrEngineType { ... }
       fn supports_mode(&self, mode: OcrMode) -> bool { ... }
       fn build_prompt(&self, mode: OcrMode) -> String { ... }
       fn parse_response(...) -> Result<OcrPageResult, OcrError> { ... }
   }
   ```

3. 在 `factory.rs` 中注册：
   ```rust
   OcrEngineType::NewEngine => Arc::new(NewEngineAdapter::new()),
   ```

4. 更新 `engine_info_list()` 添加 UI 信息

## 注意事项

1. **引擎生效范围**：题目集 OCR、PDF OCR、图片 OCR、Chat V2 分析模式 OCR 均遵循 `ocr.engine_type`
2. **题目集识别**：使用 Grounding 模式，需要支持坐标输出的引擎
3. **Free OCR 模式**：用于翻译、多模态索引等场景，所有引擎都支持
4. **模型配置**：OCR 专用模型配置在「模型分配 → 题目集 OCR 专用模型」中设置
5. **向后兼容**：默认使用 DeepSeek-OCR，与现有行为一致

## 引擎对比测试

系统提供了 OCR 引擎对比测试功能，可以：

1. 上传一张图片
2. 同时调用所有已配置的 OCR 引擎
3. 对比各引擎的：
   - **速度**：请求耗时（毫秒）
   - **文本量**：识别出的字符数
   - **坐标定位**：识别出的定位区域数量

### 使用方法

1. 进入「设置」→「应用」→「OCR 识别设置」
2. 确保已配置至少 2 个 OCR 引擎
3. 点击「引擎对比测试」按钮
4. 上传测试图片
5. 点击「开始测试」

### 后端命令

```typescript
// 测试指定引擎的 OCR 能力
await invoke('test_ocr_engine', {
  request: {
    imageBase64: 'data:image/png;base64,...',
    engineType: 'deepseek_ocr' | 'paddle_ocr_vl' | 'generic_vlm'
  }
});
```

响应结构：

```typescript
interface OcrTestResponse {
  engineType: string;      // 引擎类型
  engineName: string;      // 引擎名称
  text: string;            // 识别文本
  regions: OcrTestRegion[]; // 识别区域
  elapsedMs: number;       // 耗时（毫秒）
  success: boolean;        // 是否成功
  error: string | null;    // 错误信息
}
```

## 当前限制

1. **PaddleOCR-VL-1.5 的坐标输出**：目前 PaddleOCR-VL-1.5 的 JSON 坐标格式与 DeepSeek-OCR 不同，题目集识别时会降级为纯文本模式（整页作为一个区域）。后续可通过 prompt 优化或解析逻辑增强来支持精确坐标。

2. **通用 VLM 的 Grounding 模式**：通用 VLM 不支持坐标输出，Grounding 模式会自动降级为纯文本模式。

3. **预留类型**：`OcrRequest`, `OcrAdapterConfig`, `ImageSource` 等类型已定义但未使用，预留给未来的统一 OCR 服务接口。

## 升级说明

### 从 PaddleOCR-VL 升级到 1.5 版本

硅基流动平台于 2026-01-29 上线了 PaddleOCR-VL 1.5 版本，模型名称从 `PaddlePaddle/PaddleOCR-VL` 变更为 `PaddlePaddle/PaddleOCR-VL-1.5`。

**自动迁移**：系统会自动检测并迁移旧版本配置：
- 当读取 OCR 模型配置时，如果发现旧模型名称 `PaddlePaddle/PaddleOCR-VL`，会自动更新为 `PaddlePaddle/PaddleOCR-VL-1.5`
- 迁移后的配置会自动保存，无需用户手动操作
- 控制台会输出 `[OCR] 已自动迁移 PaddleOCR-VL 配置到 1.5 版本` 日志

**手动迁移**：如果自动迁移不生效，可以：
1. 进入「设置」→「模型分配」
2. 使用「硅基流动一键分配」重新配置 OCR 模型
3. 或手动修改模型配置中的模型名称

## 更新日志

- **2026-01-27**：初始版本，支持 DeepSeek-OCR、PaddleOCR-VL、通用 VLM
- **2026-02-02**：升级 PaddleOCR-VL 到 1.5 版本（`PaddlePaddle/PaddleOCR-VL-1.5`），添加自动迁移逻辑
