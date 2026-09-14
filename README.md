# 本地知识库助手

单机版 macOS 应用：把指定文件夹里的文本文件（`.txt` / `.md`）自动向量化存入本地
LanceDB，并通过本地运行的 Ollama 大模型进行检索增强问答（RAG）。所有数据（文档、
向量、对话）都保存在本机，不上传云端。

技术栈：React + TypeScript（前端）、Tauri 2 + Rust（后端/打包）、Ollama（本地推理）、
LanceDB（向量数据库）。

---

## 1. 目录结构

```
├── src/                  React 前端
│   ├── components/       Ollama状态 / 模型安装 / 文件夹选择 / 聊天 / 文档管理 / 关于 / 更新
│   ├── api.ts             Tauri invoke 封装
│   └── App.tsx
├── src-tauri/            Rust 后端
│   └── src/
│       ├── hardware.rs        硬件+中文环境检测
│       ├── config.rs           配置文件读写
│       ├── ollama/             安装 / 进程管理 / 拉取模型 / 聊天+向量化 API
│       ├── docs/                文件扫描（source / archive / delete）+ 分块
│       ├── vectordb/            LanceDB 封装
│       ├── manual_update.rs    "导入更新包"离线更新（见第 4 节）
│       └── commands.rs         全部 Tauri command
├── .github/workflows/release.yml   CI：签名 + 公证 + 发布到 GitHub Release
└── icon-source.png        生成应用图标用的源图（占位，可替换）
```

## 2. 本地开发（在 Cursor / 终端中）

前置要求（Mac 上）：

```bash
# Node.js 18+，Rust 工具链，Xcode Command Line Tools
xcode-select --install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
```

安装依赖并生成应用图标（首次执行一次即可，`tauri icon` 会读取 `icon-source.png`
生成 `src-tauri/icons/` 下全部尺寸 + `.icns`/`.ico`）：

```bash
npm install
npm install -g @tauri-apps/cli
npx tauri icon icon-source.png
```

开发模式（热重载）：

```bash
npm run tauri dev
```

本地打包（未签名，仅供自测，双击可能被 Gatekeeper 拦截，右键"打开"即可绕过一次）：

```bash
npm run tauri build
```

> `icon-source.png` 是我生成的占位图标（蓝底聊天气泡），建议换成你们自己的品牌图标
> 后重新执行一次 `npx tauri icon icon-source.png`。

### 已知的编译风险点

`src-tauri/src/vectordb/mod.rs` 里用到 `lancedb` + `arrow-array` crate。这两个库
版本要互相匹配，如果 `cargo build` 报类型不匹配（比如 `FixedSizeListArray` 相关报
错），执行：

```bash
cd src-tauri
cargo update -p arrow-array -p arrow-schema
```

让 Cargo 自动选一个和当前 `lancedb` 版本兼容的 arrow 版本。如果还有报错，把完整报
错贴给我，我来对应修一版。

## 3. 修改 GitHub 仓库信息（必须做）

仓库地址已经改好，指向 `stephenxue/claude_test_01`（public）：

1. `src-tauri/tauri.conf.json` → `plugins.updater.endpoints`：
   ```
   https://github.com/stephenxue/claude_test_01/releases/latest/download/latest.json
   ```
   ✅ 已改好，不需要再动。
2. `src-tauri/tauri.conf.json` → `plugins.updater.pubkey`：还是占位符
   `REPLACE_WITH_PUBKEY_FROM_tauri_signer_generate`，需要你按下一节生成一次，把公钥
   贴进去（公钥本身不是敏感信息，可以放心提交进仓库；私钥不能）。

## 4. 更新机制是怎么工作的

应用支持三种更新方式，覆盖"能访问 GitHub"和"访问不了 GitHub（例如中国大陆）"两种情况：

- **手动检查**：菜单栏"检查更新..."，或"关于"对话框里的"立即检查更新"按钮。
- **自动检查**（新增）：在"关于"对话框里打开"自动检查更新"开关后，应用每小时在后
  台静默检查一次；发现新版本会自动下载并安装到磁盘，但**不会**擅自重启——会弹出一
  个提示，等你点"现在重启"才真正切换到新版本，避免在你正在用/正在向量化的时候突然
  被打断。开关状态存在本地 `config.json` 里，重开应用还记得。
- **手动导入更新包**（新增，专门给连不上 GitHub 的场景用）：能访问 GitHub 的朋友
  帮忙把某个 Release 的 `.dmg` 或 `.app.tar.gz` 传给你（网盘/微信都行），你在"关于"
  对话框点"导入更新包..."选中这个文件，应用会自动解压/挂载、替换掉当前安装的版本、
  重启——全程不需要联网。这条路径**没有**签名校验（因为文件是你自己手动选的、已经
  信任了），跟前两种走网络+Ed25519签名校验的路径不同，两者互不影响，按需要用哪个都
  行。

前两种走的是 Tauri 官方 `updater` 插件：向
`.../releases/latest/download/latest.json` 请求版本清单，如果有新版本，下载安装包、
用 Ed25519 签名校验完整性再安装——这比"直接覆盖文件"更安全（不会因为下载中断/被
篡改而装坏）。`latest.json` 由下面第 5 步的 GitHub Actions 在你打 tag 发布时自动生
成并上传，你不需要手动维护它。

