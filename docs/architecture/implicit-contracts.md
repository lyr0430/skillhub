# 隐性业务契约（Implicit Contracts）

> 本文档汇聚跨模块、跨分支容易踩坑的**非显式**业务约定，作为实现与评审时的快速检查表。
> 凡与权威文档冲突处，以根目录扁平 `docs/NN-*.md` 系列为准，此处只做约束提炼。
>
> **维护原则：本文件的内容无法从目录结构或依赖推导，只能由真实缺陷/评审发现沉淀。**
> 新增条目必须写明它来自哪次变更（见文末「权威来源」），**不得**为凑数而写入可从代码直接读出的事实。

## 目录

1. 身份主键约束（已冻结）
2. 技能坐标体系（已冻结）
3. slug 保留字
4. 生命周期与状态机分离原则
5. 提升（Promotion）唯一事实来源
6. 并发约束落地（PostgreSQL）
7. 幂等
8. 鉴权分层
9. 用户可见错误
10. 客户端安装（桌面/Tauri）约定

## 1. 身份主键约束（已冻结）

- 用户身份主键**全链路统一使用 `string`**，禁止使用 `int` / `long` / `bigint` 作为平台用户标识的正式契约类型。
- 覆盖认证主体、API 入参/出参、权限判定、审计、owner/creator/updater/reviewer/actor/submittedBy 等全部用户关联字段。
- 原因：兼容外部 SSO / OAuth / OIDC / SCIM 身份源。
- 若未来引入 surrogate key 作为内部索引，也只能是内部实现细节，**不能**替代字符串 `userId` 成为契约主键。

## 2. 技能坐标体系（已冻结）

- skillhub 内部坐标：`@{namespace_slug}/{skill_slug}`。
- ClawHub CLI 兼容层 canonical slug：`@global` → `{skill_slug}`；`@team-name` → `{namespace_slug}--{skill_slug}`。
- 分隔符为**双连字符 `--`**；skill slug 与 namespace slug 均禁止包含 `--`。
- 冲突规则：若 `@global/team-name--my-skill` 与 `@team-name/my-skill` 冲突，以 `--` 拆分优先。全局空间 skill slug 禁止含 `--`。
- 显示：Web 端始终显示完整坐标；兼容层返回 canonical slug。

## 3. slug 保留字

namespace slug 不可使用保留字：`admin, api, dashboard, search, auth, me, global, system, static, assets, health`。
`@global` 由 Flyway 预置，绕过 slug 校验；保留词校验仅作用于用户创建 namespace 的接口。

## 4. 生命周期与状态机分离原则

- **skill 容器 `status`**：只表达生命周期，不再承载"隐藏"语义。隐藏是独立治理覆盖层（`hidden` / `hidden_at` / `hidden_by`）。
- **version `status`**：`PUBLISHED / PENDING_REVIEW / DRAFT / REJECTED / YANKED`。YANKED 版本号永久占用、不可复用、标记不可下载。
- **review task `status`**：与 skill 容器、version 生命周期相分离。
- 各状态机不互相耦合，改动一个生命周期字段时须确认不影响其他两层。

## 5. 提升（Promotion）唯一事实来源

- 提升关系唯一事实来源是 `promotion_request` 表。
- "是否已提升"通过 `SELECT ... FROM promotion_request WHERE source_skill_id=? AND status='APPROVED'` 判定。
- `skill` 表**不**冗余 `promoted_to_skill_id`。

## 6. 并发约束落地（PostgreSQL）

约束并发重复提交的典型方案（审核任务 / 提升申请）：
- partial unique index：`CREATE UNIQUE INDEX ... WHERE status='PENDING'`；
- 或 `deleted` 字段 + `(skill_version_id, deleted)` 唯一约束（撤回时 `deleted=id`）；
- 或撤回时物理删除 + `(skill_version_id)` 唯一约束。

## 7. 幂等

- Redis 做快速去重（SETNX），PostgreSQL `idempotency_record` 做持久化兜底。
- 写接口必须明确事务边界，禁止无条件全表更新/删除。

