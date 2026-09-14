# ctos

[English](README.md) | **简体中文**

<p align="center">
  <img src="banner.webp" alt="ctos — count tokens of skill" width="100%">
</p>

**count tokens of skill** —— 一个 cloc 风格、跨平台的 token 统计工具，同时服务于 Agent Skill 与源代码。

> ⚠️ *与《看门狗》里的 ctOS 无关。*

[![CI](https://github.com/leftvalue/ctos/actions/workflows/ci.yml/badge.svg)](https://github.com/leftvalue/ctos/actions/workflows/ci.yml)
![license](https://img.shields.io/badge/license-GPL--3.0-blue.svg)

`cloc` 告诉你有多少**行**代码；`ctos` 告诉你一个目录消耗多少 **token**——可用任意你关心的 tokenizer 度量——并且当目录是一个 Agent Skill 时，还会拆分出这些 token 分布在哪些"真正会被注入模型上下文"的层级里。

由于现在大量代码都由 AI 编写，"这是多少 token？"对源码树来说也成了头等问题，而不只是对 skill。`ctos` 两者都能回答。

```
cloc:  代码行数   → 文件数、空行、注释、代码
ctos:  token 统计 → 文件数、字节、token   （+ skill 的 L1/L2/L3 分层）
```

---

## 目录

- [快速开始](#快速开始)
- [安装](#安装)
- [统计什么](#统计什么)
- [三层 skill 模型](#三层-skill-模型)
- [模型矩阵](#模型矩阵)
- [命令与选项](#命令与选项)
- [JSON 输出](#json-输出)
- [CI 集成](#ci-集成)
- [配置](#配置)
- [内置 tokenizer 的打包](#内置-tokenizer-的打包)
- [校准](#校准)
- [从源码构建](#从源码构建)
- [设计说明](#设计说明)
- [许可证](#许可证)

---

## 快速开始

```bash
# 统计整个源码树的 token（用注册表里的全部模型）
ctos ./my-project

# 只用一个模型，cloc 风格按语言聚合
ctos -m gpt-4o ./src

# 一次传入多个路径，并排除若干目录
ctos -m gpt-4o --exclude-dir node_modules,target ./src ./docs

# 只统计 Python 与 Rust，按行数排序
ctos -m gpt-4o --include-lang Python,Rust --sort lines ./src

# 逐文件树形展示，以及 cloc 风格三线表
ctos -m gpt-4o --by-file ./src
ctos -m gpt-4o --style plain ./src

# Markdown / CSV 输出；从 stdin 读取一段文本
ctos -m gpt-4o --format md ./src > report.md
echo "some text" | ctos -m gpt-4o --stdin-name note.md -

# 一个 skill 目录 —— 代码表 + L1/L2/L3 分层分析
ctos -m gpt-4o ./skills/image-gen-retry

# 机器可读输出，供 CI / baseline 消费
ctos --format json ./skills > report.json

# 预算门禁（见 CI 章节）—— skill 超预算时以非零码退出
ctos check ./skills
```

示例（按语言聚合 + skill 分层）：

```
ctos v0.2.2 — count tokens of skill
root: /repo/skills
      7 files scanned.  (6 text, 1 binary)
github.com/leftvalue/ctos v0.2.2  T=0.02 s (350.0 files/s, 8100.0 lines/s)

tokenizer: gpt-4o (tiktoken)
┌──────────┬───────┬───────┬─────────┬─────────┐
│ Language ┆ files ┆ lines ┆   bytes ┆  tokens │
╞══════════╪═══════╪═══════╪═════════╪═════════╡
│ Markdown ┆     4 ┆   612 ┆  48.2 KB ┆  12,044 │
│ Python   ┆     2 ┆    98 ┆   3.1 KB ┆     812 │
│ Image    ┆     1 ┆     - ┆ 120.0 KB ┆       0 │
│ SUM      ┆     7 ┆   710 ┆ 171.3 KB ┆  12,856 │
└──────────┴───────┴───────┴─────────┴─────────┘

Skill layers (L1 metadata / L2 body / L3 assets):
┌────────────────────────┬─────┬───────┬─────────┬───────────┬────────┐
│ skill                  ┆  L1 ┆    L2 ┆ L3(tok) ┆ L3(bytes) ┆ status │
╞════════════════════════╪═════╪═══════╪═════════╪═══════════╪════════╡
│ image-gen-retry        ┆  92 ┆ 3,412 ┆  12,044 ┆   48.2 KB ┆ OK     │
│ TOTAL (1 valid skills) ┆  92 ┆ 3,412 ┆  12,044 ┆   48.2 KB ┆        │
└────────────────────────┴─────┴───────┴─────────┴───────────┴────────┘
resident L1 total: 92 tok  →  peak injection: 3,504 tok
```

`--style plain` 会改用 cloc 风格的三线表，`--by-file` 则以 `tree(1)` 风格展示层级结构：

```
File                      Language  lines  bytes  tokens
└── skills/
    ├── alpha/
    │   ├── references/
    │   │   └── guide.md  Markdown      3   87 B      21
    │   └── SKILL.md      Markdown      8  246 B      58
    └── beta/
        └── SKILL.md      Markdown      4   82 B      19

5 files · 22 lines · 551 B · 127 tokens
```

顶部的扫描概览（扫描文件数、版本、吞吐）对齐 cloc 风格。最后一行才是容量规划真正关心的：**常驻成本**（每个 skill 的 L1 始终在上下文中）与**峰值注入成本**（常驻 + 某次触发能拉进来的最重那个 skill 正文）。

---

## 安装

### 方式 1 —— 下载预编译二进制（无需工具链）

预编译的静态二进制发布在 **[Releases](../../releases)** 页面，覆盖五个目标平台，以压缩包形式提供：

| 平台 | 架构 | 资产文件 |
|---|---|---|
| Linux | x86_64 | `ctos-x86_64-unknown-linux-musl.tar.gz` |
| Linux | aarch64 | `ctos-aarch64-unknown-linux-musl.tar.gz` |
| macOS | Apple Silicon（arm64） | `ctos-aarch64-apple-darwin.tar.gz` |
| macOS | Intel（x86_64） | `ctos-x86_64-apple-darwin.tar.gz` |
| Windows | x86_64 | `ctos-x86_64-pc-windows-msvc.zip` |

> **不确定选哪个？** 运行 `uname -m`：输出 `x86_64` / `amd64` → 选 x86_64 包；
> `arm64` / `aarch64` → 选 aarch64 包。（较新的 Mac 均为 Apple Silicon = arm64。）
> 下面命令里的 `v0.2.2` 请替换为 Releases 页面上的最新 tag。

**Linux — x86_64（Intel/AMD）：**

```bash
curl -L https://github.com/leftvalue/ctos/releases/download/v0.2.2/ctos-x86_64-unknown-linux-musl.tar.gz | tar xz
sudo install -m 755 ctos /usr/local/bin/ctos   # 或：sudo mv ctos /usr/local/bin/
ctos --version
```

**Linux — aarch64（ARM64）：**

```bash
curl -L https://github.com/leftvalue/ctos/releases/download/v0.2.2/ctos-aarch64-unknown-linux-musl.tar.gz | tar xz
sudo install -m 755 ctos /usr/local/bin/ctos
ctos --version
```

**macOS — Apple Silicon（M1/M2/M3，arm64）：**

```bash
curl -L https://github.com/leftvalue/ctos/releases/download/v0.2.2/ctos-aarch64-apple-darwin.tar.gz | tar xz
xattr -d com.apple.quarantine ./ctos 2>/dev/null || true   # 解除 Gatekeeper 隔离
sudo mv ctos /usr/local/bin/
ctos --version
```

**macOS — Intel（x86_64）：**

```bash
curl -L https://github.com/leftvalue/ctos/releases/download/v0.2.2/ctos-x86_64-apple-darwin.tar.gz | tar xz
xattr -d com.apple.quarantine ./ctos 2>/dev/null || true   # 解除 Gatekeeper 隔离
sudo mv ctos /usr/local/bin/
ctos --version
```

**Windows — x86_64（PowerShell）：**

```powershell
Invoke-WebRequest -Uri "https://github.com/leftvalue/ctos/releases/download/v0.2.2/ctos-x86_64-pc-windows-msvc.zip" -OutFile ctos.zip
Expand-Archive ctos.zip -DestinationPath .
.\ctos.exe --version
# 然后把 ctos.exe 放到 PATH 上的目录里
```

> **macOS Gatekeeper：** 二进制未做代码签名/公证，首次运行可能被拦截。上面的 `xattr`
> 命令会清除隔离标记；或在首次弹窗后到 **系统设置 → 隐私与安全性 → 仍要打开** 放行。

> Release 只有在维护者推送版本 tag 后才会出现 —— 见[发布流程](#发布流程维护者)。

### 方式 2 —— 用 Cargo 从 git 安装（无需 crates.io）

```bash
cargo install --git https://github.com/leftvalue/ctos --tag v0.2.2
# 或使用最新的默认分支：
cargo install --git https://github.com/leftvalue/ctos
```

这会在本地编译并把 `ctos` 装到 `~/.cargo/bin`。内置 tokenizer 已随仓库提交，因此构建完全离线。

### 方式 3 —— 克隆后构建

```bash
git clone https://github.com/leftvalue/ctos
cd ctos
cargo build --release        # 二进制位于 target/release/ctos
```

### 方式 4 —— crates.io

尚未发布（`publish = false`）。若将来发布：

```bash
cargo install ctos
```

### 方式 5 —— Docker（无需工具链、无需安装）

一个极小（约 35 MB）、基于 `scratch` 的镜像，内含全静态二进制与打包好的内置 tokenizer。挂载你的项目并传入挂载点下的路径：

```bash
# 本地构建
docker build -t ctos .

# 或拉取已发布镜像
docker pull ghcr.io/leftvalue/ctos:latest

# 统计当前目录
docker run --rm -v "$PWD:/work" ghcr.io/leftvalue/ctos /work

# 指定模型 + skill 预算门禁（退出码会被保留供 CI 使用）
docker run --rm -v "$PWD:/work" ghcr.io/leftvalue/ctos -m gpt-4o /work/skills
docker run --rm -v "$PWD:/work" ghcr.io/leftvalue/ctos check /work/skills

# 元命令
docker run --rm ghcr.io/leftvalue/ctos --version
docker run --rm ghcr.io/leftvalue/ctos models
```

> 容器的工作目录是 `/work`；把项目挂载到这里，并使用 `/work/...` 路径。镜像里没有 shell —— `ctos` 就是入口点。

### Shell 补全

`ctos` 可以为你的 shell 生成 Tab 补全脚本（覆盖全部子命令与参数）。默认不启用——只需生成一次脚本，放到 shell 查找补全的目录即可。

**bash：**

```bash
mkdir -p ~/.local/share/bash-completion/completions
ctos completions bash > ~/.local/share/bash-completion/completions/ctos
# 或安装到系统级：
# ctos completions bash | sudo tee /etc/bash_completion.d/ctos >/dev/null
```

**zsh：**

```bash
mkdir -p ~/.zfunc
ctos completions zsh > ~/.zfunc/_ctos
# 确保 ~/.zshrc 里有这两行（只需一次）：
#   fpath=(~/.zfunc $fpath)
#   autoload -U compinit && compinit
```

**fish：**

```bash
ctos completions fish > ~/.config/fish/completions/ctos.fish
```

重开终端（fish 会自动加载），之后 Tab 就能补全 `ctos ch⇥` → `check`、`--for⇥` → `--format`、以及枚举取值等。

> `powershell` 与 `elvish` 同样支持（`ctos completions powershell`、`ctos completions elvish`）。升级 `ctos` 后，若子命令或参数有变化，请重新生成脚本。

### 发布流程（维护者）

仅推送代码**不会**产生二进制。发布工作流由版本 tag 触发：

```bash
git tag v0.2.2
git push origin v0.2.2
```

随后 GitHub Actions 会：拉取 tokenizer、交叉编译全部五个目标、把二进制附加到 GitHub Release，并构建并推送 Docker 镜像到 `ghcr.io/leftvalue/ctos`（打上版本号与 `latest` 标签）。（`ci.yml` —— fmt/clippy/test —— 每次 push/PR 都会跑；只有 `release.yml` 需要 tag。）

---

## 统计什么

`ctos` 遍历路径（像 ripgrep 一样尊重 `.gitignore`），对每个文件：

| 文件类型 | tokens | 行数 | bytes | 语言 |
|---|---|---|---|---|
| 纯文本（代码、markdown、配置……） | ✅ 按模型计数 | ✅ 物理行数 | ✅ | 按扩展名映射 |
| 二进制（图片、字体、压缩包、`.bin`……） | —（跳过） | — | ✅ | 尽力标注 |

- **文本 vs 二进制**：通过头部采样判断（含 NUL 字节或大部分不是合法 UTF-8 ⇒ 二进制）。
- 文件按 UTF-8 读取；非法字节替换为 `U+FFFD` 并计入 —— `ctos` 绝不会因编码问题 panic。
- token 计数使用 **`add_special_tokens = false`**。特殊/框架 token 是客户端的职责；`ctos` 只度量原始内容，外加（对 L1）一个固定、可配置的包装开销。

---

## 三层 skill 模型

一个 Agent Skill 是包含 `SKILL.md`（YAML frontmatter + 正文）的目录，可选地带有 `scripts/`、`references/`、`assets/` 等。`ctos` 按"每部分*何时*进入模型上下文"把它拆成三层：

```
skill/
├── SKILL.md
│   ├── frontmatter: name + description ─────────────┐
│   └── 正文 (markdown)                               │
├── scripts/…                                        │
└── references/…                                     │
                                                     │
   L1  name + description (+ 开销)   ── 常驻，始终注入        ≤ 100 tok
   L2  整个 frontmatter 块 + 正文     ── skill 被触发时注入    ≤ 5000 tok
   L3  其余每个文本文件               ── 按需读取，从不常驻      （仅展示）
```

- **L1 = tokens(name + description) + `overhead_l1`**（默认 `24`，可配）。这是常开成本；所有 skill 的 L1 之和就是你的常驻预算。
- **L2 = tokens(整个 frontmatter 块 + 正文)。** *设计选择：* 整个 frontmatter 块都计入 L2（不只是 `name`/`description`）。L1 仅由 `name`+`description`+开销 度量。
- **L3 = skill 目录中除 `SKILL.md` 外的每个文本文件**，同时报告 tokens 与 bytes。二进制资产只计字节。
- 缺少 `name`/`description` 或 YAML 无法解析的 skill 会被标记为 **`INVALID`** —— 不会中断整体运行，但会影响退出码。

---

## 模型矩阵

`ctos` 把友好的模型名解析为四种 tokenizer 来源之一：

| 来源                  | 形式                     | 示例                            | 离线 | 说明 |
|-----------------------|--------------------------|---------------------------------|:----:|------|
| **builtin**           | `builtin:<key>`          | `builtin:qwen3`                 | ✅   | 打包进二进制的 `tokenizer.json` **或** `tiktoken.model` |
| **file**              | `file:<path>`            | `file:/opt/models/tok.json`     | ✅   | 任意本地 `tokenizer.json`（内网 / 长尾模型） |
| **tiktoken**          | `tiktoken:<encoding>`    | `tiktoken:o200k_base`           | ✅   | OpenAI 编码（`o200k_base`、`cl100k_base`……） |
| **claude-approx**     | `claude-approx`          | —                               | ✅   | `字符数 / chars_per_token` 估计 |

默认注册表（`config/models.toml`）：`qwen3`、`deepseek-v3`、`kimi-k2`、`hunyuan`（均为 `builtin:`）、`gpt-4o`（`tiktoken:o200k_base`）与 `claude`（`claude-approx`）。

> **默认模型：** 当你不传 `-m/--model` 时，`ctos` 使用 `config/models.toml` 里的 `default_models` 列表，出厂值为 `["qwen3"]`（即默认用千问）。把它设为 `default_models = []` 可回落为"用*全部*已注册模型"分别统计，或列出多个名字让多个模型成为默认。

> **一次跑全部模型：** 传 `--all-models`（或 `-m all`）即可用整个注册表一次性统计。任何无法加载 tokenizer 的模型——例如你的构建里没有 vendor 的 `builtin:` 模型（默认情况下的 `hunyuan` 就是如此）——会在 stderr 上以 warning 报告并被跳过，因此单个缺失的 tokenizer 绝不会拖垮整体运行。

> **关于 Kimi-K2：** 它不发布 HuggingFace `tokenizer.json`；而是提供 `tiktoken.model` BPE 词表加上一个自定义的切分正则。`ctos` 通过 tiktoken-rs 用 Kimi 的**原始**正则（包括其 `&&` 字符集交集子类，`fancy-regex` 能接受）加载该模型，因此计数与模型精确一致且完全离线。

> **关于 Claude：** 没有公开的 Claude tokenizer。`claude` 是一个**近似**（默认 `字符数 / 4.0`），其数字以 `~` 前缀标注。在 `check` 中，近似模型的预算放宽 ×1.1。`ctos` **不**声称精确支持 Claude —— 请对它做校准（见下文）。

列出当前构建已知的全部模型：

```bash
ctos models
```

---

## 命令与选项

```
ctos <PATH> [OPTIONS]          # 默认：count 统计
```
ctos <PATH>... [OPTIONS]       # 默认：count 统计（可传多个路径；`-` = stdin）
ctos check <PATH>... [OPTIONS] # CI 预算门禁
ctos models                    # 列出注册表模型及其来源
ctos calibrate --model <m> <PATH>   # 输出各层计数，供人工校准
```

常用选项：

| 选项 | 含义 | 默认 |
|---|---|---|
| `-m, --model <name>` | 使用的 tokenizer，可重复指定；`-m all` = 全部模型 | `qwen3`（见 `default_models`） |
| `-a, --all-models` | 使用注册表全部模型；无法加载的模型会 warning 并跳过 | 关闭 |
| `--format table\|json\|md\|csv` | 输出格式 | `table` |
| `--style boxed\|plain` | 表格风格（`plain` = cloc 风格三线表） | `boxed` |
| `--sort tokens\|bytes\|lines\|files\|name` | 语言/文件行的排序键 | `tokens`（降序） |
| `--summary-cutoff <X:N[%]>` | 把低于阈值的语言并入 `Other`（X = tokens\|files\|lines\|bytes） | 无 |
| `--exclude-dir <D1,D2,...>` | 按目录名排除子树 | 无 |
| `--include-ext` / `--exclude-ext <e1,...>` | 按扩展名过滤（白名单 / 黑名单） | 无 |
| `--include-lang` / `--exclude-lang <L1,...>` | 按语言过滤（白名单 / 黑名单） | 无 |
| `--max-file-size <MB>` | 遍历时跳过超大文件（命令行显式路径豁免） | 无 |
| `--hide-rate` | 隐藏耗时/吞吐（输出确定性可复现） | 关闭 |
| `--no-progress` | 关闭实时进度条（`-q` 隐含关闭） | 自动 |
| `--estimate` | 快速预估模式：按语言分层采样，替代全量精确编码 | 关闭 |
| `--sample-budget <CHARS>` | `--estimate` 的每语言字符预算 | 524288 |
| `--by-file` | 逐文件树形展示，而非按语言聚合 | 关闭 |
| `--by-file-by-lang` | 逐文件树形展示 **加** 语言聚合 | 关闭 |
| `--stdin-name <file>` | 用于判定 `-`（stdin）输入语言的文件名 | — |
| `-o, --output <path>` | 输出到文件 | stdout |
| `--baseline <path>` | （`check`）用于涨幅对比的 baseline 结果 JSON | 无 |
| `--budgets <path>` | 自定义 `budgets.toml` | 内置默认 |
| `--models-config <path>` | 自定义 `models.toml` | 内置默认 |
| `--no-ignore` | 不尊重 `.gitignore` | 尊重 |
| `-v, --verbose` | 逐文件 L3 明细 / 配置回落提示 | 关闭 |
| `-q, --quiet` | 最简输出 | 关闭 |

过滤优先级：`exclude` 优先于 `include`；非空的 `include` 列表作为白名单。`--summary-cutoff` 只作用于语言聚合表（逐文件视图忽略）。

**实时进度条。** 扫描大目录时，`ctos` 会在 **stderr** 上显示两阶段进度（stdout 的表格/JSON 输出保持逐字节纯净）：

```
⠴ scan 1234 files · 56.8 MB · 5.2 MB/s [00:00:11]                          ← 扫描中（总量未知）
█████████████░░░░░ tokenize 12.4/56.8 MiB @ 3.1 MiB/s qwen3 · src/… ETA 00:08  ← 计数中（按字节计量）
```

token 化进度条按**字节**而非文件数计量——单个巨大的 vendored 文件会按比例推动进度条、ETA 保持诚实，不会卡在 99% 不动；同时显示当前正在编码的文件名。skill 的 L3 资产只编码一次并复用（不会按层重复编码）。

仅当 stderr 是终端时才渲染——管道、重定向与 CI 日志自动保持干净。`-q` 隐含关闭，`--no-progress` 强制关闭；`--hide-rate` 只隐藏表头里的*最终*耗时/吞吐行（与进度条互不影响）。

**快速预估模式。** 大仓库精确计数太慢时：

```bash
ctos --estimate .            # 采样估算，替代逐文件精确编码
ctos --estimate --sample-budget 262144 .   # 预算更小 = 更快、更粗
```

文件并不同质（混合目录树的 chars→token 比例离散约 20%），但**语言内**相当稳定（常见语言实测 CV 5-13%）。因此 `--estimate` 按语言确定性采样：大文件优先、单文件最多贡献 64K 字符头部切片、消耗每语言预算（默认 512K 字符，`--sample-budget` 可调）。切片文件按自身头部比例外推；未采样文件按语言比例估算。token 化工作量与**仓库大小无关**——100 MB 的目录树几秒出结果。实测语料（Rust 工具仓库与 1000 文件的 Perl 仓库）误差分别为 1.9% 与 4.4%，均在报告的误差界内；误差界本身可能偏松（上述仓库为 ±7% 到 ±28%）——请视为最坏情况指示，而非紧的置信区间。

输出诚实标记：`[estimate] sampled N/M files · X% of chars · ±Y%` 统计行（误差界只覆盖未精确编码的部分——全量采样时报 ±0%）、预估数值带 `~` 前缀、JSON 附 `estimate` 元数据对象。SKILL.md 的 L1/L2 始终精确。`check` 拒绝 `--estimate`（预算门禁不能建立在预估上）。全程无随机，结果完全确定可复现。

---

## JSON 输出

`--format json` 产出稳定、对 CI 友好的文档。`results` 数组遵循锁定的 schema；平行的 `code` 块承载按语言/文件的聚合：

```json
{
  "tool": { "name": "ctos", "version": "0.2.2" },
  "models": ["gpt-4o"],
  "results": [
    {
      "skill": "image-gen-retry",
      "model": "gpt-4o",
      "l1": 92, "l2": 3412,
      "l3_tokens": 12044, "l3_bytes": 49357,
      "files": [{ "path": "scripts/render.py", "tokens": 12044 }],
      "status": "OK",
      "issues": []
    }
  ],
  "code": [
    {
      "model": "gpt-4o",
      "approx": false,
      "languages": [{ "language": "Markdown", "files": 4, "bytes": 48200, "tokens": 12044 }],
      "files": [{ "path": "…", "language": "Markdown", "bytes": 246, "is_binary": false, "tokens": 58 }]
    }
  ]
}
```

---

## CI 集成

`ctos check` 就是门禁。退出码是一份契约：

| 码 | 含义 |
|:---:|---|
| `0` | 所有 skill 都在预算内 |
| `1` | 有 skill 超预算 / 涨幅过大 / 为 `INVALID` |
| `2` | 运行错误（路径不存在、配置无法解析……） |

开箱即用的 GitHub Actions 步骤（可直接复制）：

```yaml
name: skill-budget
on: [pull_request]
jobs:
  ctos:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install ctos
        run: cargo install --git https://github.com/leftvalue/ctos --tag v0.2.2   # 或下载 release 二进制
      - name: Enforce skill budgets
        run: ctos check ./skills -m gpt-4o
```

带 baseline 以捕捉悄然增长：

```yaml
      - name: Fetch baseline (from main)
        run: git show origin/main:ctos-baseline.json > baseline.json || echo '{}' > baseline.json
      - name: Check growth
        run: ctos check ./skills -m gpt-4o --baseline baseline.json
```

如果某个 skill 的 L2 涨幅超过 `diff_ratio`（默认 15%），会失败——**除非**其 `SKILL.md` 中包含 `ctos-ok-growth` 标记（例如放在 HTML 注释里），以确认这是有意的增长。

---

## 配置

两个 TOML 文件，均可通过 `--models-config` / `--budgets` 覆盖。省略自定义文件时使用内置默认（在 `-v` 下会提示）。

**`config/models.toml`** —— 注册表：

```toml
# 未指定 -m/--model 时使用的模型（[] 表示用全部已注册模型）
default_models = ["qwen3"]

[models.qwen3]
source = "builtin:qwen3"
overhead_l1 = 24

[models.gpt-4o]
source = "tiktoken:o200k_base"

[models.claude]
source = "claude-approx"
chars_per_token = 4.0

[models.custom-internal]
source = "file:/opt/models/internal/tokenizer.json"
```

**`config/budgets.toml`** —— 门禁：

```toml
[global]
l1_max       = 100    # 每个 skill 的 L1 上限
l2_max       = 5000   # 每个 skill 的 L2 上限
l1_total_max = 2000   # 所有 skill 的 L1 之和（常驻容量）
diff_ratio   = 0.15   # 相对 baseline 的 L2 涨幅，超过则需说明
```

---

## 内置 tokenizer 的打包

`builtin:` 模型在构建期把其 tokenizer 文件（HF 模型是 `tokenizer.json`，Kimi-K2 是 `tiktoken.model`）**嵌入二进制**（通过 `build.rs` + `include_bytes!`），因此发布产物完全离线可用。

最简单的方式是用 fetch 脚本，它把每个文件钉到确切的 commit，并对照 `tokenizers/checksums.sha256` 校验：

```bash
scripts/fetch-tokenizers.sh          # qwen3 + deepseek-v3 + kimi-k2（默认集）
scripts/fetch-tokenizers.sh hunyuan  # 额外拉取可选的 Hunyuan 模型
cargo build --release
```

或者手动把文件放到 `tokenizers/<key>/`（`tokenizer.json` 或 `tiktoken.model`）后重新构建。

若文件**缺失**，构建仍会成功，而该 `builtin:` 模型会在运行时给出清晰、可操作的错误 —— 你随时可以回落到 `tiktoken:`、`claude-approx` 或 `file:` 来源。详见 [`tokenizers/README.md`](tokenizers/README.md)。

> **许可：** 每个打包文件都按其各自的模型许可再分发，而非 ctos 的 GPL-3.0。把它嵌入被分发的二进制属于再分发行为 —— 见 [`THIRD_PARTY_LICENSES/README.md`](THIRD_PARTY_LICENSES/README.md)。Hunyuan 之所以是可选项，是因为其社区许可带有额外条件。

> 钉住你下载的确切 revision。tokenizer 变化会改变计数，golden 测试正是为捕捉这一点而设计的。

---

## 校准

L1 的 `overhead_l1` 常量与 `claude-approx` 的 `chars_per_token` 除数用于近似真实客户端的注入成本。校准方法：

```bash
ctos calibrate --model claude ./skills/image-gen-retry
```

把 `ctos` 的数字与客户端报告的数字（例如 Claude Code 的 `/skills` 视图）对照，然后在 `models.toml` 中调整 `overhead_l1` / `chars_per_token`。见 [`calibration/README.md`](calibration/README.md)。自动化校准计划在 v0.2 提供。

---

## 从源码构建

需要较新的 stable Rust 工具链。

```bash
git clone https://github.com/leftvalue/ctos
cd ctos
cargo build --release       # 二进制位于 target/release/ctos
cargo test --all-features   # 单元 + golden + 退出码 测试
```

跨平台静态二进制由 release 工作流用 [`cargo-zigbuild`](https://github.com/rust-cross/cargo-zigbuild) 为五个目标产出（Linux x86_64/aarch64 musl、macOS x86_64/arm64、Windows x86_64）。

本地打包构建：

```bash
scripts/build-release.sh                        # 主机目标 -> ctos-<arch>-<os>.tar.gz
scripts/build-release.sh x86_64-unknown-linux-musl
```

**二进制体积：** 内置的 tokenizer 以 **gzip 压缩** 形式嵌入、按需解压，因此二进制既自带离线数据又保持精简（约为朴素构建的一半）。

Feature 开关：`hf`（HuggingFace `tokenizers`，用于 `builtin:`/`file:`）与 `tiktoken`（OpenAI 编码）默认都开启；`claude-approx` 两者都不需要。如不需要某个功能，可关闭对应 feature 以缩小二进制。

---

## 设计说明

- **度量，而非治理。** `ctos` 不改写 skill、不评判其质量、也不模拟完整的客户端注入上下文。它只度量体积与一个固定的开销常量。清晰的边界让工具活得更久。
- **确定且锁定。** 对给定的 `Cargo.lock`，token 计数可复现。golden 测试对 fixture 计数做快照，使 tokenizer 升级不会悄悄改变你的数字。
- **对坏输入绝不中断。** 不可读的文件被跳过，坏编码被有损解码，`INVALID` 的 skill 就地报告，其余部分照常完成。

---

## 许可证

GPL-3.0。见 [LICENSE](LICENSE)。
