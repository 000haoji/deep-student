# 开发与发布流程

## 日常开发

### 1. 创建 feature 分支

```bash
git checkout main
git pull origin main
git checkout -b feat/my-feature
```

### 2. 提交代码（使用 Conventional Commits 格式）

```bash
git commit -m "feat: 新增 XX 功能"
git commit -m "fix: 修复 XX 问题"
git commit -m "chore: 更新依赖"
git commit -m "docs: 更新文档"
```

> **为什么要用这个格式？** release-please 根据 commit message 自动判断版本号：
> - `feat:` → 自动 bump 次版本号（0.9.2 → 0.10.0）
> - `fix:` → 自动 bump 补丁版本号（0.9.2 → 0.9.3）
> - `feat!:` 或 `BREAKING CHANGE:` → 自动 bump 主版本号（0.9.2 → 1.0.0）

### 3. 推送并创建 PR

```bash
git push origin feat/my-feature
```

去 GitHub 创建 Pull Request → CI 自动运行 → 通过后 Squash Merge 到 main。

---

## 发布新版本（自动流程）

你不需要手动改版本号或打 tag，release-please 全自动处理：

```
push to main → release-please 检查 commit
    ↓
有 feat/fix commit → 自动创建 "Release PR"
    ↓（内容：bump 版本号 + 更新 CHANGELOG.md）
你 review 后 merge Release PR
    ↓
自动构建 macOS + Windows + Android
    ↓
产物上传到 GitHub Releases + 生成 latest.json
```

**你只需要做一件事：merge Release PR。**

---

## 手动发布（备用方式）

如果你想跳过 release-please，直接发版：

```bash
# 1. 改版本号
#    - package.json
#    - src-tauri/tauri.conf.json
# 2. 提交
git add -A && git commit -m "chore: release v0.9.3"
# 3. 打 tag 并推送
git tag v0.9.3
git push origin main --tags
```

---

## 紧急热修复

```bash
# 1. 从 main 拉分支
git checkout main && git pull
git checkout -b hotfix/critical-bug

# 2. 修复并提交
git commit -m "fix: 紧急修复 XX"

# 3. 创建 PR → merge
# 4. release-please 会自动创建 Release PR
# 5. merge Release PR → 自动构建发布
```

如果情况特别紧急，你作为 admin 可以绕过分支保护直接 push。

---

## Secrets 清单

在 [Settings → Secrets](https://github.com/000haoji/deep-student/settings/secrets/actions) 中需要配置：

| Secret | 用途 |
|--------|------|
| `TAURI_SIGNING_PRIVATE_KEY` | Tauri 自动更新签名私钥 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 私钥密码 |
| `ANDROID_KEYSTORE_BASE64` | Android 签名密钥库（base64） |
| `ANDROID_KEYSTORE_PASSWORD` | 密钥库密码 |
| `ANDROID_KEY_ALIAS` | 密钥别名 |
| `ANDROID_KEY_PASSWORD` | 密钥密码 |

---

## 构建产物说明

每次 Release 自动生成以下文件：

| 文件 | 平台 | 用途 |
|------|------|------|
| `*.aarch64.dmg` | macOS ARM | 安装包 |
| `*.x64.dmg` | macOS Intel | 安装包 |
| `*_aarch64.app.tar.gz` + `.sig` | macOS ARM | 自动更新包 |
| `*_x86_64.app.tar.gz` + `.sig` | macOS Intel | 自动更新包 |
| `*_x64-setup.exe` + `.sig` | Windows | 安装包 + 自动更新 |
| `*_arm64.apk` | Android | 安装包 |
| `latest.json` | 全平台 | 自动更新检查清单 |
