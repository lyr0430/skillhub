---
name: skill-install-mode-20260916-01
type: design
status: draft
created: 2026-09-16
covers: G1, G2 (install-mode preference + settings entry + install branch) + 验收 12 (已有技能链接到其他 agent)
defers: G3 (.skillhub 展示分类), G4 (线上版本对比 / 更新·替换) — 拆为独立 change
---

# 设计：技能安装方式偏好与共享安装

## 范围与切分

proposal 含 4 个相对独立的能力，R5 建议拆分。**已确认按 R5 拆分**，本 change 只交付**切片一**：

1. **持久化的安装方式偏好**（共享安装 / 独立副本，全局单开关，默认共享）。
2. **Tauri 端设置入口**（齿轮 + 浮层，无需登录，Web 不渲染）。
3. **安装主链路的安装方式分支**（共享安装落 `~/.skillhub/skills/<slug>/` 并建链接；独立副本保持现状）。
4. **已有技能链接到其他 agent**（proposal 验收 12）——与第 3 点共用建链接能力，拆开要写两遍，故并入本切片。

**拆出为独立 change（不在本 change 实现）**：

- **切片二**：`.skillhub` 展示分类（G3，验收 8）。依赖本切片先产生仓库内容，故排在其后。
- **切片三**：线上版本对比与更新/替换按钮（G4，验收 9–11）。含 registry 查询与网络降级（R4），与前两者低耦合，可独立推进。
- 孤儿技能清理、批量迁移：proposal 非目标，仍不做。

本文不为切片二/三定义实现，仅在数据结构上不做阻断它们的设计。
本切片覆盖 proposal 验收 **1–7、12、13**。

## 现状锚点（已读代码）

| 资产 | 位置 | 本设计如何使用 |
|------|------|---------------|
| `install_skill()` | `web/src-tauri/src/installer/install.rs:307` | 新增安装方式分支的唯一改动点 |
| `resolve_install_target()` | `install.rs:126` | 共享模式下对**已存在链接**的写穿契约不变，直接复用 |
| `swap_into_place()` | `install.rs:214` | 落盘逻辑不变，共享模式只改变「落到哪个目录」 |
| `resolve_real_dir()` | `link.rs` | 共享模式建链接后，一切写入仍经此解析真实目录 |
| `ensure_under_agent_root()` | `link.rs` | agent 目录边界校验不变 |
| `skill_dir()` / `AGENT_PROFILES` | `agents.rs` | 复用；新增仓库根解析函数与之并列 |
| `is_skillhub_sibling` / `SIBLING_TAGS` | `metadata.rs` | 仓库扫描排除规则的既有形状，后续切片复用 |
| `use-theme` → `readStoredTheme`/`saveTheme` | `shared/hooks/use-theme.ts`, `shared/lib/theme.ts` | 偏好持久化的既有范式，安装方式偏好照此实现 |
| `isTauri()` | `shared/lib/tauri.ts` | 设置入口的 Tauri 判定，用 `__TAURI_INTERNALS__` |
| `InstallSkillInput` | `features/skill/tauri-installer.ts` | 新增 `installMode` 字段的落点 |

## 一、安装方式偏好（前端持久化）

安装方式是跨会话偏好，与主题同层，**不入后端、不入账号**。照 `use-theme` 的范式实现，不引入 zustand。

```
shared/lib/install-mode.ts
  export type InstallMode = 'shared' | 'copy'
  export const DEFAULT_INSTALL_MODE: InstallMode = 'shared'   // 默认共享
  readStoredInstallMode(): InstallMode | null   // localStorage，解析失败→null
  saveInstallMode(mode: InstallMode): void
  resolveInstallMode(): InstallMode             // 存储值，否则默认值

shared/hooks/use-install-mode.ts
  useInstallMode() → { mode, setMode }          // 结构对齐 useTheme
```

存储键 **`skillhub-install-mode`**（`skillhub-` 前缀，与既有 `skillhub-theme` 一致）。读取无效值时回退 `DEFAULT_INSTALL_MODE`，不抛错（对齐 `readStoredTheme() ?? DEFAULT_THEME`）。

**`resolveInstallMode()` 的存在理由**（实现期补充）：overlay 与安装按钮是两个互不重渲染的组件。安装动作若闭包捕获 `useInstallMode()` 的 state，拿到的是**挂载时**的值——用户改完模式立刻安装会用到旧模式。因此**动作处理器必须在点击时调用 `resolveInstallMode()` 实时读取**；`useInstallMode()` 仅供 overlay 自身渲染。

