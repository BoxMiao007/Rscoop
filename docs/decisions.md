# 决策记录

| 日期 | 决策 | 为什么 | 放弃了什么 |
| ---- | ---- | ------ | ---------- |
| 2026-11-07 | fork 发布改为本地构建：`npm run tauri -- build` 产出 NSIS 安装包后手动上传 GitHub Release，并禁用仓库的 `Release Build` 工作流（`gh workflow disable`） | 明确要求不使用远程构建；本地构建产物可直接验证，发布节奏可控 | 放弃推送 tag 后由 GitHub Actions 自动构建并上传资产的流程；此后发布需本地构建并手动上传资产，恢复自动构建需执行 `gh workflow enable 409051913` |
