# Yazi 技术架构分析报告

## 项目概览

- **项目名称**: Yazi — Blazing fast terminal file manager
- **项目地址**: https://github.com/sxyazi/yazi
- **当前版本**: v26.5.6
- **语言**: Rust (Edition 2024, MSRV 1.95.0)
- **协议**: MIT License
- **二进制产物**: `yazi` (文件管理器 TUI) + `ya` (CLI 伴侣工具)

## 技术栈

| 领域 | 技术选型 |
|------|---------|
| 异步运行时 | Tokio (full features) |
| TUI 框架 | ratatui 0.30 + crossterm 0.29 |
| 插件系统 | mlua (Lua 5.5, vendored) |
| 序列化 | serde + toml |
| SSH/SFTP | russh |
| 图片处理 | image + quantette + moxcms |
| 文件监控 | notify |
| 内存分配 | jemalloc (Linux) / 系统 (macOS/Windows) |
| CLI 解析 | clap |
| 哈希 | foldhash + twox-hash (xxHash3) |

## Workspace 架构（28 个 crate）

### 分层架构总览

```
┌─────────────────────────────────────────┐
│           yazi-fm (TUI 入口)              │
│           yazi-cli (CLI 入口)             │
├─────────────────────────────────────────┤
│  yazi-actor  │  yazi-parser  │  yazi-proxy │  命令分发层
├─────────────────────────────────────────┤
│              yazi-core                    │  核心状态模型
├──────────┬──────────┬────────────────────┤
│ yazi-plugin│yazi-runner│  yazi-binding    │  插件/Lua 层
├──────────┴──────────┴────────────────────┤
│ yazi-scheduler │ yazi-watcher │ yazi-dds  │  基础设施层
├────────────────┼──────────────┼───────────┤
│   yazi-adapter │   yazi-vfs   │ yazi-sftp │  适配/VFS 层
├────────────────┼──────────────┼───────────┤
│   yazi-config  │   yazi-fs    │ yazi-term │  配置/平台层
├────────────────┼──────────────┼───────────┤
│  yazi-widgets  │ yazi-emulator│ yazi-tty  │  UI/终端层
├────────────────┼──────────────┼───────────┤
│  yazi-macro    │  yazi-codegen│ yazi-shim  │  宏/代码生成层
├────────────────┼──────────────┼───────────┤
│  yazi-shared   │  yazi-ffi    │ yazi-boot  │  基础工具层
└────────────────┴──────────────┴───────────┘
```

### 各 crate 职责

#### 二进制入口

| Crate | 产物 | 职责 |
|-------|------|------|
| `yazi-fm` | `yazi` | 文件管理器 TUI 主程序，初始化所有子系统，运行事件循环 |
| `yazi-cli` | `ya` | CLI 伴侣，通过 DDS 向运行中的 Yazi 发送命令，管理插件包 |
| `yazi-boot` | — | 引导启动器，解析 CLI 参数（clap），生成 shell 补全 |

#### 核心逻辑层

| Crate | 职责 |
|-------|------|
| `yazi-core` | 核心状态模型，管理所有 UI 状态：文件管理标签页、任务、输入、确认框、帮助、补全等 |
| `yazi-actor` | Actor 命令分发层，定义 `Actor` trait，包含所有用户动作的具体实现 |
| `yazi-parser` | 动作参数解析，为每个 action 定义类型化的输入结构体 |
| `yazi-proxy` | 事件代理层，从调度器/后台任务桥接回主事件循环 |

#### 插件系统

| Crate | 职责 |
|-------|------|
| `yazi-plugin` | 插件运行时，管理全局 Lua VM，提供标准/精简两种 Lua 环境 |
| `yazi-binding` | Lua 绑定层（mlua bridge），将 Rust 类型暴露为 Lua UserData |
| `yazi-runner` | Lua 脚本执行器，管理插件加载、预加载器/抓取器/预览器注册 |

#### 基础设施层

| Crate | 职责 |
|-------|------|
| `yazi-scheduler` | 异步任务调度器，管理文件操作（复制/剪切/删除/链接）、插件执行、预加载，使用优先级通道 |
| `yazi-watcher` | 文件系统监控，基于 notify crate，支持本地和远程（VFS 代理）监控 |
| `yazi-adapter` | 图片协议适配器，自动检测终端能力，选择最佳图片渲染协议 |
| `yazi-dds` | 数据分发服务（IPC），支持多实例间通信和 pub/sub 模式 |

#### 文件系统层

| Crate | 职责 |
|-------|------|
| `yazi-fs` | 物理文件系统操作，文件元数据、目录列表、CWD 跟踪、挂载点、XDG 路径 |
| `yazi-vfs` | 虚拟文件系统抽象，统一本地和远程（SFTP）文件系统访问 |
| `yazi-sftp` | SFTP 客户端实现，基于 russh，含连接池（deadpool） |

#### 终端与 UI 层

| Crate | 职责 |
|-------|------|
| `yazi-term` | 终端管理，raw mode、alternate screen、bracketed paste、鼠标捕获 |
| `yazi-tty` | 底层 TTY 访问，stdin/stdout 平台相关句柄管理 |
| `yazi-emulator` | 终端模拟器检测数据库，识别终端品牌和能力 |
| `yazi-widgets` | UI 组件库（Input、Scrollable、Clear、Step、Clipboard），基于 ratatui |

