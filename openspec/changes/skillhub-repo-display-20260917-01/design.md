---
name: skillhub-repo-display-20260917-01
type: design
status: designed
created: 2026-09-17
parent: skill-install-mode-20260916-01
covers: G1（.skillhub 展示分类）+ G2（引用关系）+ 仓库卸载
scope_note: 父提案 G3 定义为「纯展示」，仓库卸载明确推给独立 change。本 change 经用户批准，把「仓库卸载」纳入切片二——需同步修订父提案非目标，避免被当作范围漂移。
---

# 设计：`.skillhub` 仓库展示分类与仓库卸载

## 范围与切分

本 change 交付**切片二**：

1. `.skillhub` 作为展示分类，与 agent 平级（G1，验收 1-3、9-10）。
2. 引用关系展示：一个共享技能被哪些 agent 链接（G2，验收 4-5）。
3. **仓库卸载**：从 `.skillhub` 分类删除技能真实目录 + 所有指向它的链接（验收 6-8）。

**范围扩展说明（重要）**：父提案 `skill-install-mode-20260916-01` 的 G3 与非目标把仓库分类限定为「仅展示」，并将「孤儿清理 / 仓库垃圾回收」推给独立 change。本 change 经用户明确批准，把**仓库卸载**纳入切片二。这不是孤儿清理的全部（不做「自动扫描并批量清理无引用技能」），而是**用户主动对某个仓库技能执行的删除**。Task 5.3 需同步修订父提案非目标一行，标注此决策来源。

**仍不做**（留待后续独立 change）：
- 孤儿技能自动清理 / 批量垃圾回收。
- 批量迁移（复制型 → 共享型）。
- `.skillhub` 作为安装目标（不能「只存仓库不链 agent」）。
- 线上版本对比 / 更新·替换（切片三）。

## 现状锚点（已读代码）

| 资产 | 位置 | 本设计如何使用 |
|------|------|---------------|
| `scan_roots()` | `local_skills.rs:100` | 新增 `.skillhub` 合成源，复用现有遍历 |
| `aggregate()` | `local_skills.rs:174` | **不改**：已按 canonical 真实路径聚合，仓库项与 agent 链接自然归并 |
| `build_skill()` | `local_skills.rs:216` | 扩展：填充 `repo_managed` / `linked_agents` 派生字段 |
| `is_skillhub_sibling` | `metadata.rs` | 仓库内 backup/tmp 兄弟目录的既有排除规则，复用 |
| `repo_root()` / `repo_skill_dir()` | `agents.rs:82`（切片一新增） | 仓库根解析，本 change 直接用 |
| `classify()` / `remove_location()` | `link.rs` | 卸载链接侧复用（只删链接的既有安全分支） |
| `ensure_under_roots()` | `link.rs:187` | 链接侧边界校验复用；**真实目录侧不用它**（仓库在 agent root 之外） |
| `LocalSkill` | `local_skills.rs:49` | 新增两个派生字段 |
| `local-skills.tsx` 分类栏 | `web/src/pages/dashboard/` | 新增 `.skillhub` 分类按钮 |
| `LocalSkillCard` symlink 徽章 | `local-skill-card.tsx:79` | 引用徽章照此形状 |

## 一、扫描建模：`.skillhub` 作为第六个合成源（方案 A）

**核心决策**：不新建独立 `RepoEntry` 结构与第二套聚合逻辑，而是把 `~/.skillhub/skills/` 以合成 agent id `.skillhub` 加入 `scan_roots` 的输入。理由:

- 现有 `aggregate()` **已按 canonical 真实路径归并**。agent 链接 `~/.claude/skills/weather` canonicalize 后是 `~/.skillhub/skills/weather`；仓库项 `~/.skillhub/skills/weather` 的 canonical 也是它自己。二者**自然聚合到同一个 `LocalSkill`**。
- 复用久经考验的聚合与「受管/形态唯一定义」，不引入第二处「谁指向谁」的判定（隐性契约 §10 精神）。

### 扫描入口

```rust
// scan_local_skills(): 现有 5 个 agent + 1 个合成仓库源
pub const REPO_AGENT_ID: &str = ".skillhub";

pub fn scan_local_skills() -> (Vec<LocalSkill>, Vec<String>) {
    let mut roots: Vec<(String, PathBuf)> = AGENT_PROFILES
        .iter()
        .map(|p| (p.id.to_string(), profile_root(p)))
        .collect();
    // 合成源：仓库根。其子目录都是真实技能目录（非 . 开头），不会被 dot 过滤。
    roots.push((REPO_AGENT_ID.to_string(), repo_root()));
    scan_roots(&roots)
}
```

`scan_roots` 与 dot 过滤、`is_skillhub_sibling` 过滤**均不改**：仓库下的技能目录名（`weather`）不以 `.` 开头,不会被跳过;而 `~/.skillhub/namespace-sync.json`（CLI 工作区文件）在 `repo_root()`（即 `~/.skillhub/skills/`）**之外**,根本不在这次遍历里。