**作用域**：全局单开关（D5）。**只影响后续安装**，不触碰已安装技能——这是硬约束，UI 文案必须写明（验收 5）。切换偏好只写 localStorage，不发起任何 Tauri 调用。

## 二、设置入口（Tauri only）

### 渲染门控

`layout.tsx` 顶栏，主题切换按钮**右侧**新增齿轮。`isTauri()` 为假时**整个入口不渲染**——不是隐藏、不是置灰、不注册路由（验收 1、13）。不复用 `settings/*`，不经登录守卫（验收 2）。

```
shared/components/settings-trigger.tsx   # 齿轮按钮（受控：open / onToggle）
shared/components/settings-dialog.tsx    # 居中弹窗（遮罩 + 左菜单 + 右内容）
```

**实现期修正（三处，最后一条推翻了最初的方案）**：

1. **拆为两个文件**，且由 `Layout` 持有 open 状态。触发器在 header 内，弹窗在 header **之外**——原因见第 3 条。

2. **弹窗形态：居中 modal + 遮罩，不是 header 内联浮层**。尺寸放宽到 `min(calc(100vw-2rem), 46rem)`，左菜单 12rem。初版做成了 header 内联浮层，视觉上不符合「居中弹窗」的预期。

3. **必须在 header 之外渲染**（这条推翻了初版「用 in-tree absolute」的做法）。header 带 `backdrop-blur-xl`，而非 `none` 的 `backdrop-filter` 会使其成为 `position: fixed` 后代的**包含块**——在 header 内声明的居中弹窗会相对 header 盒子居中，而非视口。故：open 状态提升到 `Layout`，弹窗由 `Layout` 渲染（header 之外），用 Radix `Dialog`。至于「顶栏内联浮层不能用 portal」的既有约束仍成立，但它约束的是**内联浮层**，不适用于渲染在 header 之外的弹窗。

4. **去掉 switch**：两个选项本就是单选列表，选中态已可见；再加一个 switch 会让同一状态有两个控件、可能互相矛盾。改用 `role="radiogroup"` + `role="radio"` + `aria-checked` 的单选语义。

5. **右侧内容按模块分组**：抽出 `SettingsSection`（标题 + 可选描述 + 分隔线），后续新增配置项即新增一个 section，不必各自发明间距与标题层级。当前两个模块：安装方式、切换行为。

6. **左菜单选中态用实心 pill**（`bg-primary text-primary-foreground`）。浅色主题 `--primary` 是 `0 0% 13%`（近黑），`text-primary` 与正文无差异、`bg-primary/12` 几乎不可见——淡色方案在这里天然失效。实心 pill 与顶栏导航选中态的做法一致，且深浅色主题都能自动反转（深色下 primary 为近白）。

### 浮层结构

左菜单列表 + 右设置项（D6）。当前仅「Skill 配置」一项，但菜单为数组渲染，为后续多菜单预留（不写死单项 JSX）。右侧内容：安装方式 switch。

文案按 proposal「命名建议」表——主文案表后果，副文案表实现：

| mode | 主文案 | 副文案 | 说明 |
|------|--------|--------|------|
| shared | 共享安装 | 软链接到 .skillhub | 技能只存一份，多 agent 共用，更新一处全部生效 |
| copy | 独立副本 | 复制文件到 agent | 每个 agent 各存一份，互不影响，升级需逐个进行 |

switch 下方固定一行说明：**切换只影响后续安装，不改动已安装技能**（验收 5 的可见承诺）。

## 三、安装主链路分支（Rust）

### 输入扩展

`InstallInput`（`install.rs`）新增：

```rust
/// 安装方式。缺省视为 shared，与前端默认一致，且兼容旧版前端不传该字段。
#[serde(default)]
pub install_mode: InstallMode,   // serde: "shared" | "copy"
```

`InstallMode` 定义为独立 enum，`#[serde(rename_all = "kebab-case")]`，`Default` = `Shared`。前端 `InstallSkillInput` 同步加 `installMode?: 'shared' | 'copy'`。

> 隐性契约 §10：serde 字段名是契约。新增字段用 `#[serde(default)]` 兜底，旧前端（不传 `installMode`）仍走 shared，不 panic。

### 目标解析：仓库根

`agents.rs` 新增，与 `profile_root` 并列：