## 8. 鉴权分层

- namespace 权限由 `namespace_member.role`（OWNER / ADMIN / MEMBER）决定，**不**走 RBAC 表。
- 平台级角色（SUPER_ADMIN / SKILL_ADMIN / USER_ADMIN / AUDITOR）走 RBAC。
- 路由守卫按角色进行页面级控制（见前端架构）。

## 9. 用户可见错误

- 错误信息不得泄露敏感数据（内部异常、堆栈、SQL）。
- 面向 UI 提供用户友好的错误消息。

## 10. 客户端安装（桌面/Tauri）约定

- **下载 URL 走 CLI 兼容层**：`/api/cli/v1/skills/{namespace}/{slug}/versions/{version}/download`（`/api/v1` 是 Web 端，`/api/cli/v1` 是 CLI/桌面端统一复用）。桌面（Tauri/Rust）与 CLI 都使用这条路径下载技能 zip。
- **Agent skill 目录约定**（与 `cli/src/agents/profiles/` 对齐，避免桌面端与 CLI 两处漂移）：claude-code → `.claude/skills`、codex → `.codex/skills`、opencode → `.opencode/skills`、openclaw → `.openclaw/skills`、**generic（默认全局）→ `.agents/skills`**。路径由 `dirs::home_dir()` / `%USERPROFILE%` 解析，禁止硬编码绝对路径。
- **Tauri 运行环境检测**：通过 `window.__TAURI__?.core?.invoke` 是否存在判断。桌面端走一键安装（弹窗选择 agent → Rust `install_skill`），**网页端回退为复制安装命令**（浏览器沙箱无法写本地目录，此为不可绕过约束）。
- **zip 解压安全**：解压必须防 zip-slip（`enclosed_name()` 校验，禁止条目逃逸出目标目录）。
- **`metadata.json` 是 CLI 与桌面端的共享契约**：`.skillhub/metadata.json`（`schemaVersion: 1`，camelCase）由 CLI 写入，桌面端也会读写。桌面端的 `InstalledMetadata` 结构体只建模其中一部分字段，**写回必须是合并式**（读原始 JSON → 覆盖已知字段 → 写回），否则 CLI 写入的 `versionId` / `fingerprint` / `files` 会被静默丢弃。两侧都不拒绝未知字段。
- **「受管」只有一个定义**：`metadata.json` **能被解析**（而非文件存在）。扫描、卸载、安装覆盖三处必须共用同一个判定函数；历史上两处定义漂移曾导致「UI 承诺备份、实际无备份删除」。
- **软链接是共享技能的唯一实现方式**，因此有两条硬不变量：
  - **卸载只删链接本身**，真实目录原封不动（显式 `symlink_metadata().file_type().is_symlink()` 判定后 `remove_file`，不得对可能是链接的路径调用 `remove_dir_all`）。
  - **更新写真实目录**，链接与其指向关系保持不变；跟随链接前必须确认目标目录含 `SKILL.md`，否则拒绝写入——破坏性动作作用在解析结果上，不加此前提则链接可把任意目录（如 `$HOME`）交给替换逻辑。
