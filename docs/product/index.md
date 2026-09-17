# SkillHub 产品规则

> 产品规则索引页。权威产品文档为 [`docs/00-product-direction.md`](../00-product-direction.md) 与 [`docs/25-skill-suites.md`](../25-skill-suites.md)。

## 定位（已冻结）

- **单实例共享技能注册中心**，不是多租户平台。
- 隔离边界是 **namespace**（`@global` / `@team-*`），不是租户。
- `@global` 是平台级公共空间，由平台管理员管理。
- 公共技能（`visibility=PUBLIC`）**匿名可浏览和下载**，无需登录。

## 产品边界蓝本

- 参照 **ClawHub** 继承产品模型（不照搬技术实现）。
- 参照 **OpenSkills** 借鉴 SKILL.md 格式与目录结构约定。
- 一期必须提供 **ClawHub CLI 协议兼容层**：服务端暴露一组 ClawHub CLI 兼容 registry API，使现有 CLI 无需或仅最小配置修改即可完成查询/解析/下载/发布/校验。

## 功能切分

- 门户区（公开）：首页、搜索、技能列表、套件列表、namespace 主页、技能详情、版本历史、版本对比、套件详情。
- **本地技能**（`/local-skills`）：桌面端托管本地 agent 已安装技能的视图与来源跳转（见「客户端与本地技能」）。
- 个人中心（登录）：我的技能、发布技能、收藏、订阅、Token 管理、我的 namespace、我的套件、通知。
- 命名空间管理（空间 ADMIN）：成员管理、空间审核。
- 审核与治理（按角色）：审核中心、提升审核、报告处理、治理台。
- 平台管理（平台角色）：审计日志、运营标签、namespace 管理、用户管理。

## 治理能力

- 发布后治理：报告、标记、隐藏、撤回。
- 审计、收藏、评分、统计、运营标签等为扩展位。
- 公共查询 API 与 CLI API 双通道。

## 客户端与本地技能

- **Tauri 桌面端**提供「一键安装技能到本地 agent」；网页端因浏览器沙箱无法写本地目录，**回退为复制安装命令**（不可绕过）。
- 安装目标的 agent 目录约定与 CLI 对齐：claude-code → `.claude/skills`、codex → `.codex/skills`、opencode → `.opencode/skills`、openclaw → `.openclaw/skills`、generic → `.agents/skills`。
- **软链接是共享技能的唯一实现方式**：卸载只删链接本身，更新写真实目录。相关硬约束见 [`docs/architecture/implicit-contracts.md`](../architecture/implicit-contracts.md) 第 10 节。
- `.skillhub/metadata.json` 是 CLI 与桌面端的**共享契约**，写回必须合并式。

## 身份与坐标（产品视角的硬约束）

- 用户身份主键全链路 `string`。
- 技能坐标 `@{namespace_slug}/{skill_slug}`，ClawHub 兼容层使用单 slug 映射（双连字符规则）。

## 变更工件

- 需求提案：`openspec/changes/<change-id>/proposal.md`
- 进行中变更：`openspec/changes/add-skill-suites/`
- 已归档变更：见 [`openspec/changes/archive/index.md`](../../openspec/changes/archive/index.md)

## 权威来源

- [`docs/00-product-direction.md`](../00-product-direction.md) — 定位、蓝本、坐标体系、身份主键约束
- [`docs/05-business-flows.md`](../05-business-flows.md) — 业务流
- [`docs/07-skill-protocol.md`](../07-skill-protocol.md) — 技能协议与客户端安装
- [`docs/25-skill-suites.md`](../25-skill-suites.md) — 技能套件

