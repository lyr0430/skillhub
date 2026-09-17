---
name: skill-install-mode-20260916-01
created: 2026-09-16
status: draft
---

# 需求提案：技能安装方式设置（软链接/复制）与本地技能页增强

## 背景

桌面端（Tauri）当前的技能安装**只有一种形态：复制**。「本地技能」页能正确识别并安全处理软链接（`LocationKind`、`remove_location`、`install.rs` 的 Symlink contract 均已实现），但**没有任何入口能把技能安装为软链接**。由此产生四类问题。

### 1. 安装形态不可选，软链接能力只能靠用户手工搭建

- `install_skill()`（`web/src-tauri/src/installer/install.rs:307`）下载 zip → 解压到临时目录 → `swap_into_place()` 落盘，落盘结果**恒为真实目录**。
- 代码里已完整具备「通过链接更新」的能力（`install.rs:301` 明确写着 *Symlink contract: when the target entry is a symlink, the bytes are written to the link's target*），也具备「卸载只删链接」的能力（`link.rs` `remove_location()` 显式分支、拒绝把链接交给 `remove_dir_all`）。
- 但用户环境里 `~/.claude/skills/` 下的大量链接（指向 `~/.cc-switch/skills`、`~/.agents/skills`、工作区路径）**全是手工或第三方工具建的**。产品自身不提供这条路，等于把已验证的能力锁在代码里。

### 2. 同一技能装到多个 agent 会各自漂移

复制模式下，`~/.claude/skills/x` 与 `~/.codex/skills/x` 是两份独立副本。升级一处不会影响另一处，用户需要逐个更新，且**版本是否一致完全不可见**（问题 4）。

### 3. `.skillhub` 中央仓库在「本地技能」页不可见

软链接模式需要一处存放技能真实文件。约定为 `~/.skillhub/skills/<slug>/`。但目前：

- 扫描逻辑 `local_skills.rs:120` **跳过所有以 `.` 开头的条目**，仓库目录天然不可见。
- 分类维度只有 `AGENT_PROFILES`（`agents.rs`）里的 5 个 agent（`generic`/`claude-code`/`codex`/`opencode`/`openclaw`），没有仓库这一维。
- 后果：用户无法确认「技能是否已在仓库中」，也无法理解一个链接指向何处。

### 4. 卡片只显示本地版本，无法判断是否该升级

`LocalSkillCard`（`web/src/features/skill/local-skill-card.tsx`）当前只渲染本地 metadata 里的版本。存在两个具体缺陷：

- **看不出线上有更新版本**：用户必须自己打开网页搜索同名技能比对。
- **同名但不同源时无出路**：本地是手工拷贝的、或来自另一个 namespace 的同名技能，与线上版本不一致时，用户没有任何「用线上版本覆盖」的操作入口，只能先卸载再安装。

### 5. 缺少安装方式的设置入口

安装方式（软链接/复制）是**跨会话持续生效的偏好**，需要一个持久入口。当前无任何设置承载位：`web/src/app/layout.tsx` 顶栏只有主题切换（`web/src/shared/components/theme-toggle.tsx`），`settings/*` 路由均为登录后页面。

## 目标

### G1. 提供「共享安装」与「独立副本」两种安装方式，默认共享

- 在 `install_skill` 路径上新增安装方式分支：
  - **共享安装（软链接）**：真实文件落 `~/.skillhub/skills/<slug>/`，agent 目录下创建指向它的符号链接。
  - **独立副本（复制）**：保持现状，文件直接落 agent 目录。
- **默认 = 共享安装**。
- 复用既有链接安全不变量，不得新增写链接路径的代码路径（必须经 `resolve_real_dir()`）：
  - 更新写**真实目录**，链接与指向关系保持不变。
  - 卸载软链接模式技能时**只删链接**；真实文件仍在仓库中，需另行处理（见非目标）。

### G2. 顶栏新增设置入口（齿轮），Tauri 可用、无需登录

- 位置：页面右上角，**主题切换按钮右侧**（`layout.tsx` 顶栏）。
- 形态：点击弹出浮层，**左侧菜单列表 + 右侧设置项**；当前仅「Skill 配置」一个菜单，结构需为后续多菜单预留。
- **仅 Tauri 环境可见**：用 `window.__TAURI__?.core?.invoke` 判定（与既有客户端检测方式一致），Web 端不渲染该入口，也不暴露对应路由。
- **无需登录**。不得复用 `settings/*` 的登录守卫。
- 设置项：技能安装方式 switch（共享安装 / 独立副本）。
- **切换只影响后续安装**，不改动、不迁移、不删除任何已安装技能。UI 必须明确写出这一点。