```rust
/// 共享安装的中央仓库：~/.skillhub/skills/<slug>/  (D2)
pub fn repo_skill_dir(slug: &str) -> PathBuf {
    home_dir().join(".skillhub").join("skills").join(slug)
}
```

`~/.skillhub/namespace-sync.json` 是 CLI 工作区文件，与 `~/.skillhub/skills/` **不同层**（D2、R2）——本 change 只写 `skills/` 子目录，不读不写 `namespace-sync.json`，CLI 语义零改动（验收：CLI 回归）。

### 分支逻辑

`install_skill()` 内，在解析 `target_path`（agent 目录下的条目路径）之后分流：

**copy 模式**（现状，零改动路径）：
- `target_path` 即 `skill_dir(profile, slug)`，`resolve_install_target` → `swap_into_place` 落真实目录。

**shared 模式**：
1. 真实文件目标 = `repo_skill_dir(slug)`。下载、解压、`swap_into_place` **全部作用于仓库目录**——即把现有落盘链路的目标从 agent 目录换成仓库目录，`write_metadata` 也写仓库目录内的 `.skillhub/metadata.json`。
2. 落盘完成后，在 agent 目录 `target_path` 处建**指向仓库真实目录的符号链接**。
3. 建链接前对 `target_path` 现状分类：
   - 已是指向本仓库目录的链接 → 幂等，跳过重建。
   - 已是**其他链接**（`ForeignSymlink` / 指向外部目标，如 `~/.cc-switch/skills`）→ 不覆盖，返回 warning，让用户经卸载走既有安全分支（R3）。
   - 已是真实目录 → 按 `preserve_existing` 决定备份让位（复用 `swap_into_place` 已有的 preserve 语义，不新写一套）后建链接。
   - 不存在 → 直接建链接。

> **关键不变量（隐性契约 §10 / R1）**：shared 模式下写入的目标**永远是 `repo_skill_dir` 这个真实目录**，绝不写穿链接路径。链接是落盘**之后**才建的独立步骤。这样 `swap_into_place` 内部的 `rename` 作用于真实目录，不会把链接换成真实目录。既有「更新已存在链接」的写穿契约（`resolve_install_target` 对 symlink 分支）保持不变——那条路径处理的是「agent 目录里已经有链接、要更新其 target」，与 shared 首次安装建链接是两件事，不合并。

### 建链接的安全边界

新建链接调用必须复用既有校验，不新增绕过 `resolve_real_dir` 的写链接路径（R1）：
- 链接落点经 `ensure_under_agent_root` 校验（与 uninstall 同一边界）。
- 链接 target = `repo_skill_dir` 的 canonical 路径（真实目录，非再套一层链接）。
- 平台：`std::os::unix::fs::symlink`；Windows 用 `std::os::windows::fs::symlink_dir`。以 `#[cfg]` 分平台，失败返回可读 `InstallError` 而非 panic。

### 卸载（本切片不改，仅确认）

共享模式技能的卸载走既有 `remove_location`：链接只被 unlink，`~/.skillhub/skills/<slug>/` 真实文件保留（验收 7）。仓库中孤儿真实目录的清理是 G3 之后的独立 change，本切片不做。

## 四、已有技能链接到其他 agent（验收 12）

本地技能页在**已安装**的技能卡片上，提供「添加到其他 agent」——把该技能在另一个 agent 下也变为可用。**纯本地操作，不下载、不查 registry**，因此对 unmanaged 技能同样可用（它们没有 registry 来源）。

### 与安装的区别

| | 安装（第三节） | 添加到其他 agent（本节） |
|---|---|---|
| 内容来源 | registry 下载 zip | 本地已有真实目录 |
| 需要 registry | 是 | 否 |
| 对 unmanaged 技能 | 不适用 | **适用** |

### 命令

```rust
#[tauri::command]
pub fn attach_skill_to_agent_command(
    source_dir: String,
    agent: String,
    mode: InstallMode,
) -> CommandResult<AttachResult>
```

行为按 mode 分派（与偏好一致，前端把 `useInstallMode().mode` 传下来）：

- `shared` → 在目标 agent 目录建**指向源真实目录**的符号链接。
- `copy` → 把源真实目录**递归复制**到目标 agent 目录。

### 校验边界（重要，易写错）

`source_dir` 来自 web view，必须校验——但**不能复用 `ensure_under_agent_root`**：共享安装产出的真实目录是 `~/.skillhub/skills/<slug>/`，位于任何 agent root **之外**；用 agent-root 边界校验会把合法的共享技能源全部拒掉。正确的源校验是：

