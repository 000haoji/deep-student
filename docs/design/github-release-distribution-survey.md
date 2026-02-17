# GitHub Release 国内分发方案调研报告

## 1. 镜像加速服务 (零成本/低成本)

利用第三方或社区提供的镜像服务，直接修改下载链接即可加速。

### 1.1 KGitHub (kgithub.com)
*   **原理**: 针对 GitHub 的反向代理。
*   **使用方式**: 用户只需将 `github.com` 替换为 `kgithub.com`。
    *   原链接: `https://github.com/user/repo/releases/download/v1.0.0/app.zip`
    *   加速链: `https://kgithub.com/user/repo/releases/download/v1.0.0/app.zip`
*   **优点**: 简单，无需配置，支持 Git Clone 和 Release 下载。
*   **缺点**: 依赖第三方服务稳定性，不仅限于 Release，全站代理。

### 1.2 GHProxy (ghproxy.com / gh-proxy.com)
*   **原理**: 专门针对 GitHub 静态资源（Release, Raw, Archive）的公共反向代理。
*   **使用方式**: 在原链接前加上 `https://mirror.ghproxy.com/` (或其他节点)。
*   **优点**: 极其流行，很多开源项目直接在文档中提供此链接。
*   **缺点**: 公共节点常变动，带宽有限。
*   **自建**: 开发者可使用 Cloudflare Workers 自建（见方案 4）。

### 1.3 GitClone (gitclone.com)
*   **原理**: 缓存加速，主要针对 git clone，也支持部分资源。
*   **使用方式**: `git clone https://gitclone.com/github.com/user/repo`
*   **Release 支持**: 主要侧重代码库，Release 加速效果不如前两者明确。

---

## 2. 同步到国内代码托管平台 (Gitee/GitLink)

将 GitHub Release 自动同步到国内平台，利用国内平台的带宽分发。

### 2.1 Gitee (码云)
*   **现状**: Gitee 官方的 "从 GitHub 导入" 功能经常受限，且通常只同步代码，**不同步 Release 附件**。
*   **解决方案**: 使用 GitHub Actions 自动同步。
*   **推荐 Action**: `nICEnnnnnnnLee/action-gitee-release` 或 `Hub Mirror Action`。
    *   **功能**: 监听 GitHub Release 事件，自动下载附件并上传到 Gitee 对应的 Repo Release 中。
    *   **配置**: 需要 Gitee API Token (私钥/密码)。
*   **优点**: 国内访问速度极快，合规性好。
*   **缺点**: Gitee 对上传文件大小有限制（普通用户 <100MB），API 频控严格。

### 2.2 GitLink (确实开源) / AtomGit
*   **GitLink (确实开源)**:
    *   **API 能力**: 提供了 Release 相关 API，但文档（如 `https://www.gitlink.org.cn/docs/api`）主要集中在代码库管理。Release 附件上传 API 细节不明确，社区缺乏现成的 GitHub Action。
    *   **兼容性**: 底层基于 Gitea，理论上可能兼容 Gitea API，但官方并未明确承诺 API 兼容性。若尝试使用通用 Gitea Action (如 `go-gitea/gitea-release-action`) 可能面临鉴权或路径不匹配问题。
*   **AtomGit**:
    *   **API 能力**: OpenAPI 文档（`http://docs.atomgit.com/en/openAPI/`）处于早期阶段。虽然支持 Release 创建，但关于“上传二进制附件（Assets）”的 API 描述缺失或难以找到示例。
    *   **生态**: 暂无官方或第三方成熟的“GitHub to AtomGit Release Sync” Action。
*   **建议**: 现阶段不建议作为首选自动化分发渠道，除非愿意投入资源开发专用同步脚本。

---

## 3. 对象存储/CDN 分发 (高可靠/付费)

将 Release 产物上传到国内云厂商的对象存储（OSS/COS/Kodo），并开启 CDN。

### 3.1 阿里云 OSS / 腾讯云 COS / 七牛云
*   **实现方式**: 在 GitHub Actions Build 流程后，增加上传步骤。
*   **GitHub Actions**:
    *   **腾讯云 COS**: `TencentCloud/cos-action`
    *   **阿里云 OSS**: `manyuanrong/setup-ossutil` 或 `aliyun/oss-util`
    *   **七牛云**: `qiniu/upload-action`
*   **流程**:
    1. Build 生成二进制文件。
    2. Action 将文件上传至 Bucket (如 `https://download.example.com/v1.0.0/app.zip`)。
    3. 在 GitHub Release Description 中自动追加国内下载链接。
*   **优点**: 速度最快，最稳定，专业。
*   **缺点**: 需要付费（存储费+流量费），需要备案域名。

---

## 4. 自建无服务器代理 (Cloudflare Workers)

利用 Cloudflare 的全球边缘网络（对国内有一定优化或通过优选 IP）加速。

### 4.1 Cloudflare Workers
*   **原理**: 部署一个 Worker 脚本，拦截请求并代理回源 GitHub，同时处理 Header 避免跨域等问题。
*   **代码模板**: `hunshcn/gh-proxy` 是最经典的实现。
    *   仓库: `https://github.com/hunshcn/gh-proxy`
*   **部署**:
    1. Fork 仓库。
    2. 在 Cloudflare Dashboard 创建 Worker。
    3. 复制 `index.js` 代码。
    4. 绑定自定义域名（可选，推荐）。
*   **优点**: 免费（每日 10万次请求），完全可控，无广告。
*   **缺点**: Cloudflare `workers.dev` 域名在国内部分地区被阻断，建议绑定自定义域名。

---

## 5. 综合对比与建议

| 方案 | 速度 | 稳定性 | 成本 | 维护难度 | 适用场景 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **镜像站 (ghproxy/kgithub)** | ⭐⭐⭐ | ⭐⭐ | 免费 | 低 | 个人项目，快速分享，用户量不大 |
| **Gitee 同步** | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ | 免费 | 中 (需配Action) | 国内用户为主，文件 <100MB |
| **云存储 (OSS/COS)** | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | 付费 | 中 | 企业级项目，追求极致体验，有预算 |
| **自建 CF Worker** | ⭐⭐⭐ | ⭐⭐⭐ | 免费 | 中 | 只有技术用户，已有域名的开发者 |

### 最佳实践推荐
1.  **首选**: **Gitee 同步**。利用 `action-gitee-release` 实现 GitHub 发布时自动同步到 Gitee，在 README 中提供 Gitee 下载链接。体验最好且免费。
2.  **备选**: **镜像链接**。在 Release 说明中，利用 Markdown 提供一个 "国内镜像下载" 按钮，指向 `https://mirror.ghproxy.com/github.com/...`。
3.  **土豪**: 直接上 **OSS/COS + CDN**。
