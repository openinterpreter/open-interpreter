---
title: 自动审查
description: 让审查代理评估符合条件的批准请求。
---

自动审查可以将符合条件的批准提示委托给审查代理，而不是始终直接询问用户。

```toml
approvals_reviewer = "auto_review"
```

自动审查不会移除沙箱。它更改了在活动策略下评估符合条件的批准提示的主体。仅在审查策略和沙箱足够严格以适用于仓库时使用。

对于代码审查，请使用 `/review` 或 `interpreter exec review`。