### G3. `.skillhub` 作为展示分类，与 agent 平级

- 在「本地技能」页的分类维度中加入 `.skillhub` 仓库，与 `claude-code`、`codex`、`opencode` 等**平级展示**。
- 语义**仅限展示**：列出仓库中存在的技能及其被哪些 agent 链接引用。`.skillhub` **不可被选为安装目标**（不能「只存进仓库而不链到任何 agent」）。
- 仓库分类下的破坏性操作范围需明确（孤儿技能清理等）——**本变更不做**，见非目标。

### G4. 卡片展示线上版本，并提供更新 / 替换操作

- 卡片在本地版本之外，展示**线上同名技能的版本号**。
- **分层匹配**（本地不一定有 `namespace`）：
  - 有 metadata（`origin = managed`）→ 按 `(namespace, slug)` 精确匹配。
  - 无 metadata（`origin = unmanaged`）→ 只按 `slug` 模糊匹配。
- **按来源区分两个操作**：

  | 操作 | 触发条件 | 语义 |
  |------|---------|------|
  | **更新** | 同源（managed 且 namespace+slug 命中）且线上版本 > 本地版本 | 增量升级，保留来源可追溯性 |
  | **替换** | 同名但来源不同（unmanaged 拷贝，或 namespace 不同）且版本不一致 | 整体覆盖为线上版本 |

- 条件不满足时**不渲染对应按钮**（不置灰、不占位）。线上无同名技能时，不展示线上版本信息。

## 非目标

- **不做**孤儿技能清理 / 仓库垃圾回收（无任何 agent 链接的技能如何清理、何时清理）。G3 仅展示。**需单独开 change**。
- **不做**已安装技能的批量迁移（不提供「把现有复制型技能转换为共享型」的一键操作）。
- **不做** Web 端的设置入口或安装方式选择（仅 Tauri）。
- **不做** CLI（`cli/`）侧的安装方式设置。CLI 的 `~/.skillhub`（工作区根，`cli/src/platform/paths.ts:5`）语义保持不变。
- **不做**「两个安装按钮」方案（原始设想中已被设置项取代，明确废弃）。
- **不做**按 agent 分别设置安装方式（单一全局开关）。
- **不做**现有 `settings/*` 页面的改动。

## 已澄清的决策

| # | 决策点 | 结论 |
|---|--------|------|
| D1 | `.skillhub` 分类语义 | **只作展示分类**，不可选为安装目标 |
| D2 | 中央仓库磁盘路径 | **`~/.skillhub/skills/<slug>/`**，与 CLI 的 `namespace-sync.json` 不同层，不干扰 |
| D3 | 版本匹配键 | **分层匹配**：managed 按 `(namespace, slug)`，unmanaged 仅按 `slug` |
| D4 | 更新 vs 替换 | **按来源区分**：同源且线上更高→更新；同名不同源且版本不一致→替换 |
| D5 | 设置作用域 | **全局单开关**（非 per-agent）——已确认 |
| D6 | 设置形态 | **浮层**（左菜单 + 右内容），非独立路由页面——已确认 |

### 命名建议（原需求请求斟酌）

「软链接 / 文件复制」是实现术语，用户真正关心的是**后果**：改一处是否影响所有 agent。建议主文案表达后果、副文案补充实现：

| 模式 | 主文案 | 副文案 | 说明文案 |
|------|--------|--------|---------|
| 软链接 | **共享安装** | 软链接到 .skillhub | 技能只存一份，多个 agent 共用。更新一处，所有引用它的 agent 同步生效。 |
| 复制 | **独立副本** | 复制文件到 agent | 每个 agent 各存一份独立副本，互不影响。占用空间更多，升级需逐个进行。 |

## 利益相关方

- **桌面端用户**：安装方式的直接使用与切换者。
- **多 agent 用户**：同一技能装到 claude-code / codex / opencode 多处，是共享安装的主要受益者。
- **平台维护者**：需理解 `.skillhub` 仓库目录的运维含义。
- **CLI 用户**：`~/.skillhub` 语义不得被本变更改变，需回归验证。

## 验收标准