- `fs::canonicalize(source_dir)` 必须成功且为目录；
- 必须通过 `is_skill_package()`（存在 `SKILL.md`）——与 `resolve_install_target` 用于「是否允许写穿链接」的是**同一个谓词**，保持一处定义。

目标侧仍须是 agent root 之下的条目：目标路径由 `skill_dir(profile, slug)` 构造（构造即安全），`slug` 取**源目录名**（`file_name`），使技能在页面上的显示名与目录名一致。

### 目标现状分类

建链接/复制前对目标条目分类，与第三节同表：

- 不存在 → 执行。
- 已是指向**同一源目录**的链接 → 幂等，直接返回成功（不重建）。
- 已指向**其他目标**的链接，或已是真实目录 → **不覆盖**，返回 warning，提示用户先卸载该 agent 下的条目。破坏性让位不在本操作里做。

> 复用第三节的建链接 helper 与 `classify`，不新写一套判定（R1：不新增第二处「受管/形态」定义）。

### 前端

卡片操作区新增「添加到其他 agent」，仅在存在其他 agent 时出现。点击弹出 agent 多选（排除已持有该技能的 agent），确认后逐个调用命令，成功后 `refresh({ silent: true })`。

## 五、数据流

```
用户在浮层选安装方式 → saveInstallMode(localStorage)
                              │
安装按钮点击 → 读 useInstallMode().mode → InstallSkillInput.installMode
                              │
invoke('install_skill_command', { input, registry })
                              │
install_skill(): match install_mode
   ├─ Copy   → 现状：落 agent 目录
   └─ Shared → 落 ~/.skillhub/skills/<slug>/  → 在 agent 目录建链接
```

## 六、不变量与回归清单

- [ ] shared 安装：真实文件在 `~/.skillhub/skills/<slug>/`，agent 目录为指向它的链接（验收 3）。
- [ ] copy 安装：agent 目录为真实目录，仓库**不新增**该技能（验收 4）。
- [ ] 切换偏好后已装技能形态/版本/数量前后快照一致（验收 5）。
- [ ] shared 更新：链接仍是链接，真实目录内容更新，多 agent 链接同时生效（验收 6）。
- [ ] shared 卸载：只删链接，真实文件保留（验收 7）。
- [ ] 指向外部目标的既有手工链接不被 shared 安装覆盖，走 warning（R3）。
- [ ] `~/.skillhub/namespace-sync.json` 读写不受影响，CLI 回归通过（R2）。
- [ ] 旧前端不传 `installMode` 时，后端按 shared 处理不 panic（serde 契约）。
- [ ] 添加到其他 agent：源为 `~/.skillhub/skills/<slug>`（agent root **之外**）时不被边界校验误拒（验收 12）。
- [ ] 添加到其他 agent：源缺 `SKILL.md` 时拒绝。
- [ ] 添加到其他 agent：目标已存在（其他链接或真实目录）时不覆盖，返回 warning。
- [ ] 添加到其他 agent：目标已是指向同一源的链接时幂等，不报错不重建。
- [ ] Web 端无齿轮、无安装方式设置、无对应路由（验收 1、13）。

## 七、测试策略

- **Rust 单测**：仿 `link.rs` / `local_skills.rs` 的 temp-tree 模式，新增 `repo_skill_dir` 目标下的 shared 安装测试——断言真实目录落点、链接存在且 target 正确、copy 模式仓库不新增。对「已存在外部链接」用例断言不覆盖 + warning。
- **Rust 单测（attach）**：源指向 agent root 之外的 temp 目录（模拟 `~/.skillhub/skills/x`）断言**通过**校验；源无 `SKILL.md` 断言拒绝；目标已占用断言 warning 且目标未被改动；同源已链接断言幂等；copy 模式的递归复制断言文件齐全。
- **前端单测**：`install-mode.ts` 读写 + 无效值回退；`settings-overlay` 在 `isTauri()` 真/假下的渲染门控（仿 `local-skills.test.tsx` mock `invokeTauri`）；卡片「添加到其他 agent」在无可选 agent 时不渲染。
- **契约测试**：旧 payload（无 `installMode`）反序列化为 `Shared`。

## 八、决策状态

D5（全局单开关）、D6（浮层形态）**均已确认**，与本设计的实现一致，无悬置项。
