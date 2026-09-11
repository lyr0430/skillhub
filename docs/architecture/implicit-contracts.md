# 隐性业务契约（Implicit Contracts）

> 本文档汇聚跨模块、跨分支容易踩坑的**非显式**业务约定，作为实现与评审时的快速检查表。
> 凡与权威文档冲突处，以根目录扁平 `docs/NN-*.md` 系列为准，此处只做约束提炼。

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

## 权威来源

- 身份主键 / 坐标体系 / 保留字：[`docs/00-product-direction.md`](../00-product-direction.md)、[`docs/02-domain-model.md`](../02-domain-model.md)
- 生命周期状态机：[`docs/14-skill-lifecycle.md`](../14-skill-lifecycle.md)、[`docs/05-business-flows.md`](../05-business-flows.md)
- 并发与幂等：[`docs/02-domain-model.md`](../02-domain-model.md)
- 鉴权分层：[`docs/03-authentication-design.md`](../03-authentication-design.md)
- 客户端安装（桌面/Tauri）约定：从本变更 `client-skill-manage-20260911-01`、[`docs/07-skill-protocol.md`](../07-skill-protocol.md) 沉淀。

