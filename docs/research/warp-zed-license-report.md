# Warp & Zed 开源模块许可证分析报告

> 生成时间：2026-06-01 | 分析人：Claude Code
> 数据源：Warp (commit a44b7030) + Zed (commit 5b948e5)

---

## 一、项目概览

| 项目 | 语言 | Crates 数 | 源文件数 | 仓库地址 |
|------|------|----------|---------|---------|
| Warp | Rust | 47 | 1,760 | github.com/warpdotdev/Warp |
| Zed | Rust | 234 | 3,342 | github.com/zed-industries/zed |

---

## 二、许可证分布总览

### Warp 许可证分布

| 许可证 | 模块数 | 占比 | 商业可用性 |
|--------|-------|------|-----------|
| AGPL-3.0-only | 65 | 93% | ⚠️ 需开源衍生作品 |
| MIT | 2 | 3% | ✅ 自由使用 |
| Alacritty (Apache-2.0) | 部分 | - | ✅ 自由使用（终端模拟模块） |

**关键发现：** warpui 和 warpui_core 是唯二的 MIT 许可模块，可直接复用。

### Zed 许可证分布

| 许可证 | 模块数 | 占比 | 商业可用性 |
|--------|-------|------|-----------|
| GPL-3.0-or-later | 197 | 84% | ⚠️ 需开源衍生作品 |
| Apache-2.0 | 26 | 11% | ✅ 自由使用 |
| AGPL-3.0-or-later | 1 | <1% | ⚠️ 需开源衍生作品（含网络服务） |
| Unknown | 10 | 4% | ❓ 需确认 |

**关键发现：** GPUI 框架核心（gpui, sum_tree, collections 等 26 个 crate）采用 Apache-2.0，可直接用于商业项目。

---

## 三、按复用优先级分类的模块清单

### 🟢 Tier 1: 自由可用（MIT / Apache-2.0，无 copyleft 限制）

#### Warp — MIT 模块

| 模块 | 许可证 | 功能 | 依赖数 | 复用难度 |
|------|--------|------|--------|---------|
| warpui | MIT | 自研 UI 框架（Entity-Component-Handle 模式） | 72 | 中 |
| warpui_core | MIT | UI 核心渲染引擎 | 55 | 中 |

#### Zed — Apache-2.0 模块（26 个，商业友好）

| 模块 | 许可证 | 功能 | 复用难度 |
|------|--------|------|---------|
| **gpui** | Apache-2.0 | GPU 加速 UI 框架（Metal/WGPU） | 中-高 |
| **sum_tree** | Apache-2.0 | 并发安全 B-tree 数据结构 | 极低 |
| **collections** | Apache-2.0 | 高性能集合类型（indexmap/rustc-hash） | 极低 |
| **refineable** | Apache-2.0 | 精化类型派生宏 | 极低 |
| **extension_api** | Apache-2.0 | WASM 扩展 API 定义 | 极低 |
| **http_client** | Apache-2.0 | HTTP 客户端抽象 | 低 |
| **watch** | Apache-2.0 | 文件监视器 | 低 |
| **audio** | Apache-2.0 | 音频播放 | 低 |
| **db** | Apache-2.0 | SQLite 数据库封装 | 低 |
| **sqlez** | Apache-2.0 | SQLite 便捷宏 | 极低 |
| **sqlez_macros** | Apache-2.0 | SQL 宏 | 极低 |
| **util_macros** | Apache-2.0 | 通用工具宏 | 极低 |
| **gpui_macros** | Apache-2.0 | GPUI 过程宏 | 极低 |
| **gpui_linux** | Apache-2.0 | GPUI Linux 平台实现 | 中 |
| **gpui_macos** | Apache-2.0 | GPUI macOS 平台实现 | 中 |
| **gpui_windows** | Apache-2.0 | GPUI Windows 平台实现 | 中 |
| **gpui_platform** | Apache-2.0 | GPUI 平台抽象层 | 低 |
| **gpui_tokio** | Apache-2.0 | GPUI + Tokio 运行时桥接 | 低 |
| **gpui_wgpu** | Apache-2.0 | GPUI WGPU 渲染后端 | 中 |
| **language_onboarding** | Apache-2.0 | 语言配置向导 | 低 |
| **release_channel** | Apache-2.0 | 发布渠道管理 | 低 |
| **markdown_preview** | Apache-2.0 | Markdown 预览 | 低 |
| **image_viewer** | Apache-2.0 | 图片查看器 | 低 |
| **svg_preview** | Apache-2.0 | SVG 预览 | 低 |
| **eval_cli** | Apache-2.0 | 评估 CLI 工具 | 极低 |
| **eval_utils** | Apache-2.0 | 评估工具库 | 极低 |

### 🟡 Tier 2: Copyleft 但可独立复用（低内部依赖）

