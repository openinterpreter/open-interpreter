# Documentation localization

English is the canonical, unprefixed documentation locale. Chinese is the only
fully localized documentation locale right now.

- English pages live at `docs/<slug>.md`.
- Chinese pages live at `docs/zh/<slug>.md`.
- English navigation lives in `docs.json`.
- Chinese navigation lives in `docs.zh.json`.
- Locale-specific website landing content lives at
  `docs-site/<locale>/terminal-index.mdx`.

Every English document must have a Chinese document with the same filename, and
vice versa. The two navigation files must also contain the same ordered page
slugs. Run this before committing documentation changes:

```bash
pnpm docs:locales:check
```

CI runs the same check. The website then generates its Chinese terminal-doc
routes from these files; generated website routes are never the canonical
place to edit documentation.
