#!/usr/bin/env node

import { copyFile, mkdir, readdir, readFile, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const workspaceRoot = path.resolve(scriptDir, "..");
const sourceDocsDir = path.join(workspaceRoot, "docs");
const sourceDocsJson = path.join(workspaceRoot, "docs.json");
const sourceDocsSiteDir = path.join(workspaceRoot, "docs-site");
const sourceDocsLandingPage = path.join(sourceDocsSiteDir, "terminal-index.mdx");
const websiteRoot = path.resolve(
  process.env.OPEN_INTERPRETER_WEBSITE_ROOT ??
    "/Users/killianlucas/Documents/project/services/workstation/app/website",
);
const docsAppDir = path.join(websiteRoot, "src", "app", "cli", "docs");
const publicDir = path.join(websiteRoot, "public");
const marketingAssetSource = path.join(
  sourceDocsSiteDir,
  "assets",
  "open-interpreter-terminal-hero.png",
);
const marketingAssetDestination = path.join(
  publicDir,
  "assets",
  "docs",
  "open-interpreter-terminal-hero.png",
);
const docsProductSource = path.join(
  websiteRoot,
  "src",
  "lib",
  "docs",
  "docs-product.ts",
);
const docsNavigationSourceFile = path.join(
  websiteRoot,
  "src",
  "lib",
  "docs",
  "docs-navigation.ts",
);
const pageTitlesSourceFile = path.join(
  websiteRoot,
  "src",
  "lib",
  "docs",
  "page-titles.ts",
);
const schemaDestination = path.join(
  publicDir,
  "schema.json",
);
const schemaSource = path.join(
  workspaceRoot,
  "oi1",
  "codex-rs",
  "core",
  "config.schema.json",
);
// Installer scripts are not copied. openinterpreter.com/install redirects to
// the canonical scripts in this repository's main branch.

const docsBasePath = "/docs/terminal";
const checkOnly = process.argv.slice(2).includes("--check");

function parseFrontmatter(markdown) {
  if (!markdown.startsWith("---\n")) {
    return { body: markdown, data: {} };
  }

  const endIndex = markdown.indexOf("\n---", 4);
  if (endIndex === -1) {
    return { body: markdown, data: {} };
  }

  const rawFrontmatter = markdown.slice(4, endIndex).trim();
  const body = markdown.slice(endIndex + 4).replace(/^\s*\n/, "");
  const data = {};

  for (const line of rawFrontmatter.split("\n")) {
    const match = line.match(/^([A-Za-z0-9_-]+):\s*(.*)$/);
    if (!match) {
      continue;
    }

    const [, key, rawValue] = match;
    data[key] = rawValue.trim().replace(/^["']|["']$/g, "");
  }

  return { body, data };
}

function titleFromSlug(slug) {
  return slug
    .split(/[-_]/)
    .filter(Boolean)
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");
}

function firstHeading(markdown) {
  const match = markdown.match(/^#\s+(.+)$/m);
  return match?.[1]?.trim() ?? null;
}

function ensureH1(markdown, title) {
  if (/^#\s+/m.test(markdown)) {
    return markdown;
  }

  return `# ${title}\n\n${markdown}`;
}

function generatedSourceNotice(source) {
  return `{/* 🚨 GENERATED FILE — DO NOT EDIT. Canonical source: ${source}. Run pnpm docs:sync from the website directory. */}\n\n`;
}

function normalizeDocsLinks(markdown) {
  return markdown
    .replace(/\]\(\/docs(?=\/|\))/g, `](${docsBasePath}`)
    .replace(/href="\/docs(?=\/|")/g, `href="${docsBasePath}`);
}

function layoutSource(slug) {
  return `import { pageMetadata } from "@/lib/docs/page-metadata";

export const metadata = pageMetadata(${JSON.stringify(slug)});

export default function Layout({ children }: { children: React.ReactNode }) {
  return children;
}
`;
}

function docsHref(href) {
  return href === "/" ? docsBasePath : `${docsBasePath}${href}`;
}

function formatNavItem([name, href]) {
  return `{ name: ${JSON.stringify(name)}, href: ${JSON.stringify(docsHref(href))} }`;
}

function renderNavigationSections(sections, indent = "") {
  const renderedSections = sections
    .map((section) => {
      const groups = section.groups?.length
        ? `,
    groups: [
${section.groups
  .map(
    (group) => `      {
        name: ${JSON.stringify(group.name)},
        items: [
${group.items.map((item) => `          ${formatNavItem(item)},`).join("\n")}
        ],
      }`,
  )
  .join(",\n")}
    ]`
        : "";

      return `  {
    title: ${JSON.stringify(section.title)},
    items: [
${section.items.map((item) => `      ${formatNavItem(item)},`).join("\n")}
    ]${groups},
  }`;
    })
    .join(",\n");

  return renderedSections
    .split("\n")
    .map((line) => (line ? `${indent}${line}` : line))
    .join("\n");
}

function docsNavigationSource(sections) {
  const renderedSections = renderNavigationSections(sections);

  return `export type NavItem = {
  name: string;
  href: string;
};

export type NavSection = {
  title: string | null;
  items: NavItem[];
  groups?: {
    name: string;
    items: NavItem[];
  }[];
};

export const navigation: NavSection[] = [
${renderedSections},
];

export const allDocsPages: NavItem[] = navigation.flatMap((section) => [
  ...section.items,
  ...(section.groups?.flatMap((group) => group.items) ?? []),
]);

export const orderedDocsPages: NavItem[] = Array.from(
  new Map(allDocsPages.map((item) => [item.href, item])).values(),
);
`;
}

function docsProductTerminalNavigationSource(sections) {
  return `const terminalNavigation: NavSection[] = [
${renderNavigationSections(sections)},
];`;
}

function pageTitlesSource(titles) {
  const entries = Object.entries(titles)
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([slug, title]) => `  ${JSON.stringify(slug)}: ${JSON.stringify(title)},`)
    .join("\n");

  return `export const PAGE_TITLES: Record<string, string> = {
${entries}
};

export function getPageTitle(slug: string): string | null {
  return slug in PAGE_TITLES ? PAGE_TITLES[slug]! : null;
}
`;
}

async function pathExists(filePath) {
  try {
    await readFile(filePath);
    return true;
  } catch (error) {
    if (error && typeof error === "object" && "code" in error && error.code === "ENOENT") {
      return false;
    }

    throw error;
  }
}

async function removeStaleRoutes(expectedRouteNames) {
  const entries = await readdir(docsAppDir, { withFileTypes: true });
  const staleRoutes = entries.filter(
    (entry) =>
      entry.isDirectory() && !expectedRouteNames.has(entry.name),
  );

  if (staleRoutes.length === 0) {
    return;
  }

  for (const entry of staleRoutes) {
    await rm(path.join(docsAppDir, entry.name), { recursive: true, force: true });
  }
}

async function sourceDocs() {
  const entries = await readdir(sourceDocsDir, { withFileTypes: true });
  const files = entries
    .filter((entry) => entry.isFile() && entry.name.endsWith(".md"))
    .map((entry) => entry.name)
    .sort((left, right) => left.localeCompare(right));

  return Promise.all(
    files.map(async (fileName) => {
      const slug = path.basename(fileName, ".md");
      const raw = await readFile(path.join(sourceDocsDir, fileName), "utf8");
      const { body, data } = parseFrontmatter(raw);
      const normalizedBody = body.trim();
      const title = data.title ?? firstHeading(normalizedBody) ?? titleFromSlug(slug);

      return {
        fileName,
        slug,
        title,
        content: `${normalizeDocsLinks(ensureH1(normalizedBody, title)).trim()}\n`,
      };
    }),
  );
}

function slugFromDocsJsonPage(page) {
  if (typeof page !== "string") {
    return null;
  }

  const normalized = page.replace(/\.mdx?$/, "");
  const docsPrefix = "docs/";
  if (!normalized.startsWith(docsPrefix)) {
    return null;
  }

  return normalized.slice(docsPrefix.length);
}

function navItemFromDocsJsonPage(page, docsBySlug) {
  const slug = slugFromDocsJsonPage(page);
  if (!slug) {
    return null;
  }

  const doc = docsBySlug.get(slug);
  if (!doc) {
    throw new Error(`docs.json references missing docs/${slug}.md`);
  }

  return [doc.title, hrefFromSlug(slug)];
}

function hrefFromSlug(slug) {
  return slug === "getting-started" ? "/" : `/${slug}`;
}

async function navigationFromDocsJson(docsBySlug) {
  const raw = await readFile(sourceDocsJson, "utf8");
  const docsJson = JSON.parse(raw);
  const groups = docsJson.navigation?.groups;
  if (!Array.isArray(groups)) {
    throw new Error("Expected core/docs.json to contain navigation.groups.");
  }

  return groups
    .map((group) => {
      const pages = Array.isArray(group.pages) ? group.pages : [];
      const items = [];
      const nestedGroups = [];

      for (const page of pages) {
        if (typeof page === "string") {
          const item = navItemFromDocsJsonPage(page, docsBySlug);
          if (item) {
            items.push(item);
          }
          continue;
        }

        if (page && typeof page === "object" && Array.isArray(page.pages)) {
          const nestedItems = page.pages
            .map((nestedPage) => navItemFromDocsJsonPage(nestedPage, docsBySlug))
            .filter(Boolean);

          if (nestedItems.length > 0) {
            nestedGroups.push({
              name: page.group ?? "More",
              items: nestedItems,
            });
          }
        }
      }

      return {
        title: group.group ?? null,
        items,
        groups: nestedGroups,
      };
    })
    .filter((group) => group.items.length > 0 || group.groups.length > 0);
}

async function writeRoute(doc) {
  const routeDir = path.join(docsAppDir, doc.slug);
  await mkdir(routeDir, { recursive: true });
  await writeFile(
    path.join(routeDir, "page.mdx"),
    `${generatedSourceNotice(`core/docs/${doc.fileName}`)}${doc.content}`,
  );
  await writeFile(path.join(routeDir, "layout.tsx"), layoutSource(doc.slug));
}

async function landingPageSource(rootDoc) {
  if (await pathExists(sourceDocsLandingPage)) {
    const raw = await readFile(sourceDocsLandingPage, "utf8");
    return `${generatedSourceNotice("core/docs-site/terminal-index.mdx")}${normalizeDocsLinks(raw).trim()}\n`;
  }

  return `${generatedSourceNotice(`core/docs/${rootDoc.fileName}`)}import { DocsTerminalHero } from "@/components/docs/docs-terminal-hero";\n\n${rootDoc.content.replace(
    /^# .+\n/,
    `# ${rootDoc.title}\n\n<DocsTerminalHero />\n`,
  )}`;
}

function docsProductWithNavigation(raw, sections) {
  const nextTerminalNavigation = docsProductTerminalNavigationSource(sections);
  const terminalNavigationPattern =
    /const terminalNavigation: NavSection\[\] = \[[\s\S]*?\n\];\n\nconst desktopNavigation:/;

  if (!terminalNavigationPattern.test(raw)) {
    throw new Error("Could not replace terminalNavigation in website docs-product.ts");
  }

  return raw.replace(
    terminalNavigationPattern,
    `${nextTerminalNavigation}\n\nconst desktopNavigation:`,
  );
}

async function updateDocsProductNavigation(sections) {
  const raw = await readFile(docsProductSource, "utf8");
  const next = docsProductWithNavigation(raw, sections);

  await writeFile(docsProductSource, next);
}

async function checkFile(filePath, expected, drift) {
  let actual;
  try {
    actual = await readFile(filePath);
  } catch (error) {
    if (error && typeof error === "object" && "code" in error && error.code === "ENOENT") {
      drift.push(`${path.relative(websiteRoot, filePath)} (missing)`);
      return;
    }
    throw error;
  }

  const expectedBuffer = Buffer.isBuffer(expected) ? expected : Buffer.from(expected);
  if (!actual.equals(expectedBuffer)) {
    drift.push(path.relative(websiteRoot, filePath));
  }
}

async function checkGeneratedSite(docs, rootDoc, navigationSections) {
  const drift = [];
  const expectedRouteNames = new Set(docs.map((doc) => doc.slug));
  const routeEntries = await readdir(docsAppDir, { withFileTypes: true });

  for (const entry of routeEntries) {
    if (entry.isDirectory() && !expectedRouteNames.has(entry.name)) {
      drift.push(`${path.relative(websiteRoot, path.join(docsAppDir, entry.name))}/ (stale route)`);
    }
  }

  for (const doc of docs) {
    const routeDir = path.join(docsAppDir, doc.slug);
    await checkFile(
      path.join(routeDir, "page.mdx"),
      `${generatedSourceNotice(`core/docs/${doc.fileName}`)}${doc.content}`,
      drift,
    );
    await checkFile(path.join(routeDir, "layout.tsx"), layoutSource(doc.slug), drift);
  }

  await checkFile(
    path.join(docsAppDir, "page.mdx"),
    await landingPageSource(rootDoc),
    drift,
  );

  const titles = Object.fromEntries(docs.map((doc) => [doc.slug, doc.title]));
  titles[""] = rootDoc.title;
  await checkFile(pageTitlesSourceFile, pageTitlesSource(titles), drift);
  await checkFile(
    docsNavigationSourceFile,
    docsNavigationSource(navigationSections),
    drift,
  );

  const docsProductRaw = await readFile(docsProductSource, "utf8");
  await checkFile(
    docsProductSource,
    docsProductWithNavigation(docsProductRaw, navigationSections),
    drift,
  );

  if (await pathExists(schemaSource)) {
    await checkFile(schemaDestination, await readFile(schemaSource), drift);
  }
  if (await pathExists(marketingAssetSource)) {
    await checkFile(marketingAssetDestination, await readFile(marketingAssetSource), drift);
  }

  if (drift.length > 0) {
    console.error("\n🚨 OPEN INTERPRETER DOCS ARE OUT OF SYNC 🚨\n");
    for (const file of drift) {
      console.error(`  - ${file}`);
    }
    console.error(
      "\nDo not edit generated website docs. Edit open-interpreter-next/core/docs, " +
        "then run pnpm docs:sync from the website directory.\n",
    );
    process.exitCode = 1;
    return;
  }

  console.log(`Docs are in sync with ${path.relative(workspaceRoot, sourceDocsDir)}.`);
}

async function main() {
  const docs = await sourceDocs();
  const docsBySlug = new Map(docs.map((doc) => [doc.slug, doc]));
  const rootDoc = docs.find((doc) => doc.slug === "getting-started");
  const navigationSections = await navigationFromDocsJson(docsBySlug);

  if (!rootDoc) {
    throw new Error("Expected core/docs/getting-started.md to generate the /cli/docs page.");
  }

  if (checkOnly) {
    await checkGeneratedSite(docs, rootDoc, navigationSections);
    return;
  }

  await mkdir(docsAppDir, { recursive: true });
  await removeStaleRoutes(new Set(docs.map((doc) => doc.slug)));
  await Promise.all(docs.map(writeRoute));

  await writeFile(path.join(docsAppDir, "page.mdx"), await landingPageSource(rootDoc));

  const titles = Object.fromEntries(docs.map((doc) => [doc.slug, doc.title]));
  titles[""] = rootDoc.title;

  await writeFile(pageTitlesSourceFile, pageTitlesSource(titles));
  await writeFile(docsNavigationSourceFile, docsNavigationSource(navigationSections));
  await updateDocsProductNavigation(navigationSections);

  let copiedSchema = false;
  if (await pathExists(schemaSource)) {
    await mkdir(path.dirname(schemaDestination), { recursive: true });
    await copyFile(schemaSource, schemaDestination);
    copiedSchema = true;
  }

  let copiedMarketingAsset = false;
  if (await pathExists(marketingAssetSource)) {
    await mkdir(path.dirname(marketingAssetDestination), { recursive: true });
    await copyFile(marketingAssetSource, marketingAssetDestination);
    copiedMarketingAsset = true;
  }

  console.log(`Synced ${docs.length} docs from ${path.relative(workspaceRoot, sourceDocsDir)}`);
  console.log(`Wrote docs routes to ${path.relative(websiteRoot, docsAppDir)}`);
  console.log(
    `Generated /docs/terminal from ${path.relative(workspaceRoot, sourceDocsLandingPage)}`,
  );
  console.log(`Generated terminal navigation from ${path.relative(workspaceRoot, sourceDocsJson)}`);
  console.log(`${copiedSchema ? "Copied" : "Skipped"} config schema`);
  console.log("Installer URLs remain backed by the public main branch");
  console.log(`${copiedMarketingAsset ? "Copied" : "Skipped"} terminal hero image`);
}

await main();