#### Warp — AGPL-3.0 模块（可复用）

| 模块 | 功能 | 内部依赖数 | 复用难度 |
|------|------|-----------|---------|
| **mcp** | Model Context Protocol 实现 | 0（纯外部依赖） | 极低 |
| **fuzzy_match** | 模糊匹配算法 | 0 | 极低 |
| **sum_tree** | 求和树数据结构 | 0 | 极低 |
| **string-offset** | 文本偏移管理 | 0 | 极低 |
| **field_mask** | Protobuf 字段掩码 | 0 | 极低 |
| **natural_language_detection** | 自然语言检测 | 0 | 极低 |
| **handlebars** | 模板引擎封装 | 0 | 极低 |
| **markdown_parser** | Markdown 解析 | 0 | 极低 |
| **settings_value** | 配置值类型 + derive 宏 | 1 | 低 |
| **warp_features** | Feature Flag 管理 | 0 | 极低 |

#### Zed — GPL-3.0 模块（可复用）

| 模块 | 功能 | 复用难度 |
|------|------|---------|
| **rope** | 不可变文本缓冲区 | 极低 |
| **text** | 文本编辑原语 | 低 |
| **streaming_diff** | 流式 diff 算法 | 极低 |
| **fuzzy_nucleo** | 高性能模糊匹配 | 极低 |
| **language_model_core** | LLM 客户端抽象接口 | 低 |
| **anthropic** | Anthropic API 客户端 | 低 |
| **open_ai** | OpenAI API 客户端 | 低 |
| **snippet** | 代码片段引擎 | 中 |
| **syntax_theme** | 语法高亮主题系统 | 低 |
| **settings_json** | JSON 配置解析 | 极低 |
| **proto** | 协议定义（protobuf） | 极低 |

### 🔴 Tier 3: Copyleft + 高耦合（需大量重构才能复用）

| 项目 | 模块 | 许可证 | 功能 | 依赖数 |
|------|------|--------|------|--------|
| Warp | warp_terminal | AGPL-3.0 | 终端模拟器 | 30 |
| Warp | editor | AGPL-3.0 | 文本编辑器 | 41 |
| Warp | ai | AGPL-3.0 | AI 集成 | 58 |
| Warp | warp_core | AGPL-3.0 | 核心运行时 | 52 |
| Warp | app | AGPL-3.0 | 主应用 | 227 |
| Zed | editor | GPL-3.0 | 编辑器核心 | 50+ |
| Zed | agent | GPL-3.0 | AI Agent 系统 | 40+ |
| Zed | language | GPL-3.0 | 语言服务 | 40+ |
| Zed | project | GPL-3.0 | 项目管理 | 50+ |
| Zed | collab | AGPL-3.0 | 协作编辑 | 30+ |
| Zed | extension_host | GPL-3.0 | 扩展运行时 | 20+ |

---

## 四、对 AgentHouse 开发的建议

### 立即可用（MIT/Apache-2.0，直接 cargo 依赖）

1. **Zed sum_tree + collections** — 数据结构基础，Apache-2.0
2. **Zed GPUI 全家桶** — 如果考虑用 GPU 渲染 UI，Apache-2.0
3. **Zed extension_api** — 插件系统参考，Apache-2.0
4. **Warp warpui / warpui_core** — 自研 UI 框架参考，MIT

### 短期可提取（AGPL/GPL，低依赖）

5. **Warp mcp** — MCP 协议实现，零内部依赖
6. **Warp fuzzy_match + markdown_parser** — 通用工具
7. **Zed rope + text + streaming_diff** — 文本编辑核心
8. **Zed language_model_core + anthropic** — LLM 客户端

### 长期战略参考

9. **Zed agent + acp_thread + acp_tools** — AI Agent 架构
10. **Warp ai + skills** — Warp 的 AI 技能系统
11. **Zed GPUI 完整框架** — 作为独立 UI 框架评估

---

## 五、许可证合规提醒

| 许可证 | 要求 | 对商业产品的影响 |
|--------|------|----------------|
| **MIT** | 保留版权声明 | ✅ 无限制，可直接集成 |
| **Apache-2.0** | 保留版权+声明变更 | ✅ 无限制，可直接集成 |
| **GPL-3.0** | 衍生作品必须 GPL-3.0 | ⚠️ 动态链接可分离，静态链接整个作品需开源 |
| **AGPL-3.0** | GPL-3.0 + 网络服务也需开源 | ⚠️ 网络服务使用也触发开源义务 |

**策略建议：**
- 独立进程调用 GPL/AGPL 模块 → 可避免传染（如 LSP server 模式）
- Apache-2.0 模块直接静态链接 → 无合规风险
- AGPL 模块作为独立微服务运行 → 避免主程序被传染
