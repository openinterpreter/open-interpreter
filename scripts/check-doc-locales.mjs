#!/usr/bin/env node

import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const docsRoot = path.join(repoRoot, "docs");
const localizedDocs = [{ locale: "zh", directory: path.join(docsRoot, "zh") }];

function navigationPages(value) {
  if (typeof value === "string") return [value];
  if (!value || typeof value !== "object") return [];
  if (Array.isArray(value)) return value.flatMap(navigationPages);
  return navigationPages(value.pages ?? value.groups ?? []);
}

function normalizePage(page, locale = null) {
  const localePrefix = locale ? `docs/${locale}/` : "docs/";
  return page.replace(/\.mdx?$/, "").replace(localePrefix, "");
}

async function markdownNames(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  return entries
    .filter((entry) => entry.isFile() && entry.name.endsWith(".md"))
    .map((entry) => entry.name)
    .sort((left, right) => left.localeCompare(right));
}

function difference(left, right) {
  const rightNames = new Set(right);
  return left.filter((name) => !rightNames.has(name));
}

function hasFrontmatterValue(markdown, key) {
  if (!markdown.startsWith("---\n")) return false;
  const end = markdown.indexOf("\n---", 4);
  if (end === -1) return false;
  return new RegExp(`^${key}:\\s*\\S.+$`, "m").test(markdown.slice(4, end));
}

function hasObviousUntranslatedScaffolding(markdown) {
  const prose = markdown.replace(/```[\s\S]*?```/g, "");
  return [
    /^\|\s*(?:Command|Purpose|Value|Meaning|Role|Behavior)\s*\|/m,
    /^#{1,4}\s+(?:Good First Step|Pull Requests|Built-In Profiles|Custom Profile|Filesystem Rules|Network Rules|Use Files|Better Than Vague Requests)$/m,
  ].some((pattern) => pattern.test(prose));
}

const englishNames = await markdownNames(docsRoot);
const errors = [];

const englishNavigation = JSON.parse(
  await readFile(path.join(repoRoot, "docs.json"), "utf8"),
);
const chineseNavigation = JSON.parse(
  await readFile(path.join(repoRoot, "docs.zh.json"), "utf8").catch(() => "{}"),
);
const englishPages = navigationPages(englishNavigation.navigation?.groups).map((page) =>
  normalizePage(page),
);
const chinesePages = navigationPages(chineseNavigation.navigation?.groups).map((page) =>
  normalizePage(page, "zh"),
);

if (JSON.stringify(englishPages) !== JSON.stringify(chinesePages)) {
  errors.push(
    "docs.zh.json must contain the same ordered page slugs as docs.json",
  );
}

for (const { locale, directory } of localizedDocs) {
  const localizedNames = await markdownNames(directory).catch(() => []);
  for (const name of difference(englishNames, localizedNames)) {
    errors.push(`docs/${locale}/${name} is missing`);
  }
  for (const name of difference(localizedNames, englishNames)) {
    errors.push(`docs/${locale}/${name} has no matching English document`);
  }

  for (const name of localizedNames) {
    const markdown = await readFile(path.join(directory, name), "utf8");
    if (!hasFrontmatterValue(markdown, "title")) {
      errors.push(`docs/${locale}/${name} is missing title frontmatter`);
    }
    if (!hasFrontmatterValue(markdown, "description")) {
      errors.push(`docs/${locale}/${name} is missing description frontmatter`);
    }
    if (hasObviousUntranslatedScaffolding(markdown)) {
      errors.push(`docs/${locale}/${name} contains untranslated English headings or table labels`);
    }
  }
}

const localizedLandingPage = path.join(
  repoRoot,
  "docs-site",
  "zh",
  "terminal-index.mdx",
);
await readFile(localizedLandingPage, "utf8").catch(() => {
  errors.push("docs-site/zh/terminal-index.mdx is missing");
});

if (errors.length > 0) {
  console.error("Localized documentation is out of sync:\n");
  for (const error of errors) console.error(`- ${error}`);
  process.exit(1);
}

console.log(
  `Localized documentation is complete: ${englishNames.length} English and ${englishNames.length} Chinese documents.`,
);