1. 设置齿轮在 Tauri 端出现在主题切换右侧；Web 端**完全不渲染**，且不存在对应路由。
2. 设置浮层无需登录即可打开；左菜单含「Skill 配置」，右内容含安装方式 switch，默认选中「共享安装」。
3. 默认状态下安装技能：真实文件出现在 `~/.skillhub/skills/<slug>/`，agent 目录下为指向它的符号链接。
4. 切换为「独立副本」后安装：agent 目录下为真实目录，`~/.skillhub` 中**不新增**该技能。
5. 切换安装方式后，**已安装技能的文件形态、版本、数量均不变**（前后快照一致）。
6. 共享模式下更新技能：链接保持为链接，真实目录内容更新；同一技能的其他 agent 链接同时生效。
7. 共享模式下卸载：**只删除链接**，`~/.skillhub/skills/<slug>/` 真实文件仍在。
8. 本地技能页出现 `.skillhub` 分类，与 agent 分类平级；列出仓库中技能；该分类**不可被选为安装目标**。
9. 卡片展示线上同名技能版本号；线上版本 > 本地且同源时显示「更新」，否则不显示。
10. 同名不同源且版本不一致时显示「替换」，点击后本地技能被线上版本整体覆盖。
11. 线上无同名技能时，卡片不展示线上版本信息，也不出现任何新增按钮。
12. 本地技能页可将已有技能**安装/链接到其他 agent**。
13. Web 端不出现任何 Tauri 专属入口或安装方式设置。

## 风险

### R1. 直接触碰隐性契约第 10 节（最高风险）

`docs/architecture/implicit-contracts.md` §10 记录了两次真实事故（软链接不变量、serde 字段名、`metadata.json` 合并式写回、"受管"唯一定义）。本变更**正在修改软链接的创建路径**——必须复用既有不变量，不得绕开 `resolve_real_dir()` / `remove_location()`：

- 新建链接后，写入必须作用于真实目录，否则 `rename` 会把链接换成真实目录并使其脱离目标。
- "受管"判定必须继续共用同一函数（`has_metadata` / `read_metadata_lenient`），不得新增第三处定义。

### R2. 仓库路径与既有 `.skillhub` 含义的潜在混淆

`~/.skillhub/skills/<slug>/` 与技能目录内的 `<skill_dir>/.skillhub/metadata.json` 会形成嵌套（`~/.skillhub/skills/<slug>/.skillhub/metadata.json`）。需在 design 阶段确认：

- 扫描仓库分类时，`~/.skillhub/namespace-sync.json`（CLI 工作区文件）必须被排除，不得被当成技能。
- 需回归验证 CLI 读写 `~/.skillhub/namespace-sync.json` 不受影响。

### R3. 现有手工软链接的兼容

用户环境中已存在指向 `~/.cc-switch/skills`、工作区路径等**外部**目标的链接。本变更只负责**新建**指向仓库的链接；对此类既有链接的更新/卸载必须继续走既有安全分支（`ForeignSymlink` / 非受管），不得因新增仓库概念而误判。

### R4. 版本对比引入网络依赖与不可用降级

卡片展示线上版本需查询 registry。设计阶段需明确：

- 是否复用既有批量查询接口，还是逐技能请求（后者有 N+1 风险）。
- registry 不可达 / 未配置时的降级：**静默不显示线上版本**，不得让卡片报错或阻塞本地技能页渲染。

### R5. 范围偏大，建议拆分 —— 已决策：拆分

本变更含 4 个相对独立的能力。`/harness-plan` 阶段已评估，**结论：拆分交付**。

| 切片 | 内容 | 落点 |
|------|------|------|
| 切片一 | 设置入口 + 安装方式偏好 + 共享安装 + **已有技能添加到其他 agent（验收 12）** | **本 change**（`design.md` / `tasks.md`） |
| 切片二 | `.skillhub` 展示分类（G3，验收 8） | 独立 change，待切片一完成后 propose |
| 切片三 | 线上版本对比 / 更新·替换（G4，验收 9–11） | 独立 change |

验收 12 原属「本地技能页拓展」的独立诉求，因其与共享安装**共用建链接能力**（拆开要写两遍），故并入切片一，而非归入 G3/G4。

本 change 覆盖验收 **1–7、12、13**；验收 8–11 属切片二/三。

## 备注

- 原需求曾提出「弹窗内两个安装按钮」，随后用户明确改为设置项 switch，**两个按钮方案已废弃**，不进 tasks。
- 现有可复用资产：`LocationKind` / `classify` / `remove_location` / `ensure_under_roots`（`link.rs`）、`resolve_install_target` / `swap_into_place`（`install.rs`）、`AgentTarget` / `AGENT_PROFILES`（`agents.rs`）、`LocalSkillCard` 与 `Link2/Link2Off` 图标位。
- 受保护路径（`server/`、`sql/`、`deploy/`、`infra/`、`*.yml`）**本变更不涉及**。
- 主要改动面：`web/src-tauri/src/installer/*`（Rust）、`web/src/features/skill/*`、`web/src/pages/dashboard/local-skills.tsx`、`web/src/app/layout.tsx`、`web/src/shared/components/*`（新增设置浮层）。
