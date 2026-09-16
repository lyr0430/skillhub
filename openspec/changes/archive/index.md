# OpenSpec 归档索引

按归档时间倒序。每条记录指向归档目录，目录内含该变更的 `proposal.md` / `design.md` / `tasks.md` / `.openspec.yaml` 与 `归档记录.md`。

| 归档时间 | change-id | 需求 | 归档目录 |
|---|---|---|---|
| 2026-09-16 | `local-skill-manage-20260915-01` | 本地技能页托管非本仓库技能、软链接安全处理、来源跳转 | [`local-skill-manage-20260915-01-20260916/`](./local-skill-manage-20260915-01-20260916/) |
| 2026-09-16 | `client-skill-manage-20260911-01` | Tauri 桌面客户端「一键安装技能到本地 agent」+ 本地技能管理页 | [`client-skill-manage-20260911-01-20260916/`](./client-skill-manage-20260911-01-20260916/) |

## 未归档但仍在本地的变更

| change-id | 状态 | 说明 |
|---|---|---|
| `add-skill-suites` | 上游变更 | 来自 `upstream/main`，非本 fork 产出，未改动 |

## 说明

`client-skill-manage-20260911-01` 归档时**补跑了一次评审**：它实现后直接合入 main，评审步骤从未执行。补评审查出 1 CRITICAL + 4 HIGH，已修复 5 项（HIGH 及以上），其余如实记入该归档记录的「已知风险」表 —— 其中 **`capabilities/default.json` 对自定义命令不生效**是最高优先的遗留项，整改需单独开 change。