### 派生字段（`build_skill` + 聚合后一次性回填）

```rust
pub struct LocalSkill {
    // ...既有字段不变...
    /// 该技能是否存在于 ~/.skillhub/skills/ 仓库中。
    /// = locations 里是否有 agent == ".skillhub" 的条目。
    pub repo_managed: bool,
    /// 指向该仓库技能的 agent（不含合成的 ".skillhub" 本身）。
    /// 仅当 repo_managed 时有意义；非仓库技能为空。
    pub linked_agents: Vec<String>,
}
```

`repo_managed` / `linked_agents` 是**从 `locations` 派生**的，不是独立扫描来的：聚合结束后遍历每个 skill 的 locations，agent == `.skillhub` 者置 `repo_managed = true`，其余 agent 计入 `linked_agents`。放在 `aggregate()` 末尾的排序循环里一并算，零额外遍历成本。

> **不变量**：`.skillhub` 只是**展示用合成 agent id**，不进 `AGENT_PROFILES`、不参与安装目标解析、不出现在安装弹窗。它只出现在 location.agent 与分类筛选里。

## 二、前端分类层

### 分类顺序与渲染

`local-skills.tsx` 顶部分类栏，顺序 **`.skillhub` → `所有` → 各 agent**（D5：仓库语义优先）。现有 agent 按钮已是 `agents.map` 渲染；`.skillhub` 作为一个前置的固定项加入，不硬编码进循环。

- `.skillhub` 分类用仓库图标（`Package` / `FolderGit2`，lucide 现有），与 agent 品牌图标区分。
- 选中 `.skillhub` 时,`visibleSkills` 过滤 `skill.repoManaged === true`。

```ts
const visibleSkills = useMemo(() => {
  if (selectedAgent === REPO_CATEGORY_ID) {
    return skills.filter((s) => s.repoManaged)
  }
  if (!selectedAgent) return skills
  return skills.filter((s) => s.locations.some((l) => l.agent === selectedAgent))
}, [skills, selectedAgent])
```

### `.skillhub` 不可作为安装目标（验收 10）

安装入口在技能详情页（`InstallForAgentButton`），其 agent 列表来自 `detectAgents()` → `AGENT_PROFILES`,合成的 `.skillhub` 不在其中,**天然不会出现**在安装弹窗。本 change 只需**不把它加进 `AGENT_PROFILES`**——已满足。加一条测试断言即可（`detectAgents` 结果不含 `.skillhub`）。

## 三、引用关系展示

### 引用徽章（`LocalSkillCard`）

当 `linkedAgents.length > 0` 时,标题旁显示徽章,形状复用现有 symlink 徽章（sky blue，`Link2` 图标）：

- `linkedAgents.length === 1` → 「被 1 个 Agent 引用」
- `> 1` → 「被 N 个 Agent 引用」

### agent 列表

在现有 location chips 区,仓库技能额外显示一行引用的 agent（图标 + 名称,`·` 分隔）。复用现有 `AgentBrandIcon`。

### i18n（zh/en/ru）

- `localSkills.repoCategory` — ".skillhub 仓库" / "Repository" / "Репозиторий"
- `localSkills.refBadge` — "被 {{count}} 个 Agent 引用"（用 i18next 复数规则）
- `localSkills.repoUninstallTitle` / `repoUninstallDesc` — 卸载确认文案

## 四、仓库卸载（验收 6-8）

### 语义

从 `.skillhub` 分类卸载一个技能 = **删除真实目录 `~/.skillhub/skills/<slug>/` + 所有指向它的 agent 链接**。这与 agent 分类下「只删一个链接、保留真实目录」是相反语义,所以**必须独立命令**,不复用 `uninstall_location`（后者 `ensure_under_agent_root` 只认 agent root,会拒掉仓库路径——这是好事,不是障碍）。

### 命令

```rust
#[tauri::command]
pub async fn uninstall_repo_skill_command(slug: String)
    -> CommandResult<UninstallRepoResult>
```

`UninstallRepoResult { ok, removed_dir, removed_links: Vec<String>, warnings }`。

### 实现顺序（先删链接,后删真实目录）

1. `validate_slug(&slug)` + 构造 `repo_skill_dir(slug)`,确认它在 `repo_root()` 之内且为真实目录（`symlink_metadata` 非链接）。
2. `find_links_to_target(&real_dir)`：遍历 5 个 agent root,找出 canonical 指向 `real_dir` 的 symlink,逐个 `remove_file`（复用「只删链接」安全分支——显式 `is_symlink()` 判定后 `remove_file`,绝不 `remove_dir_all` 一个可能是链接的路径）。
3. 全部链接删完后,`remove_dir_all(&real_dir)` 删真实目录——**但仅当该目录是「受管」的**（`has_metadata` 可解析 `.skillhub/metadata.json`）。一个手放进去、无元数据的仓库目录会被**重命名备份**（`*.skillhub-backup-*`）而非删除,与 agent 路径对非受管目录的处置一致（隐性契约 §10「「受管」只有一个定义」）。评审发现无条件 `remove_dir_all` 会在 `.skillhub` 分类下删掉用户手放的目录且无备份。
4. **顺序理由**：先删链接再删目录,任一步失败时不会留下「目录已删、链接悬空」的中间态过久;若先删目录,链接会先成为悬空链接。删链接是幂等安全操作,放前面更稳。

