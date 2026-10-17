# Linux AppImage 白屏（EGL_BAD_PARAMETER）修复 — 决策记录

日期：2026-10-17
上游 issue：helixnow/deep-student#481（Linux Wayland 下 AppImage 无法正常运行，白屏）
相关 issue：helixnow/deep-student#480（同批 Linux 打包路径问题，独立修复）

## 根因

Tauri 的 AppImage 打包（linuxdeploy）会把构建镜像（Ubuntu 22.04）的
`libwayland-*.so*` 复制进 `$APPDIR/usr/lib`；生成的 `AppRun` 又把该目录放到
`LD_LIBRARY_PATH` 首位。宿主的 Mesa 驱动（libEGL/libGL/libgbm/DRI 均不随包发布）
之后被 dlopen 时，就会解析到包内的 wayland 1.20：

```
/usr/lib/libEGL_mesa.so.0: undefined symbol: wl_fixes_interface
```

`wl_fixes_interface` 是 wayland 1.23 才加入的符号 → `libEGL_mesa` 加载失败 →
`eglGetDisplay` 拿不到 display → `WebKitWebProcess` abort（`EGL_BAD_PARAMETER`），
窗口永远不绘制。NVIDIA 闭源 EGL 不链接 libwayland-client，所以这个问题长期被
误报为「AMD/Intel 显卡专属」；实际上所有 Mesa 驱动、X11 会话同样会中，
`WEBKIT_DISABLE_DMABUF_RENDERER` / `GDK_BACKEND` 都无法规避。

## 方案对比

| 方案 | 结论 |
|---|---|
| A. 运行时 re-exec + LD_PRELOAD 宿主 libwayland | 不采用。无法修复已发布版本；不能只按 Wayland 门控（X11+Mesa 同样失败）；多一次 exec 且难以端到端测试 |
| B. 打包后重打包（解包 → 删除 `libwayland-*` → 按原 runtime/压缩参数重打包）+ 重新 `tauri signer sign` | **采用**。与当前锁定的 `@tauri-apps/cli` 2.11.2 兼容；同时修复 X11/Wayland；已有生产先例（toolport 1.16.0, btsouth/toolport#831） |
| C. 升级 CLI 2.12.3 + `LINUXDEPLOY_EXCLUDED_LIBRARIES=libwayland-*` | 长期方向。2.11.x 的 linuxdeploy 忽略该环境变量，且 CLI 升级会影响 macOS/Windows 打包，留到下个版本单独验证 |
| D. Tauri 官方配置 | 暂无。`bundle.linux.appimage` 只有 `bundleMediaFramework` / `files`（只能加不能删）；tauri#15662 仍未合并 |

## 落地

- 新增 `scripts/ci/patch-appimage-wayland.sh`：保留 AppImage runtime 前缀字节，
  `--appimage-offset` + `--appimage-extract`（无需 FUSE）解包，删除全部
  `libwayland-*.so*`（含符号链接），用原 squashfs 压缩参数重打包，随后再解包
  校验无残留、AppRun 可执行、文件数一致；无 libwayland 时直接跳过（不改字节）。
- `.github/workflows/reusable-build-linux.yml`：打包后调用该脚本，删除旧 `.sig`
  并重新签名（AppImage 就是 updater 产物，签名必须对应最终字节），再做 base64
  校验；重命名后的最终产物上再独立解包 gate 一次；v1Compatible 的
  `AppImage.tar.gz` 若出现则 fail-closed（尚未适配）。
- `scripts/ci/install-linux-deps.sh`：新增 `squashfs-tools` 依赖。
- `scripts/__tests__/linux-appimage-wayland.test.mjs`：契约测试（随 ci.yml 的
  Build Configuration Contracts 在 ubuntu-22.04 上跑）。

## 验证

- 本地（macOS）：`bash -n`、shellcheck、`node --test scripts/__tests__/*.test.mjs`
  通过（Linux 专属逻辑无法在 macOS 执行）。
- 真实产物验证：用 v0.10.5 的 `_amd64_x86_64.AppImage` 在 Linux 容器中跑
  patch 脚本，确认 libwayland 被移除、二次运行 no-op、包可再次解包且 AppRun 完好。
- 发布流水线：0.10.6 的 Linux 构建会执行上述步骤；最终上传产物有独立 gate。

## 后续

- 0.10.7 评估 `@tauri-apps/cli` → 2.12.3 + 打包步骤
  `LINUXDEPLOY_EXCLUDED_LIBRARIES=libwayland-*`，验证三个桌面平台后删除重打包
  脚本（保留最终产物 gate）。
- 同类候选（tauri#15976）：`libxkbcommon*`、`libxcb-*`、`libXau`、`libXdmcp`，
  按需再放宽。
