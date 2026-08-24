# DeepSeek Desktop

DeepSeek Desktop 是一个面向本机 DeepSeek Harness 的非官方桌面启动器和
生命周期管理器，第一阶段优先支持 macOS Apple Silicon。本项目独立维护，
与 DeepSeek 官方没有隶属、授权或背书关系。

应用只管理用户指定的 Harness 源码目录和本机进程，不修改 Harness 产品
代码，不在安装包中重新分发 Harness 源码，也不修改 `~/.dsh`、LM Studio
配置、凭据或用户会话。

## 当前状态

项目在 `dev` 分支开发。当前实现包括：

- 首次设置、安全 `git clone`、源码路径/URL/端口配置；
- Git、Node.js、pnpm 依赖检测和可执行的修复建议；
- Harness 启动、停止、重启、进程组记录、遗留进程发现、端口检测、
  stdout/stderr 实时日志和打开 Web UI；
- 安全 Harness 更新流程：脏工作区保护、显式备份 stash、fetch、
  仅 fast-forward 更新、依赖安装、配置的构建清理、重建和条件重启；
- LM Studio `/v1/models` 检测、模型列表和连接错误展示；
- 核心决策测试、Rust 测试、GitHub Actions 和 macOS 无安装包开发构建入口。

没有 Apple 证书时，应用不会伪造签名或公证成功。

## 环境要求

- 首选 macOS Apple Silicon；路径和进程层保留 Windows/Linux 扩展能力；
- Node.js 20 或更高版本；
- pnpm 10 或与 Harness 兼容的版本；
- Git；
- Rust stable 及 Tauri 本地桌面构建所需系统依赖；
- 已有 Harness 源码目录，或可访问的 Git URL；
- 可选：运行在 `http://127.0.0.1:1234` 的 LM Studio。

## 开发

```
git clone <这个私有仓库> DeepSeek-Desktop
cd DeepSeek-Desktop
git switch dev
pnpm install
pnpm tauri:dev
```

`pnpm dev` 可以只预览浏览器界面；由于进程管理只在 Tauri 环境内可用，
浏览器预览会显示明确的“桌面后端不可用”提示。

## 首次设置

1. 打开“设置”。
2. 选择 Harness 源码目录。应用不会覆盖非空目录。
3. 确认上游 Git URL、分支和 Web 端口。
4. 保存设置。
5. 如果源码不存在，点击“首次安装”。应用使用参数数组调用 Git，
   只会 clone 到指定目标目录。
6. 检查依赖卡片；缺失工具会显示修复建议。
7. 如果 Harness 要求预先构建产物，先按 Harness 自己的文档完成构建，
   再启动 Web 服务。

默认启动命令等价于：

```
pnpm dsh web --no-open
```

默认 Harness Web 端口为 `3080`，与 Harness README 保持一致。

## 安全启停和更新

应用自身的设置和运行态保存在操作系统应用数据目录中。运行态包含 PID、
命令、源码路径、端口和启动时间。下次启动时会同时检查保存的 PID 和命令
行，只有确认是目标 Harness 命令才会认定为遗留进程，不会盲信可能被复用
的旧 PID。

停止时先向受管进程组发送中断信号，等待端口释放；超时后发送 TERM。
普通停止不会使用 `kill -9`。如果进程仍未退出，应用会停止后续更新并保留
现场，交给用户检查。

更新步骤如下：

1. 检查运行状态并停止服务；
2. 确认端口已经释放；
3. 检查 Git 工作区；
4. 默认遇到本地修改就中止，只有用户明确选择后才创建备份 stash；
5. 获取配置的远程仓库；
6. 只执行 `--ff-only` 更新；
7. 安装锁定依赖；
8. 只清理配置中、且位于源码目录内的构建路径；
9. 重新构建；
10. 只有构建成功且更新前服务在运行时才重启。

任一步失败都会停止后续步骤、保留日志、记录错误，并保留源码和恢复用
stash。应用不会执行 reset、强制 checkout、递归删除源码目录或自动
stash pop。

## LM Studio

默认地址为：

```
http://127.0.0.1:1234/v1/models
```

地址可在设置中修改。界面会显示返回的模型 ID 和连接错误。当前验证目标
为 `qwen3.6-35b-a3b-nvfp4` 与 `qwen3.8-27b-nvfp4`，它们不是唯一允许的
模型，也不会被硬编码成白名单。请求不会附带 API Key，日志会脱敏常见的
Authorization 和密钥字段。

## 检查命令

```
pnpm typecheck
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml
pnpm build
pnpm tauri build --no-bundle
```

`pnpm tauri:build` 是无安装包桌面构建的别名。在签名和公证凭据准备好之前，
暂不启用安装包构建。当前中性占位图标不是 DeepSeek 官方 Logo。

## 发布路线

两条更新通道分开处理：

1. Harness 更新使用配置的 Git 远程仓库和本地安全更新状态机；
2. DeepSeek Desktop 自身更新预留 GitHub Releases，用于后续发布已签名
   的 macOS 安装包和接入 Tauri updater。

正式发布前，需要配置 Apple Developer 签名、公证、updater 签名密钥和
GitHub Actions 私有 secrets。凭据只应放在发布环境，不能提交到仓库。
当前 CI 只构建未签名桌面二进制，不声称已经生成签名安装包。

## 仓库边界

`DeepSeek-Desktop` 是独立 Git 仓库。被管理的 `deepseek-harness` 是用户
指定的外部目录，桌面应用源码绝不能加入其中。应用可以只读参考 Harness
README、Git 元数据、配置和命令输出，但不会向 Harness 仓库添加桌面文件。

另请阅读 [README.md](README.md)、[CONTRIBUTING.md](CONTRIBUTING.md) 和
[SECURITY.md](SECURITY.md)。