- **Rust ↔ TypeScript 的 wire 字段名**：Rust 结构体上凡是多词字段，必须带 `#[serde(rename_all = "camelCase")]`。serde 静默忽略未知字段，字段名不一致不会报错，只会取 `#[serde(default)]` 的默认值——曾因此让「备份并安装」实际执行 `remove_dir_all`。
- **路径来自前端时必须在 Rust 侧校验**：`install_skill` 与 `uninstall_skill_command` 的 `dir` 均来自 web view，两个入口必须对称地约束到 agent skill root 之内（只 `canonicalize` 父目录、叶子段按原样保留，按组件比较而非字符串前缀）。
- **共享安装的目录布局**：真实文件在 `~/.skillhub/skills/<slug>/`，各 agent 目录下是指向它的软链接。该仓库与 CLI 的工作区文件 `~/.skillhub/namespace-sync.json` **不同层**——桌面端只写 `skills/` 子目录，不读不写 `namespace-sync.json`。`.skillhub` 在本仓库有三个含义（技能目录内的元数据目录、CLI 工作区根、共享仓库），改动任一含义前先确认没踩到另外两个。
- **共享安装的写入目标恒为真实目录**：先落盘到仓库目录，**落盘成功后**再建链接。顺序反了（先建链接再落盘）会让 `swap_into_place` 的 `rename` 把链接换成真实目录，静默使其脱离目标——这正是问题 2 那两条不变量的成因。另：共享分支只在**全新安装**（无显式 `dir`）时生效；显式 `dir` 表示「更新既有条目」，必须继续走 `resolve_install_target` 的写穿链接契约。
- **跨 agent 复用技能时，源目录在 agent root 之外**：把已有技能添加到其他 agent（`attach_skill_to_agent`）的源是技能真实目录，共享安装下即 `~/.skillhub/skills/<slug>`，**不落在任何 agent skills root 内**。因此源校验必须用「是不是技能目录」（`is_skill_package`，存在 `SKILL.md`），**不能**用 `ensure_under_agent_root`——后者会把共享安装产出的技能全部拒掉，而共享安装恰恰是这条命令的主要来源。目标侧仍是 agent root 下的条目，由 `agent_root.join(slug)` 构造。
- **Tauri 命令参数在 JS 侧是 camelCase**：`#[tauri::command] fn f(source_dir: String)` 在 `invoke` 时要传 `{ sourceDir }`，不是 `{ source_dir }`。这与上面 serde 的 `rename_all = "camelCase"` 是**两件事**：那条管结构体字段，这条管命令参数。写错不报错，参数静默缺失。
- **顶栏的两个定位陷阱**（二者独立，都踩过）：
  - **`backdrop-filter` 会改变 `fixed` 的定位基准**。全局 header 带 `backdrop-blur-xl`，非 `none` 的 `backdrop-filter` 使其成为 `position: fixed` 后代的**包含块**——在 header 内声明居中弹窗，它会相对 header 盒子而非视口居中，居中直接失效。**结论：shell 级弹窗必须由 `Layout`（header 之外）渲染**，open 状态提升到 Layout。参见 `SettingsDialog` + `settingsOpen`。
  - **顶栏内的弹出层不要用 Radix portal**。header 随每次导航/搜索更新重渲染，body portal 在这条路径上会与 React 19 协调冲突（`removeChild` / `insertBefore`）。顶栏**内联**浮层一律用 in-tree `relative` 容器 + `absolute` 面板（见 `LanguageSwitcher` / `UserMenu`）。
  - 两者不矛盾：渲染在 header **之外**的弹窗用 Radix Dialog（自身 portal，附带遮罩/焦点陷阱/Escape）是安全的——`dismissOpenOverlays()` 也已在路由变化时覆盖它。

## 权威来源

- 身份主键 / 坐标体系 / 保留字：[`docs/00-product-direction.md`](../00-product-direction.md)、[`docs/02-domain-model.md`](../02-domain-model.md)
- 生命周期状态机：[`docs/14-skill-lifecycle.md`](../14-skill-lifecycle.md)、[`docs/05-business-flows.md`](../05-business-flows.md)
- 并发与幂等：[`docs/02-domain-model.md`](../02-domain-model.md)
- 鉴权分层：[`docs/03-authentication-design.md`](../03-authentication-design.md)
- 客户端安装（桌面/Tauri）约定：从本变更 `client-skill-manage-20260911-01`、[`docs/07-skill-protocol.md`](../07-skill-protocol.md) 沉淀。
- `metadata.json` 共享契约、软链接不变量、wire 字段名规则、前端传入路径的校验边界：从本变更 `local-skill-manage-20260915-01` 沉淀。
- 共享仓库布局、共享安装的写入顺序与生效条件、attach 的源校验边界、Tauri 命令参数命名、顶栏禁用 portal：从本变更 `skill-install-mode-20260916-01` 沉淀。