生成一对更新签名密钥（只需做一次）：

```bash
npm install -g @tauri-apps/cli
npx tauri signer generate -w ~/.tauri/local-rag-app.key
```

会输出一个公钥（复制到 `tauri.conf.json` 的 `pubkey` 字段）和一个私钥文件。私钥内容
后面要填进 GitHub Secrets（见第 5 步的 `TAURI_SIGNING_PRIVATE_KEY`），**不要提交进
仓库**，也不用发给我——生成、保管这对密钥全程只在你自己电脑上进行就够了。

## 5. 配置 GitHub Actions 自动构建 + 签名 + 公证

`.github/workflows/release.yml` 在你 push 一个 `v*.*.*` 格式的 tag 时自动触发，会
在 GitHub 的 macOS runner 上完成：编译 universal 二进制（Intel + Apple Silicon）→
用你的开发者证书签名 → 提交 Apple 公证 → 生成 `latest.json` → 发布 GitHub Release
（默认建为草稿，你确认后手动发布）。

### 需要准备的 GitHub Secrets

在仓库 Settings → Secrets and variables → Actions 里添加：

| Secret 名称 | 怎么拿 |
|---|---|
| `APPLE_CERTIFICATE` | Keychain Access 导出你的 "Developer ID Application" 证书为 `.p12`，执行 `base64 -i cert.p12 \| pbcopy` 粘贴进去 |
| `APPLE_CERTIFICATE_PASSWORD` | 导出 `.p12` 时自己设置的密码 |
| `APPLE_SIGNING_IDENTITY` | 证书全名，如 `Developer ID Application: Your Company (TEAMID)`，`security find-identity -v -p codesigning` 可以查到 |
| `APPLE_ID` | 你的 Apple 开发者账号邮箱 |
| `APPLE_PASSWORD` | 该 Apple ID 的**App 专用密码**（appleid.apple.com → 登录与安全 → App 专用密码），不是账号密码 |
| `APPLE_TEAM_ID` | 10 位 Team ID，developer.apple.com/account 右上角能看到 |
| `KEYCHAIN_PASSWORD` | 随便设一个强密码，仅在 CI 临时 keychain 里用 |
| `TAURI_SIGNING_PRIVATE_KEY` | 第 4 步生成的私钥文件内容（`cat ~/.tauri/local-rag-app.key`） |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 生成密钥时设置的密码 |

`GITHUB_TOKEN` 不用配，Actions 自动提供。

### 发布一个新版本

```bash
# 改 package.json / src-tauri/tauri.conf.json 里的 version 字段
git add -A && git commit -m "release: v0.2.0"
git tag v0.2.0
git push origin main --tags
```

Actions 跑完后去仓库的 Releases 页面，把草稿 Release 改成正式发布（Publish），用
户下次点"检查更新"就能拉到。

## 6. 模型选择说明（自动挑选，无需用户干预）

首次安装模型时，应用会检测内存大小 / 是否有可用 GPU（Apple Silicon always yes）/
系统语言是否为中文，自动挑选：

- **向量模型**：始终使用 `bge-m3`（BAAI，MIT 协议，中英文混合检索效果好）
- **对话模型**（三档，中文/非中文各一套，均可商用、无版权风险）：
  - 低配（<16GB 内存）：中文 `qwen2.5:1.5b`（Apache-2.0）/ 英文 `llama3.2:3b`（Llama
    社区协议，月活 <7亿 免费商用）
  - 中配（16-32GB）：中文 `qwen2.5:7b` / 英文 `llama3.1:8b`
  - 高配（≥32GB 且有独立/集成 GPU）：中文 `qwen2.5:14b` / 英文 `llama3.1:8b`

所有候选模型都是 Apache-2.0 / MIT / Llama 社区协议，可安全用于 100 人以下企业的
商业软件，不存在需要额外授权或付费的知识产权问题。

## 7. Source 文件夹的目录约定

选择文件夹后，应用会在里面自动创建：

```
<你选的文件夹>/
  *.txt / *.md   放新文档进来，点"开始向量化"（或之后的"重新扫描文件夹"）会被自动扫描并向量化
  archive/       已向量化的原文件会被移到这里 —— 对应界面里"已索引"这一列
  delete/        从知识库删除的文件会被移到这里 —— 对应界面里"已删除"这一列
```

`delete/` 完全由应用管理：文件只会通过界面里的"删除"按钮移进来，
也只会通过"恢复"按钮移出去（恢复时会重新向量化并放回 `archive/`）；
不再支持手动把文件拖进 `delete/` 来触发删除。

每次"扫描"（点击"开始向量化"或"重新扫描文件夹"）只处理文件夹根目录下的新文件，
删除 / 恢复已索引文档请使用文档列表里对应的按钮。

## 8. 关于 / About

`src/components/AboutDialog.tsx` 里的公司名称和网址目前是占位符
（`示例科技有限公司` / `https://example.com`），换成你们真实信息即可。

---

有任何编译报错或者想调整模型选型策略、UI 细节，把报错信息或需求发给我，我继续改。
