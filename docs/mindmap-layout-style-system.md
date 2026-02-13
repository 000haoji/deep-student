# 思维导图布局与样式配置化系统

## 概述

本系统实现了思维导图的布局和样式完全配置化/注册化，支持灵活扩展和切换不同的布局+样式组合。

## 系统架构

```
┌─────────────────────────────────────────────────────────────┐
│                    思维导图渲染系统                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐     │
│  │ 布局注册表  │ ×  │ 样式注册表  │ =  │ 预设组合    │     │
│  │ LayoutReg  │    │ StyleReg   │    │ PresetReg   │     │
│  └─────────────┘    └─────────────┘    └─────────────┘     │
│         │                 │                  │              │
│         ▼                 ▼                  ▼              │
│  ┌─────────────────────────────────────────────────────────┐       │
│  │              MindMapRenderer                     │       │
│  │   (根据配置动态选择布局算法 + 样式组件)           │       │
│  └─────────────────────────────────────────────────────────┘       │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## 目录结构

```
src/components/mindmap/
├── registry/                    # 注册系统
│   ├── types.ts                # 类型定义
│   ├── LayoutRegistry.ts       # 布局注册表
│   ├── StyleRegistry.ts        # 样式注册表
│   ├── PresetRegistry.ts       # 预设注册表
│   ├── ComponentRegistry.ts    # 组件注册表
│   └── index.ts
│
├── layouts/                     # 布局算法
│   ├── base/LayoutEngine.ts    # 基类
│   ├── mindmap/                # 思维导图类
│   │   ├── TreeLayoutEngine.ts
│   │   └── BalancedLayoutEngine.ts
│   ├── logic/                  # 逻辑图类
│   │   ├── LogicTreeLayoutEngine.ts
│   │   └── LogicBalancedLayoutEngine.ts
│   ├── orgchart/               # 组织结构图类
│   │   ├── VerticalOrgChartEngine.ts
│   │   └── HorizontalOrgChartEngine.ts
│   └── index.ts
│
├── styles/                      # 样式主题
│   ├── themes/
│   │   ├── default.ts
│   │   ├── dark.ts
│   │   ├── minimal.ts
│   │   └── colorful.ts
│   └── index.ts
│
├── presets/                     # 预设配置
│   ├── mindmap-presets.ts
│   ├── logic-presets.ts
│   ├── orgchart-presets.ts
│   └── index.ts
│
└── init.ts                      # 初始化入口
```

## 布局类型

### 思维导图 (mindmap)

| 布局 ID | 名称 | 支持方向 | 边类型 |
|---------|------|----------|--------|
| tree | 树形图 | right, left | bezier |
| balanced | 平衡图 | both | bezier |

### 逻辑图 (logic)

| 布局 ID | 名称 | 支持方向 | 边类型 |
|---------|------|----------|--------|
| logic-tree | 逻辑图 | right, left | orthogonal |
| logic-balanced | 逻辑图(平衡) | both | orthogonal |

### 组织结构图 (orgchart)

| 布局 ID | 名称 | 支持方向 | 边类型 |
|---------|------|----------|--------|
| orgchart-vertical | 组织结构图 | down, up | step |
| orgchart-horizontal | 水平组织结构图 | right, left | step |

## 样式主题

| 主题 ID | 名称 | 特点 |
|---------|------|------|
| default | 默认 | 白底黑字，清晰简洁 |
| dark | 深色 | 深色背景，适合夜间 |
| minimal | 极简 | 黑白极简风格 |
| colorful | 彩色 | 渐变色彩，视觉丰富 |

## 使用方式

### 1. 初始化模块

```typescript
import { initMindMapModule } from '@/components/mindmap/init';

// 在应用启动时调用
initMindMapModule();
```

### 2. 使用 Store 切换布局

```typescript
import { useMindMapStore } from '@/components/mindmap/store';

const MyComponent = () => {
  const applyPreset = useMindMapStore(s => s.applyPreset);
  const setLayoutId = useMindMapStore(s => s.setLayoutId);
  const setLayoutDirection = useMindMapStore(s => s.setLayoutDirection);
  
  // 应用预设
  applyPreset('mindmap-balanced');
  
  // 或单独设置
  setLayoutId('logic-tree');
  setLayoutDirection('left');
};
```

### 3. 使用结构选择器 UI

```tsx
import { StructureSelector } from '@/components/mindmap/components/mindmap';

<StructureSelector className="absolute top-4 right-4" />
```

### 4. 注册自定义布局

```typescript
import { LayoutRegistry } from '@/components/mindmap/registry';
import { BaseLayoutEngine } from '@/components/mindmap/layouts/base';

class MyCustomLayout extends BaseLayoutEngine {
  id = 'my-custom';
  name = '我的布局';
  // ... 实现
}

LayoutRegistry.register(new MyCustomLayout());
```

### 5. 注册自定义主题

```typescript
import { StyleRegistry } from '@/components/mindmap/registry';

StyleRegistry.register({
  id: 'my-theme',
  name: '我的主题',
  node: { /* ... */ },
  edge: { /* ... */ },
  canvas: { /* ... */ },
});
```

## 扩展指南

### 添加新布局

1. 在 `layouts/` 下创建新的布局引擎类，继承 `BaseLayoutEngine`
2. 实现 `calculate` 方法
3. 在 `layouts/index.ts` 中注册

### 添加新边类型

1. 在 `components/mindmap/edges/` 下创建新的边组件
2. 在 `edges/index.ts` 中导出并添加到 `edgeTypes`
3. 在预设中使用新的边类型

### 添加新主题

1. 在 `styles/themes/` 下创建主题配置
2. 在 `styles/themes/index.ts` 中导出
3. 添加到 `builtinThemes` 数组

## API 参考

### LayoutRegistry

- `register(layout: ILayoutEngine)`: 注册布局引擎
- `get(id: string)`: 获取布局引擎
- `getAll()`: 获取所有布局
- `getByCategory(category)`: 按分类获取布局
- `getDefault()`: 获取默认布局

### StyleRegistry

- `register(style: IStyleTheme)`: 注册样式主题
- `get(id: string)`: 获取主题
- `getAll()`: 获取所有主题
- `getDefault()`: 获取默认主题

### PresetRegistry

- `register(preset: IPreset)`: 注册预设
- `get(id: string)`: 获取预设
- `getByCategory(category)`: 按分类获取预设
- `getDefault()`: 获取默认预设

## 更新日志

- **v1.0.0**: 初始版本，实现布局和样式配置化系统
  - 支持 3 大类布局：思维导图、逻辑图、组织结构图
  - 支持 4 种边类型：贝塞尔、直线、直角折线、阶梯
  - 支持 4 种主题：默认、深色、极简、彩色
  - 提供结构选择器 UI 组件
