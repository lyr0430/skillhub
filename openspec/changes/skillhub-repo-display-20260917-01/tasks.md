---
name: skillhub-repo-display-20260917-01
status: planned
created: 2026-09-17
parent: skill-install-mode-20260916-01
scope: 切片二（.skillhub 展示分类 + 引用关系 + 仓库卸载）
---

# 执行任务：skillhub-repo-display-20260917-01

> 范围以 `design.md` 为准。建模用**方案 A**（`.skillhub` 合成源 + 复用 `aggregate()`）；卸载范围经用户批准为**展示 + 仓库卸载**，超出父提案 G3「纯展示」。
>
> 项目规则：只允许实现本文件列出的内容；任何行为变化必须补测试（先写测试，后写实现）。

## 任务列表

### Milestone 1：扫描层——`.skillhub` 合成源（复用聚合）

- [x] Task 1.1：`local_skills.rs` 新增 `REPO_AGENT_ID = ".skillhub"`；`scan_local_skills()` 把 `repo_root()` 以该 id 追加为第六个扫描源
  - 不改 `scan_roots` / dot 过滤 / `is_skillhub_sibling`：仓库技能目录名非 `.` 开头，不会被跳过
  - 测试：仓库源纳入扫描；`namespace-sync.json` 在 `repo_root()` 之外不进遍历
- [x] Task 1.2：`LocalSkill` 新增派生字段 `repo_managed: bool` / `linked_agents: Vec<String>`（`#[serde(rename_all=camelCase)]` 已在结构体上）
  - 测试：序列化为 `repoManaged` / `linkedAgents`；非仓库技能 `repo_managed=false`、`linked_agents` 空
- [x] Task 1.3：`aggregate()` 末尾一次性派生这两个字段（不新增遍历）
  - `repo_managed` = locations 含 `agent == ".skillhub"`
  - `linked_agents` = 其余指向它的 agent（排除合成 id 本身）
  - 测试：一个共享技能（仓库 + N agent 链接）聚合为**一个** LocalSkill；0/1/多引用；copy 模式真实目录不误标 `repo_managed`

### Milestone 2：前端分类层

- [x] Task 2.1：`local-skills.tsx` 新增 `REPO_CATEGORY_ID = '.skillhub'` 分类按钮，顺序 `.skillhub` → 所有 → 各 agent；用仓库图标区分
  - 测试：渲染顺序；选中态；点击回调；`isTauri()` 假时整个页面维持既有 desktop-only 行为
- [x] Task 2.2：`visibleSkills` 扩展——选中 `.skillhub` 时过滤 `repoManaged`
  - 测试：过滤逻辑；仓库为空时的空态
- [x] Task 2.3：`detectAgents()` 结果断言不含 `.skillhub`（保证不可作安装目标，验收 10）
  - 测试：`AGENT_PROFILES` 不含 `.skillhub`；安装弹窗 agent 列表无该项

### Milestone 3：引用关系展示

- [x] Task 3.1：`LocalSkillCard` 引用徽章——`linkedAgents.length > 0` 时显示，形状复用 symlink 徽章（sky blue + `Link2`）
  - 测试：渲染条件；单数/复数文案；`linkedAgents` 为空不渲染
- [x] Task 3.2：卡片列出引用的 agent（图标 + 名称，`·` 分隔），复用 `AgentBrandIcon`
  - 测试：渲染顺序；truncate
- [x] Task 3.3：i18n（zh/en/ru）新增 `repoCategory` / `refBadge`（复数）/ `repoUninstallTitle` / `repoUninstallDesc`
  - 测试：文案渲染；i18next 复数规则

### Milestone 4：仓库卸载

- [x] Task 4.1：`link.rs` 新增 `find_links_to_target(target) -> Vec<(String, PathBuf)>`——遍历 agent root 找 canonical 指向 target 的链接，复用 `classify` + canonicalize
  - 测试：找到 0/1/2 个链接；异源链接不误命中；悬空链接不误命中
- [x] Task 4.2：`installer` 新增 `uninstall_repo_skill(slug)`——校验 slug + `repo_skill_dir` 在 `repo_root()` 内且为真实目录；先逐个删链接（只删链接），后 `remove_dir_all` 真实目录
  - 拆纯函数 `uninstall_repo_skill_under(repo_root, agent_roots, slug)` 以便 temp tree 测试
  - 测试：删链接 + 删真实目录后皆不存在；非法 slug 拒绝；仓库外路径拒绝；真实目录侧不误删链接目标；部分链接缺失仍继续
- [x] Task 4.3：`commands.rs` 注册 `uninstall_repo_skill_command`（`spawn_blocking` + `CommandResult` 信封）+ `lib.rs` 注册
  - 测试：信封成功/失败路径
- [x] Task 4.4：`tauri-installer.ts` 新增 `uninstallRepoSkill(slug)` + `UninstallRepoResult`
- [x] Task 4.5：`local-skills.tsx` 仓库卸载确认弹窗（独立于 agent 卸载弹窗），列出真实目录 + 受影响 agent；确认后调用并 `refresh({ silent: true })`
  - 测试：弹窗列出受影响 agent；取消不调用；确认调用且参数正确；成功后刷新

### Milestone 5：验证与文档

- [x] Task 5.1：自动化验收——前端 `pnpm test` / `pnpm lint` / `pnpm typecheck`；Rust `cargo test` / `cargo clippy -D warnings`
- [x] Task 5.2：隐性契约 §10 复核——扫描新增合成源未破坏「按真实路径聚合」；卸载先删链接后删目录、只删链接不删穿；`.skillhub` 三义未混淆
- [x] Task 5.3：知识回写
  - **修订父提案** `skill-install-mode-20260916-01/proposal.md` 非目标：把「仓库卸载」从「纯展示，需单独 change」改为「已在切片二 `skillhub-repo-display-20260917-01` 实现」，标注决策来源
  - 更新 `docs/architecture/implicit-contracts.md` §10：`.skillhub` 合成扫描源、仓库卸载语义（删真实目录 + 所有链接，与 agent 卸载相反）

## 验收检查点

- [x] Rust 编译通过（`cargo check` / `cargo clippy -D warnings` 干净）
- [x] 前端类型检查通过（`pnpm typecheck`）+ lint 通过（`--max-warnings 0`）
- [x] Rust 单元测试通过
- [x] 前端单元测试通过
- [ ] 手工验收 proposal 标准 1-10 通过 —— 需在真实桌面应用中点选
- [x] 确认**未触碰**受保护路径（`server/`、`sql/`、`deploy/`、`infra/`、`application*.yml`、`db/`）
- [x] Review 通过（`.claude/REVIEW.md`）—— 见 `评审记录.md`：1 HIGH + 3 MEDIUM + 5 LOW 已全部修复并补回归测试

## 下一步

执行 `/harness-apply skillhub-repo-display-20260917-01`