#### 配置与构建

| Crate | 职责 |
|-------|------|
| `yazi-config` | TOML 配置解析器，解析 yazi.toml / keymap.toml / theme，支持预设覆盖 |
| `yazi-codegen` | 过程宏，提供 DeserializeOver / Overlay / FromLuaOwned 派生 |
| `yazi-macro` | 声明宏，提供模块声明、Actor 分发、事件发射、嵌入式资源加载 |
| `yazi-shim` | 兼容层，提供 RoCell / SyncCell 懒初始化单元和 trait 扩展 |
| `yazi-ffi` | FFI 绑定，macOS (CoreFoundation/IOKit) 和 Unix (libc) 平台特定调用 |

## 核心架构设计

### 1. 事件驱动架构

```
用户输入 → Key/Mouse/Resize Event
              ↓
         mpsc Channel
              ↓
       Event Loop (LocalSet)
              ↓
     Dispatcher → Router → Executor
              ↓
         Actor.act(cx, form)
              ↓
      更新 Core 状态 → 触发 Render
```

- 事件通过 `mpsc::unbounded_channel` 传递到主循环
- `Dispatcher` 按事件类型路由，`Router` 处理按键匹配，`Executor` 分发到对应 Actor
- 渲染节流：10ms 最小间隔，支持局部渲染

### 2. 插件系统（Lua）

**两种 Lua 环境**：

| 环境 | 用途 | 可用全局 |
|------|------|---------|
| Standard | 前台插件 | `ui`, `ya`, `fs`, `ps`, `rt`, `th` + 全部组件 |
| Slim | 后台任务 | `ui`, `ya`, `fs`, `rt`, `th`（子集） |

**插件类型**：

| 类型 | 说明 | 数量上限 |
|------|------|---------|
| Fetchers | 异步获取文件元数据（如 MIME 类型） | 16 |
| Preloaders | 异步准备预览数据 | 16 |
| Previewers | 异步渲染文件预览 | 不限 |
| Spotters | 详情面板渲染 | 不限 |

**绑定架构**：`yazi-binding` 通过 mlua 将 Rust 类型暴露为 Lua UserData，`Composer` 模式提供动态 get/set 代理。

### 3. 图片预览系统

支持 **7 种图片协议**，按终端能力自动选择：

| 协议 | 适用终端 |
|------|---------|
| Kitty Graphics Protocol (KGP) | Kitty, Ghostty, Rio |
| iTerm2 Internal Protocol (IIP) | iTerm2, WezTerm, Warp, VS Code, Hyper, Tabby |
| Sixel | Foot, Windows Terminal, BlackBox |
| X11 (Uberzug++) | X11 桌面环境 |
| Wayland (Uberzug++) | Wayland 合成器 |
| Chafa (文本回退) | 不支持图片的终端 |

图片格式支持：AVIF, BMP, DDS, EXR, GIF, HDR, ICO, JPEG, PNG, PNM, QOI, TGA, TIFF, WebP

Sixel 预览性能：从 190ms 优化到 21ms（Wu 色彩量化算法，9x 提升）

### 4. 虚拟文件系统

```
         yazi-vfs (抽象层)
        ┌───────┴───────┐
   本地 Provider    SFTP Provider
   (yazi-fs)        (yazi-sftp)
```

- 统一 URL 方案：本地路径和远程 SFTP 路径使用相同的 typed-path
- SFTP 基于 russh，使用 deadpool 连接池
- 文件监控也通过 VFS 代理支持远程

### 5. 配置系统

```
嵌入式预设 (TOML) → 解析为默认配置
                        ↓
用户配置 (TOML) → DeserializeOver 覆盖合并
                        ↓
                  最终生效配置
```

- 三个配置文件：`yazi.toml`（主配置）、`keymap.toml`（按键映射）、`theme.toml`（主题）
- `yazi-codegen` 生成的自定义 serde Deserializer 实现字段级覆盖
- 配置解析失败时回退到预设默认值

### 6. IPC 通信（DDS）

- 数据分发服务，支持多 Yazi 实例间通信
- Pub/Sub 模式广播事件
- `ya` CLI 通过 DDS 向运行中的实例发送命令
- 通过 `YAZI_ID` 唯一标识实例，`YAZI_LEVEL` 追踪嵌套深度

## 构建优化

| Profile | 设置 |
|---------|------|
| Release | LTO = true, codegen-units = 1, strip = true, panic = "abort" |
| Windows Release | panic = "unwind"（Windows 兼容性） |
| Dev | debug = "line-tables-only"（加速编译） |

## 关键技术亮点

1. **全异步 I/O**：基于 Tokio 的 async/await，所有文件操作非阻塞
2. **Lua 插件系统**：mlua (Lua 5.5) 提供完整插件生态，支持前台/后台两种环境
3. **7 协议图片预览**：自动检测终端能力，选择最佳图片渲染方式
4. **VFS 抽象**：统一本地和 SFTP 远程文件系统
5. **高性能**：Sixel 图片预览 9x 提速，目录大小计算 2x 提速，Lua 字节码缓存
6. **事件驱动**：mpsc 通道 + Actor 模式，10ms 渲染节流
7. **配置覆盖系统**：TOML 预设 + 用户配置字段级合并，通过过程宏实现

---

*报告生成日期: 2026-06-01*
