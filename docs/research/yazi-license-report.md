# Yazi 开源协议分析报告

## 项目基本信息

- **项目名称**: Yazi
- **项目地址**: https://github.com/sxyazi/yazi
- **作者**: sxyazi (sxyazi@gmail.com)
- **当前版本**: v26.5.6 (latest release)
- **编程语言**: Rust (Edition 2024, MSRV 1.95.0)
- **协议**: MIT License

## MIT 协议全文

```
MIT License

Copyright (c) 2023 - sxyazi

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## MIT 协议核心权利与义务

### 赋予的权利

| 权利 | 说明 |
|------|------|
| 商业使用 | 可用于商业项目，无需付费 |
| 修改 | 可自由修改源代码 |
| 分发 | 可自由分发原始或修改后的代码 |
| 再授权 | 可以在不同协议下再授权 |
| 私人使用 | 可私人使用和修改 |

### 唯一义务

1. 在所有副本或重要部分中包含原始**版权声明**
2. 在所有副本或重要部分中包含原始**许可声明**

### 限制

- 不提供任何担保（AS IS）
- 作者不承担任何责任
- 不得使用作者姓名/商标进行推广背书

## 与其他常见开源协议对比

| 特性 | MIT | Apache 2.0 | GPL v3 | AGPL v3 |
|------|-----|-----------|--------|---------|
| 传染性 (Copyleft) | 无 | 无 | 强 | 极强 |
| 需要开源衍生作品 | 否 | 否 | 是 | 是 |
| 专利授权 | 否 | 是 | 是 | 是 |
| 商业友好度 | 极高 | 高 | 低 | 低 |
| 协议兼容性 | 最广 | 广 | 窄 | 最窄 |
| 适合闭源衍生 | 是 | 是 | 否 | 否 |

## 基于 Yazi 进行开源项目开发的合规建议

### 1. 必须做的事

- 在项目根目录保留 `LICENSE` 文件，包含原始 MIT 协议全文
- 在 README 或文档中注明项目基于 Yazi

### 2. 可以自由做的事

- Fork 并修改代码用于自己的项目
- 选择自己的开源协议（MIT、Apache 2.0、GPL 等均可）
- 闭源部分修改（MIT 不要求衍生作品开源）
- 商业化使用和销售
- 将代码整合到更大的项目中

### 3. 推荐做法

| 场景 | 建议协议 | 理由 |
|------|---------|------|
| 开源工具/库 | MIT 或 Apache 2.0 | 与上游保持一致，社区友好 |
| 开源产品 | Apache 2.0 | 提供专利保护 |
| 商业产品 | 商业许可 + MIT 声明 | 闭源商业使用完全合法 |
| 想保护衍生作品 | GPL v3 | 要求衍生作品也开源 |

### 4. 风险评估

- **法律风险**: 极低 — MIT 是最宽松的开源协议之一
- **协议冲突风险**: 极低 — MIT 与几乎所有协议兼容
- **专利风险**: 低 — MIT 不含专利授权条款，但 Yazi 主要是文件管理器，专利风险极小
- **商标风险**: 低 — 只要不在推广中使用 "Yazi" 品牌名即可

## 结论

Yazi 采用的 MIT 协议对基于它进行开源/商业开发**非常友好**。你只需要保留版权声明和许可声明，就可以自由地 fork、修改、分发、商业化。无论你选择开源还是闭源，MIT 协议都不会构成障碍。

---

*报告生成日期: 2026-06-01*
