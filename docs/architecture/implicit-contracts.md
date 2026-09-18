# 隐性业务契约（Implicit Contracts）

> 本文档汇聚跨模块、跨分支容易踩坑的**非显式**业务约定，作为实现与评审时的快速检查表。
> 凡与权威文档冲突处，以根目录扁平 `docs/NN-*.md` 系列为准，此处只做约束提炼。
>
> **维护原则：本文件的内容无法从目录结构或依赖推导，只能由真实缺陷/评审发现沉淀。**
> 新增条目必须写明它来自哪次变更（见文末「权威来源」），**不得**为凑数而写入可从代码直接读出的事实。

## 目录

- 权威约束索引（§1–§9 已折叠，均为 `docs/00–05` 的既有规范，不在本文件重复正文）
- 10. 客户端安装（桌面/Tauri）约定

## 权威约束索引

> 以下内容是 `docs/00-product-direction.md`、`docs/02-domain-model.md`、`docs/03-authentication-design.md`、`docs/05-business-flows.md`、`docs/14-skill-lifecycle.md` 中**已冻结约束的定位索引**，不是本次变更的踩坑沉淀，本文件不再逐字重复正文。编辑时以对应权威文档为准；改动任一事实来源时必须同步索引指向，防止两份来源漂移。

| 约束 | 要点 | 权威来源 |
|------|------|---------|
| 身份主键 | 全链路 `string`，禁 `int/long/bigint`；覆盖认证、API、权限、审计等全部用户关联字段 | `docs/00-product-direction.md`、`docs/02-domain-model.md` |
| 技能坐标 | 内部 `@{namespace_slug}/{skill_slug}`；CLI canonical `@global`→`{skill_slug}`、`@team-name`→`{ns}--{slug}`；分隔符 `--` | `docs/00-product-direction.md`、`docs/02-domain-model.md` |
| slug 保留字 | 保留词仅限用户创建 namespace；`@global` 由 Flyway 预置绕过 | `docs/02-domain-model.md` |
| 生命周期/状态机分离 | skill 容器 `status` 不再承载隐藏；version `status` 五态；review task 独立；三者不耦合 | `docs/05-business-flows.md`、`docs/14-skill-lifecycle.md` |
| 提升唯一事实来源 | `promotion_request` 表；`skill` 不冗余 `promoted_to_skill_id` | `docs/02-domain-model.md`、`docs/05-business-flows.md` |
| 并发约束 | partial unique index / `deleted` 字段 / 撤回时物理删除，三选一 | `docs/02-domain-model.md`、`docs/05-business-flows.md` |
| 幂等 | Redis SETNX + `idempotency_record` 兜底；写接口必须明确事务边界 | `docs/02-domain-model.md` |
| 鉴权分层 | namespace 走 `namespace_member.role`；平台级角色走 RBAC；路由守卫页面级 | `docs/03-authentication-design.md` |
| 用户可见错误 | 不泄露敏感数据；面向 UI 提供友好消息 | `docs/03-authentication-design.md`、`docs/07-skill-protocol.md` |

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
- **仓库根路径不再是硬编码常量**：`repo_root()` 会先读桌面端自己的配置覆盖值（`dirs::config_dir()/skillhub/desktop-config.json`，键 `skillStoragePath`），无覆盖时才回退 `~/.skillhub/skills`。该覆盖值**存在桌面端自己的配置目录，不是 `~/.skillhub` 内**，写回必须合并式（保留未知字段）——与 `metadata.json` 合并式写回同理。所有消费方（共享安装、attach 源根、`.skillhub` 扫描、仓库卸载）都经 `repo_root()` 取当前生效路径，改一处全链路一致；迁移仓库时先**在 rename 前**枚举旧根的软链接（rename 后旧路径已不可解析），再整库 rename、逐个重写链接，跨盘（`EXDEV`）直接报错，目标非空预检中止。
- **`@tauri-apps/plugin-dialog` 的命令必须先在权限里授予**：`open` 命令需要 `dialog:allow-open`，`save` 需要 `dialog:allow-save`，二者独立，都必须在 `web/src-tauri/capabilities/default.json` 的 `permissions` 里声明，否则调用在 Tauri 侧被拒且**静默无反应**（前端若不 try/catch 则表现为「点按钮没反应」）。曾只授予 `dialog:allow-save`、用 `open` 选目录时确认按钮无响应。
- **共享安装的写入目标恒为真实目录**：先落盘到仓库目录，**落盘成功后**再建链接。顺序反了（先建链接再落盘）会让 `swap_into_place` 的 `rename` 把链接换成真实目录，静默使其脱离目标——这正是问题 2 那两条不变量的成因。另：共享分支只在**全新安装**（无显式 `dir`）时生效；显式 `dir` 表示「更新既有条目」，必须继续走 `resolve_install_target` 的写穿链接契约。
- **跨 agent 复用技能时，源目录在 agent root 之外**：把已有技能添加到其他 agent（`attach_skill_to_agent`）的源是技能真实目录，共享安装下即 `~/.skillhub/skills/<slug>`，**不落在任何 agent skills root 内**。因此源校验必须用「是不是技能目录」（`is_skill_package`，存在 `SKILL.md`），**不能**用 `ensure_under_agent_root`——后者会把共享安装产出的技能全部拒掉，而共享安装恰恰是这条命令的主要来源。目标侧仍是 agent root 下的条目，由 `agent_root.join(slug)` 构造。**但源必须落在这两类「合法技能家」之一**：共享仓库（`repo_root()`）或某 agent skills root（`skill_source_roots()` = 各 agent root + repo_root）。仅「含 SKILL.md」不够——否则一个 webview 可控的 `source_dir` 可指向机器上任意含 `SKILL.md` 的目录并把它链路/复制进 agent 根。位置校验在命令入口 `attach_skill_to_agent` 用 `assert_skill_source_within_roots`（复用 `ensure_under_roots` 的直接子目录规则），纯函数 `resolve_attach_source`/`attach_under` 保持可测（temp tree 源仍被接受）。
- **`.skillhub` 是展示用合成扫描源，不是 agent**：`scan_local_skills()` 把 `repo_root()`（`~/.skillhub/skills/`）以合成 agent id `.skillhub`（`REPO_AGENT_ID`）追加为第六个扫描根，与 5 个 agent 平级，但**不进 `AGENT_PROFILES`**——因此不可选为安装目标、不出现在安装弹窗。它只出现在 `locations[].agent` 与前端分类筛选里。仓库项与 agent 链接**按 canonical 真实路径聚合**（复用 `aggregate()`），所以一个共享技能不会重复出卡：`.skillhub` location + 各 agent link 归并为**一个** `LocalSkill`，其派生字段 `repo_managed`（`locations` 含 `.skillhub`）与 `linked_agents`（其余指向它的 agent）在聚合末尾一次性算出。
- **仓库卸载语义与 agent 卸载相反**：agent 分类下卸载**只删一个链接、保留真实目录**；`.skillhub` 分类下卸载 = 删除真实目录 `~/.skillhub/skills/<slug>/` **+** 所有指向它的 agent 链接，所以**必须用独立命令** `uninstall_repo_skill_command`，不复用 `uninstall_location`（后者 `ensure_under_agent_root` 只认 agent root，会拒掉仓库路径——这是好事）。顺序**先删所有链接（只删链接），后 `remove_dir_all` 真实目录**：任一步失败不会留下「目录已删、链接悬空」的中间态。`remove_dir_all` 前必须用 `symlink_metadata` 确认目标是**真实目录**（非链接），否则会删穿链接。真实目录侧**不做** `ensure_under_roots` 边界——仓库在 agent root 之外，靠 `validate_slug` + `repo_root().join(slug)` 构造路径约束。
- **仓库卸载仍须走「受管」唯一定义**：`remove_dir_all` 只作用于**可解析的 `.skillhub/metadata.json`**（`has_metadata`）的目录；一个手放进去、无元数据的仓库目录会被**重命名备份**（`*.skillhub-backup-*`）而非删除，与 agent 路径对非受管目录的处置一致（见上方「「受管」只有一个定义」）。曾一度把仓库卸载写成「删目录 + 所有链接，无条件删除」，评审发现这会在 `.skillhub` 分类下删掉用户手放的目录且无备份——已回改为按 `has_metadata` 分叉。另：**链接发现失败不可静默吞掉**：某 agent root 读不了/某条目分类失败时，`find_links_to_target_under` 必须上报警告，`remove_repo_item`据此保留真实目录，否则会「删了目录、留了个没发现的悬空链接」。
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
- 仓库根路径可配置（配置目录存储、迁移预检/跨盘限制）、`plugin-dialog` 权限声明：从本变更 `skill-storage-path-20260918-01` 沉淀。