```rust
/// 找出所有 agent root 下 canonical 指向 target 的链接。
/// 复用 classify + canonicalize,不新增形态判定。
pub fn find_links_to_target(target: &Path) -> Vec<(String, PathBuf)>
```

> **不变量（隐性契约 §10）**：删链接侧继续走「只删链接本身」;删真实目录是仓库侧独有操作,只作用于经 `repo_root()` 边界校验、`symlink_metadata` 确认是真实目录、且 `has_metadata` 确认是「受管」的路径,绝不写穿/删穿链接。链接发现失败（某 root 读不了/条目分类失败）不可静默吞掉,必须上报警告并保留真实目录,否则会「删了目录、留了个没发现的悬空链接」。

### 前端确认弹窗

`.skillhub` 分类下的卸载走**独立确认弹窗**（区别于 agent 分类的既有弹窗）,明确列出:

- 「将删除仓库真实目录:`~/.skillhub/skills/<slug>/`」
- 「并解除 N 个 agent 链接:claude-code · codex」

未确认不执行。确认后调 `uninstallRepoSkill(slug)`,成功 `refresh({ silent: true })`。

## 五、数据流

```
scan_local_skills(): 5 agent roots + repo_root() 合成源
        │  aggregate() 按 canonical 真实路径归并（既有逻辑,不改）
        ▼
LocalSkill { ..., repoManaged, linkedAgents }   ← 聚合末尾一次性派生
        │
前端分类栏: .skillhub → 所有 → 各 agent
        │  selectedAgent === '.skillhub' → filter(repoManaged)
        ▼
LocalSkillCard: 引用徽章 + agent 列表 + (仓库分类下) 仓库卸载按钮
        │
uninstall_repo_skill_command(slug)
        ├─ find_links_to_target → 逐个删链接（只删链接）
        └─ remove_dir_all(repo_skill_dir)   （真实目录,已边界校验）
```

## 六、不变量与回归清单

- [ ] `.skillhub` 合成源加入扫描,仓库技能出现在 `.skillhub` 分类（验收 1-3）。
- [ ] `.skillhub` **不在** `AGENT_PROFILES`,不出现在安装弹窗（验收 10）。
- [ ] 一个共享技能:`.skillhub` location + 各 agent link 聚合为**一个** `LocalSkill`（复用 aggregate,不重复出卡）。
- [ ] `repoManaged` / `linkedAgents` 派生正确:0/1/多个 agent 引用各场景。
- [ ] copy 模式装到 agent 的技能(真实目录、非链接)**不**聚合进仓库项,不误标 repoManaged。
- [ ] 仓库卸载:先删所有链接(只删链接)、后删真实目录;受影响 agent 在弹窗列出（验收 6-8）。
- [ ] 仓库卸载边界:`slug` 校验 + 仅作用于 `repo_root()` 之内的真实目录,不删穿链接。
- [ ] `~/.skillhub/namespace-sync.json` 不受影响（它在 `repo_root()` 之外,不进遍历）。
- [ ] Web 端无 `.skillhub` 分类(Tauri only,复用 `isTauri()` 门控)（验收 9）。

## 七、测试策略

- **Rust 单测（扫描）**：temp tree 造 `repo/skills/weather` + `agent/.claude/skills/weather → repo`,断言聚合为一个 skill、`repo_managed = true`、`linked_agents = ["claude-code"]`;0 引用（仓库有、无 agent 链接）与多引用场景。
- **Rust 单测（卸载）**：`find_links_to_target` 找到 0/1/2 个链接;`uninstall_repo_skill` 删链接 + 删真实目录后二者皆不存在;非法 slug / 仓库外路径被拒;真实目录侧不误删链接目标。
- **前端单测**：`.skillhub` 分类渲染与顺序、选中过滤 `repoManaged`;引用徽章文案（单数/复数）;仓库卸载确认弹窗列出受影响 agent;`isTauri()` 假时无分类。
- **回归**：现有 `local-skills.test.tsx` 全绿（分类栏改动不破坏既有 agent 筛选）。

## 八、决策状态

| # | 决策 | 结论 |
|---|------|------|
| D1 | 扫描建模 | **方案 A**：`.skillhub` 合成源 + 复用 `aggregate()` |
| D2 | 卸载范围 | **展示 + 仓库卸载**（删真实目录 + 所有链接）——经用户批准,超出父提案 G3「纯展示」,Task 5.3 同步修订父提案 |
| D3 | 分类顺序 | `.skillhub` → 所有 → 各 agent |
| D4 | 安装目标 | `.skillhub` 不进 `AGENT_PROFILES`,天然不可选为安装目标 |
| D5 | 卸载顺序 | 先删所有链接(只删链接),后 `remove_dir_all` 真实目录 |
