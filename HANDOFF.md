# Creatio DevHub — Engineering Handoff

Last verified: **2026-07-21**

Current version: **0.7.0** (releases v0.3.1 and v0.3.2 shipped after this doc's milestone
table below was last written; see per-release commit messages for their scope. v0.4.0 is the
shadcn/ui design-system release described below; v0.5.0 adds the automatic update notice and
stops trusting clio's exit code over its own output; v0.5.1 stops reporting a successful SQL
statement as an error, v0.5.2 fixes the regression that came with it; and v0.6.0 adds
application descriptor details.)

Repository: <https://github.com/SmitSuthar8834/creatio-devhub>

Latest release: <https://github.com/SmitSuthar8834/creatio-devhub/releases/tag/v0.6.0>

Website: <https://smitsuthar8834.github.io/creatio-devhub/> (branch `gh-pages`)

> **There is no v0.2.2.** That tag's workflow failed because the version bump wrote a UTF-8 BOM
> into `package.json` / `tauri.conf.json` — Windows PowerShell `Set-Content -Encoding utf8` writes
> a BOM. Use `[System.IO.File]::WriteAllText` with `UTF8Encoding($false)` (or an editor that
> preserves encoding) for JSON the toolchain parses. The broken tag was deleted, never reused.

## Design system: shadcn/ui migration (v0.4.0, 2026-07-20)

The UI was migrated from the hand-rolled `App.css` visual system to **shadcn/ui** (new-york
style, Radix + Tailwind v4, lucide icons). Shipped in v0.4.0.

- **Config**: `components.json` (style `new-york`, `cssVariables: true`, aliases `@/components`,
  `@/lib`, `@/hooks`), `.mcp.json` (shoogle registry MCP at `https://mcp.shoogle.dev/mcp`),
  `skills-lock.json` (skills `shadcn`, `search-registry-items`, `migrate-radix-to-base`).
- **Primitives** live in `src/components/ui/*` (button, card, badge, dialog, alert-dialog, input,
  label, select, tabs, table, progress, alert, checkbox, textarea, collapsible, separator,
  tooltip, scroll-area, dropdown-menu, sheet, sidebar, skeleton, radio-group, switch, sonner).
  Add more with `npx shadcn@latest add <name>` — it respects `components.json` and does **not**
  touch `src/index.css` theme tokens (verified).
- **Theme**: `src/index.css` is the single source of truth — `:root` (light) + `.dark` token sets
  supplied by the user, plus `@theme inline` mappings. Two token pairs were added beyond the
  supplied set: `--success`/`--warning` (+ `-foreground`), because env health and job outcomes
  need those voices. **No glassmorphism** (no `backdrop-blur`/`backdrop-filter` anywhere — only
  flat opacity tints). Fonts Inter + JetBrains Mono are bundled under `src/assets/fonts/`
  (no runtime network). Dark/light is class-based via `src/lib/theme.ts` (`system`/`light`/`dark`,
  follows OS, persisted to `localStorage`); the Settings → Appearance radio group drives it.
- **Toasts**: `sonner` via `src/components/ui/sonner.tsx`; `<Toaster>` is mounted in `App.tsx`, and
  `JobToaster.tsx` is a headless driver issuing one keyed toast per job.
- **Migrated screens**: every module now renders shadcn components — App shell/sidebar,
  Environments (+ Add/Edit dialogs), Jobs, Clio banner, ErrorNote, Settings, SQL, Packages,
  Applications, Workspaces, WorkspaceDetail, NewWorkspaceWizard, DeployFromGithubDialog.
- **`src/App.css` was deleted** — it was already orphaned (imported nowhere; `main.tsx` imports
  only `index.css`).
- **Validated**: `npx tsc --noEmit` clean; `npm run build` (tsc + vite) succeeds; `tauri dev`
  compiled and launched the desktop app cleanly. The frontend migration is the bulk of the change;
  the release also carries a `jobs.rs` "quiet job" flag (background health checks no longer raise
  toasts/desktop notifications) with a back-compat test for older `history.json` files.
- **Note for this dev box**: the shoogle MCP is project-scoped in `.mcp.json`; a Claude session
  must be started from `A:\PersonalComponents\creatio-devhub` for it to load. This migration used
  the official `npx shadcn@latest` CLI instead.

## Compare environments (v0.7.0, 2026-07-21)

A new **Compare** section answers the question the roadmap called the moat: is pre-prod still
the same as dev, and what exactly differs. Read-only — nothing in this feature writes to any
environment.

- **One command supplies everything.** `clio save-state <file> -e <env>` returns features,
  system settings, web services and every package with a hash per package *and per schema*.
  `src-tauri/src/envstate.rs` captures that into app-data `snapshots/<env>.yaml` as a
  **cancellable** job, then compares two files. Comparison is local, so re-comparing is
  instant and never re-reads an environment.
- **Do not rebuild this on live `show-diff`.** It took ~13 minutes for one pair and cannot
  back a screen; it also emits the same YAML shape, so there is no parsing advantage.
- **The YAML parser is hand-written, deliberately.** `serde_yaml` is deprecated and the
  maintained forks are unvetted — poor company for a path holding credentials. clio's output is
  machine-generated in four fixed shapes with no block scalars, anchors or flow collections
  (verified across a 986 KB capture). Parsed counts match a raw grep of that file exactly:
  713 settings, 456 features, 445 packages, 10,945 schemas, 3 web services. **If clio's output
  ever grows real YAML constructs, replace the parser rather than patch it.**
- **Capture duration is measured, not estimated**, and stored in `snapshots/durations.json`.
  The spread is too wide to hardcode: dev-834 (local IIS) takes ~2 minutes, the cloud tenant
  187559-crm-bundle took ~13. Before a first capture the UI states an honest range; afterwards
  it reports that environment's real previous time.
- **Schemas are compared only inside packages whose hash already differs.** A matching package
  hash means matching contents, so expanding all 10,945 would be noise. A real dev-834 ↔
  187559 comparison yields 230 differing packages (180 changed, 19 source-only, 31
  target-only), 133 settings, 10 features, and 2,519 schema rows — e.g. `CoreForecast` alone
  has 24 differing schemas. That schema level is the duplicate-element collision warning.
- **Absent by design:** package version and lock state are not in `save-state`. The diff keys
  on hash, so a version change reads as "differs". Adding them means merging `list-packages`
  and `package_lock_states` into the packages tab — three sources for one view.
- `--overwrite` must **not** be passed to `save-state`. It is a bare switch that already
  defaults to true, and supplying a value made clio consume `true` as the next positional and
  fail with `Active environment 'true' is not found`. Re-capture overwrites correctly without
  it (verified: 16 bytes → 1,259,334).

### Secrets handling — do not weaken this

Snapshots store setting values verbatim, so a snapshot file is a file full of API keys sitting
in app-data. Three layers, none of which is optional:

1. Values render as `••••••` until revealed **per row**; nothing bulk-reveals.
2. Exported markdown omits setting values entirely and says so in the document — a setting row
   reads `set`, never the value.
3. Capture logs a warning naming the risk, and the screen has a Delete control per snapshot.

The credential-name heuristic (`looks_sensitive`) only decides where a warning icon appears. It
is **never** the boundary, because both it and Creatio's own `SecureText` marking miss real
keys — see the settings-secrets entry under clio behaviours.

**UI verified** by driving the vite dev server with a stubbed IPC layer at realistic volume:
230 package rows render instantly, expanding a package adds exactly its differing schemas,
all 268 setting value cells mask, revealing one row reveals exactly two cells, and no secret
appears in the DOM before reveal. The **capture job itself has not been run in the packaged
app** — its clio invocation is the one verified by hand, but nobody has watched it complete
from the UI.

## Marketing content migration (implemented locally; live verification pending, 2026-07-22)

Second migration type next to lookups: Campaigns / Bulk Emails / email templates /
dynamic content / campaign flow diagrams, copied source→target GUID-preserving and idempotent.
The full spec, verified domain gotchas (SysSchema 403 → SQL path, zero-GUID stripping,
Supervisor remap, media-link MetaData reads, redis+restart requirement), proposed command
surface, and acceptance criteria are in **`HANDOFF-content-migration.md`** (repo root). The
proven reference implementation is `C:\Users\Lenovo\creatio-content-migrate\` — it performed
the real bundle→dev-834 migration on 2026-07-22 (102 content rows + 7 flow schemas + 92
CampaignItem + 135 SysLocalizableValue, all verified). The port adds the app's first direct HTTP
path (`reqwest` with Creatio forms auth), pure transformation/SQL tests, rollback scripts before
writes, and a staged Migration tab (analyze → content → flows → finalize → verify). Automated
validation is green (83 Rust tests, TypeScript and Vite production build). The required live
analyze/no-op migration and designer-after-restart verification are still pending, so this is not
yet a release gate.

**Second pass (2026-07-22): FK auto-resolution + per-record selection.** The first GUI run
(Dev-thoughtworks → pre-thoughtworks, Campaign only) failed 4/15 rows with `23503` on
`Campaign.OwnerId → Contact` — only the Supervisor contact was remapped, any other owner GUID is
per-environment. `content_migrate` now introspects the target's FK constraints (`pg_constraint`
via `refdata::run_select`; without cliogate it degrades to plain inserts), batch-verifies every FK
GUID on the target, remaps missing references to the same-named target row or clears the field,
and reports every change as a per-row `adjustment` — content-set parents are never cleared, they
are auto-included (`close_over_parents`) or the row is blocked with guidance. Self-FK parents
insert first. New `content_list_records` + `selections` on `content_migrate` back a Campaign /
BulkEmail record picker in the UI. 89 Rust tests, tsc + vite clean; details and live-verified
remap data in `HANDOFF-content-migration.md`. **Live-verified 2026-07-22: the user re-ran the
Campaign migration in the GUI after the fix and it succeeded** — first successful real run of the
content write path. Flows migration + finalize + designer check remain unexercised in the GUI.

## Lookup / reference-data migration (backend, WIP — unreleased, 2026-07-22)

Answers "move my package dev → pre *with its data*". Package config already deploys
(`pull-pkg`/`push-pkg`, which also carries package-**bound** data). The gap is **unbound
reference data** — values in lookup tables added directly in one environment. This addon reads,
compares, and migrates that. **Backend only so far — no UI, not wired into a release. Never run
against a live target yet.**

- **`src-tauri/src/refdata.rs`** (new). Six commands registered in `lib.rs`:
  - `list_lookups(env)` — enumerate lookups (name, table, package, has-Description). One
    `SysLookup ⋈ SysSchema` join; `information_schema` gives the Description flag. **Verified live
    on dev-834** (returns e.g. `SocialAccount`→has-Description `0`, exercising the NULL branch).
  - `capture_lookups(env)` — job; one `UNION ALL` reads every lookup's rows into app-data
    `snapshots/<env>.lookups.json` (separate from the `envstate` `.yaml` snapshots). **CSV shape
    verified live**: `__lk;Id;Name;Description`, Guid as text, empty tables absent (snapshot is
    seeded from the enumeration so an empty lookup still records as 0 rows).
  - `list_lookup_snapshots` / `delete_lookup_snapshot`.
  - `diff_lookups(source, target)` — local diff of two snapshots, emitted as the **same
    `envstate::DiffReport`/`DiffRow`** the Compare table renders (category `lookup`, with per-value
    `lookupValue` detail rows). Keyed on `Id`.
  - `build_lookup_migration(source, tables)` — read-only dry-run; returns the forward SQL.
  - `migrate_lookups(source, target, tables, skip_backup)` — the **mutating** job. Dual env-lock
    in sorted order, non-cancellable once writing (same discipline as
    `deploy_package_between_environments`). Unless `skip_backup`, it reads the target's current
    state first and writes a **runnable rollback `.sql`** to app-data `migrations/`
    (`rollback-<target>-<ts>.sql` + `applied-<...>.sql`) — upserts to restore updated rows, deletes
    to remove newly-inserted ones — and logs its path. The write goes through **`clio_capture`, not
    `stream_process`**, on purpose: `execute-sql-script` exits 0 even when the DB rejects the SQL,
    so the output must be read (`sql::is_failure`/`sql::friendly_error`, now `pub(crate)`).

- **Design invariants (do not weaken):**
  - **Keyed on `Id` (Guid), never `Name`** — other tables FK to lookup values by Id, so migrating
    must preserve the Guid. Migration is `INSERT … ON CONFLICT ("Id") DO UPDATE` (idempotent).
  - **`is_safe_identifier` guards every table name** spliced into SQL (alnum/underscore only); an
    unsafe name is dropped from capture and rejected by migration. Values are `''`-escaped.
  - Forward script wrapped `BEGIN;…COMMIT;` — one implicit transaction on PostgreSQL.

- **v1 scope limits (noted for the v2 backlog):** compares/migrates only the `BaseLookup` columns
  (`Id, Name, Description`); extra custom lookup columns need per-table `information_schema`
  introspection. Upsert SQL is **PostgreSQL** (`ON CONFLICT`) — MSSQL would need `MERGE`.
  `capture_lookups` is not cancellable (single clio invocation parsed after it returns).

- **Frontend:** `src/lib/ipc.ts` has all seven command wrappers + `LookupInfo` /
  `LookupSnapshotInfo` (and `DiffRow.category` now includes `lookup`/`lookupValue`).
  - **Compare → Lookups** (read-only): top-level **Configuration | Lookups** switch in
    `ComparePage.tsx`; the tab is `src/modules/compare/LookupCompare.tsx` — a self-contained
    capture / compare / delete surface on its own `.lookups.json` snapshots, with expandable
    per-value (`Id`-keyed) detail rows. Reuses the `DiffReport`/`DiffRow` renderer; no masking
    (lookups are display data, not secrets).
  - **Migration** (write-side): its own sidebar section (`ArrowRightLeft` icon, after Compare),
    `src/modules/migration/MigrationPage.tsx`. Pick source/target → filterable, checkable list from
    `list_lookups` → **Preview SQL** (`build_lookup_migration` dry-run, shown read-only) → **Migrate**
    behind a typed-target confirmation dialog with a "write a rollback script first (recommended)"
    toggle → `migrate_lookups`. Same typed-confirmation + backup pattern as the package-deploy dialog.
- **Validated:** `cargo test --lib` = **69 passed** (14 in `refdata`), crate compiles clean under
  the Build Tools env; `tsc --noEmit` clean; `npm run build` succeeds. Enumeration + capture SQL run
  live on dev-834.
- **Release status.** `v0.8.0` **was published** (2026-07-22, signed installers + `latest.json`) —
  it shipped the migration feature before the write path had a live run. The capture crash below was
  found and fixed *after* that release, so it ships as **`v0.8.1`** (patch). Write-path *SQL* is now
  verified live; the GUI-driven `migrate_lookups` command on real lookups is still not run (see
  below), but that path already shipped in v0.8.0 — v0.8.1 is fundamentally the capture bug fix.
  - **Done (2026-07-22):** the exact SQL primitives `migrate_lookups` generates were run against
    live PostgreSQL (`dev-834`) via a throwaway-table round-trip — no real lookup touched. Proven:
    forward `ON CONFLICT DO UPDATE` (update existing + insert new + leave untouched rows alone);
    `rollback_sql` (restore-updated + delete-inserted) restores the table **byte-for-byte** to the
    pre-migration state (diff empty); and a rejected statement surfaces its SQLSTATE (`42703`)
    **even though clio exits 0**, so `sql::is_failure`/`friendly_error` catch it (the v0.5.1
    regression class) and no partial write lands. This closes the "does the rollback actually
    restore?" risk.
  - **Still NOT done:** a run of the actual `migrate_lookups` **Tauri command through the GUI** on
    real lookups (Claude can't drive the native window). Someone must, in `dev.cmd`: capture the
    **dev-thoughtworks ↔ pre-thoughtworks** pair and eyeball the diff, then a real `migrate_lookups`
    against a **disposable** target, and confirm the on-disk `rollback-<target>-<ts>.sql` in app-data
    `migrations/` restores it. The Rust orchestration (dual env-lock, rollback-before-write ordering,
    file writes) is covered by unit tests + review but not yet exercised live.

### UI/UX fixes — shipped as v0.8.2 (2026-07-22)

Three fixes on top of v0.8.1, all frontend/`sql.rs` (74 Rust tests, `tsc` + `vite build` clean):
- **SQL runner explains empty NOTICE output.** clio's SQL executor returns result sets, not the
  DB connection's notice stream, so an anonymous `DO` block / `RAISE NOTICE` runs but returns
  nothing — it used to show a bare "Statement ran successfully". `run_sql` now detects this
  (`emits_only_notices`) and returns a `SqlResult.messages` note explaining notices aren't
  returned and to use a function that `RETURNS TABLE(...)` + trailing `SELECT`; the SQL page shows
  it in an info panel. (Verified live: clio drops RAISE NOTICE entirely; a temp-function + SELECT
  does come back as a grid.)
- **Jobs screen overflow + collapsible list.** The `1fr` grid track (min-width:auto) let the wide
  log scroll the whole page; now `minmax(0,1fr)` + `min-w-0` keeps it in-box, plus a toggle to
  collapse the job list for a full-width log.
- **Workspaces change list** uses a middle ellipsis so the filename stays visible (folder
  truncates), diff column `minmax(0,1fr)`, and the list scrolls for long change sets.

### Capture crash fix — shipped as v0.8.1 (2026-07-22, post-0.8.0)

The first full live `capture_lookups` run (Dev-thoughtworks, 67 lookups) failed and proved the
"run it live" rule again — **two latent bugs plus one misleading diagnosis**, all fixed in
`refdata.rs` / `diagnostics.rs`:

1. **Root cause — enumeration assumed every lookup table has `Name`.** `SysLookup` had a lookup
   registered on the system view `VwSysSSPEntitySchemaAccessList`, which has no `"Name"` column.
   One bad table inside the single `UNION ALL` fails the *entire* capture with PostgreSQL
   `42703: column "Name" does not exist` (found at POSITION 5250 by reconstructing the generated
   SQL offsets). Fix: `enumeration_sql()` now has a `WHERE EXISTS (information_schema.columns …
   column_name = 'Name')` filter, mirroring the existing `HasDescription` check — no-Name tables
   (and registry rows whose table doesn't exist at all) never reach `capture_sql`. v1's
   Id/Name/Description model couldn't compare or migrate them anyway.
2. **Duplicate registry rows duplicate captured data.** `LeadType` appears twice in `SysLookup`
   (two rows, same schema) → its `SELECT` appeared twice in the UNION → every row captured twice.
   Fix: new pure `dedupe_by_table()` (first entry wins) applied in `enumerate()`.
3. **The failure was misdiagnosed as "cliogate missing".** refdata's no-CSV fallback message
   mentions "cliogate … (clio install-gate)", which is exactly what the `cliogate-missing`
   diagnostics rule matches — so the UI blamed cliogate while the log plainly showed the SQL
   error. Fix: new `sql-column-missing` rule (`column` + `does not exist` + `42703`) ordered
   *above* `cliogate-missing`; regression test feeds the real dual-message log text and asserts
   the SQL rule wins.

Validated: `cargo test` = **72 passed** (3 new: enumeration filter, dedupe, rule precedence).
**Re-verified live (2026-07-22)** — clio ran the fixed queries against Dev-thoughtworks:
enumeration returns **63** lookups (the 4 Name-less tables — `VwSysSSPEntitySchemaAccessList`,
`BulkEmailCountLimit`, `SysModuleReport`, `WebServiceURL` — filtered), and the capture `UNION ALL`
that previously died on `42703` now **succeeds, 1,224 values across 62 tables** (LeadType deduped
2→1). The full app was then run via `dev.cmd` and a **Compare → Lookups capture of Dev-thoughtworks
completed and wrote its snapshot through the GUI**. The read/capture path is confirmed end-to-end;
the **write path (`migrate_lookups`) is still the unexercised release gate**. The incidental
`[WAR] clio 8.1.0.88 available` update notice in the log is unrelated.

## Package lock state (v0.7.0, 2026-07-21)

The Packages table has a **State** column — Locked / Unlocked / `—` — and the Lock and Unlock
menu items collapsed into one state-aware toggle. Previously both were always offered with no
indication of which one applied.

- `clio list-packages -j` returns **no lock field at all** (Descriptor carries UId,
  PackageVersion, Name, Type, ProjectPath, ModifiedOnUtc, Maintainer, DependsOn — nothing else).
  So `packages::package_lock_states(env)` reads `SysPackage."InstallType"` over SQL instead:
  **1 = locked, 0 = unlocked**, which is the same column `clio lock-package` writes.
- Verified live: dev-834 reports 434 locked / 11 unlocked, and every unlocked one is a Customer
  or Qnovate package. 187559-crm-bundle reports 447 / 10.
- Same contract as `applications::application_extras`: one query for the whole list, and an
  environment without cliogate gets an `Err` the screen swallows — the column reads `—` and
  nothing else changes. Do not promote that failure into a banner; the listing is still valid.
- The state is re-read after a lock/unlock job succeeds, not polled.
- Cost: one extra SQL round-trip per environment on the Packages screen and on the launch
  prefetch. Against a cloud environment that is a real network call, not a local one.

**Not built: the compile half of roadmap #5** — see the compile-package entry under "clio
behaviours worth knowing". It was written, then removed once the wait proved unbounded.

## Application details (v0.6.0, 2026-07-21)

`clio list-apps --json` returns only Id, Name, Code, Version, Description — which is why the
Applications tiles were so thin. Everything else Creatio shows on an application page lives in
`SysInstalledApp`, and the package membership in `SysPackageInInstalledApp`.

- **`applications::application_extras(env)`** — one SQL query for the whole list (developer,
  created/modified, required platform version, package count), keyed by code. The tiles show
  developer, package count, "Needs Creatio" and updated date, and the filter matches developer
  too, so `Customer` finds your own apps and `Creatio` the stock ones.
- **`applications::application_details(env, code)`** — the drill-down behind the tile's
  **Details** button. It assembles from two sources that fail **independently**:
  `clio get-app-info --code X --json` (pages, schema prefix — no cliogate needed) and SQL
  (descriptor row, package list). Whatever answered is rendered; whatever did not becomes a note
  at the bottom of the dialog rather than an error.
- Enrichment is silent on failure by design: an environment without cliogate shows exactly the
  tiles it showed before. Absent columns blank their field instead of failing the dialog, and the
  application code is escaped into the SQL literal (tested).
- Dates arrive as `dd-MM-yyyy HH:mm:ss`, which `Date` cannot parse — `modules/applications/
  format.ts` handles it. That helper lives in its own file because React Fast Refresh cannot
  hot-patch a component module that exports non-components; keep it that way.

**This release was exercised in `dev.cmd` before publishing** — the first in the 0.5.x line that
was. Running it immediately exposed four layout faults that types and tests cannot catch: cards of
unequal height (a missing fact row shortened a card and lifted its buttons out of line), a
collapsing row where a value was absent, a 50/50 label column, and footer buttons sized to their
own labels. Cards are now `h-full` with `flex-1` content, all four fact rows always render with an
em dash for missing values, the label column is a fixed `7rem`, and both footer buttons take
`flex-1`.

**Still unconfirmed:** only the *tiles* were seen running. Nobody opened the **Details** dialog
before release, so its layout and the `dd-MM-yyyy` date parsing are backed by unit tests and live
query output, not by having been looked at. Check it first if something there seems wrong.

## SQL errors are failures again + package filters (v0.5.2, 2026-07-21)

**v0.5.1 shipped a regression: every rejected SQL statement reported success.** Read the section
below first for what it was trying to fix, then this.

`is_failure` was defined as "non-zero exit or an `[ERR]` line", which was never checked against a
statement the *database* rejects. Verified live on dev-834 (clio 8.1.0.84, PostgreSQL): clio
**exits 0, prints no `[ERR]`**, and writes the engine's error followed by `Done`. So the new
"no result file means success" path swallowed every SQL error — a query with a bad column showed
a green "Statement ran successfully."

- `sql::database_error(out)` now reads the output for the error itself: a PostgreSQL SQLSTATE
  header (`42703:`, `42P01:` — five alphanumerics, a colon, a message) or one of
  `DB_ERROR_PHRASES` for engines that format differently. `is_failure` includes it, so both
  `run_sql` and `export_sql` are covered.
- `friendly_error` leads with that message plus its `POSITION:` hint rather than the echoed SQL.
- clio writes no result file for a successful statement **and** for a query that matched nothing,
  so the output alone cannot tell them apart — `sql::returns_rows(query)` classifies the SQL, and
  `SqlResult.statement` carries it to the UI ("Statement ran successfully." vs "Query ran — 0 rows
  returned."). Getting that classification wrong only picks the wrong wording, never a wrong
  outcome. Export is gated on `hasGrid` (columns present).
- All four cases were reproduced against dev-834 before the fix and the tests are built from that
  captured output: rejected query, rejected statement, zero-row query, matching query.

Also in this release: the Packages screen filters **by field** instead of one blurred match.
The text box is now name + version only, and **Maintainer is its own dropdown** built from the
loaded packages with per-maintainer counts; the two narrow together. The maintainer selection
resets on environment change (maintainers differ per environment, and a stale one hides
everything), a "Clear filters" button appears when either is active, and filtering to nothing says
so instead of rendering an empty table. No backend change — `PackageInfo` already carried
`maintainer`.

**Lesson recorded:** v0.5.0 and v0.5.1 both shipped on `tsc` + unit tests without the app ever
being run. The v0.5.1 regression would have been caught by executing one bad query. Claude Code
cannot drive the native window (its browser tools target a web view), so **a human must exercise
SQL and Packages before a release that touches them**.

## Statements are not failed queries (v0.5.1, 2026-07-21)

Running an `UPDATE` in the SQL screen reported an error whose text was clio's own **success**
output — the `[WAR]` version notice, the echoed statement, and `Done`. `run_sql` treated "clio
produced no CSV" as a failure alongside the exit code and `[ERR]` checks, but a statement with no
result set never produces one; `friendly_error` then found no `[ERR]` line and fell back to
dumping the raw output as the error.

- Failure detection is now `sql::is_failure(code, out)` — exit code or `[ERR]` only. A missing
  result file is a separate, successful path returning an empty `SqlResult`.
  **Superseded by v0.5.2**, which had to add database-error detection: those two signals alone
  miss every statement the database itself rejects.
- The SQL screen reports that case as a green **"Statement ran successfully."** line and hides the
  results card; CSV/Excel are disabled with a tooltip, because there is nothing to export.
  A `SELECT` is unchanged. The distinction is `columns.length === 0`.
- `export_sql` had the same conflation and now says so plainly instead of dumping output.
- `friendly_error` strips clio's `[WAR]`/`[INF]` chatter from the fallback text — the
  version-update warning prefixes every clio command and is never why a query failed.
- Reproduced before fixing with `DO $$ BEGIN END $$;` against dev-834: exit 0, no `[ERR]`, no CSV.

## Automatic update notice (v0.5.0, 2026-07-21)

Users no longer have to visit Settings and press "Check for updates" to learn that a release
exists. `src/modules/updates/UpdateBanner.tsx` sits in the header under `ClioBanner` and checks
the signed feed **4 s after launch and every 6 hours** the app stays open, then shows a
dismissible strip with Install and restart / What's new / ✕.

- Dismissal is stored per version (`creatio-devhub.app-update-dismissed.v1`), so hiding one
  release does not hide the next — same contract as the clio banner.
- **A failed check is silent.** DevHub is useful offline and behind VPNs that block github.com;
  Settings → DevHub updates still reports the reason when asked. Do not turn this into a toast.
- The check/download/relaunch calls moved into `src/lib/appUpdate.ts`
  (`checkForAppUpdate`, `installAppUpdate`), which the banner and the Settings card both use —
  the two paths must not drift on verification or restart behaviour.
- Signature verification is unchanged: `check()` only resolves for a release signed with the
  project key in `tauri.conf.json`, so the banner cannot offer an unsigned build.
- There is no "check automatically" opt-out setting. Per-version dismissal was judged enough;
  add a toggle if anyone finds the banner intrusive.

## Current state

Creatio DevHub is a Windows-first Tauri 2 desktop application that provides a visual workbench over
the installed clio, Git, and GitHub CLI tools. It manages Creatio environments, source-controlled
workspaces, packages, applications, background jobs, GitHub identity, and signed application
updates.

The main milestones are complete:

| Area | Status |
|---|---|
| Environment registration, default selection, ping/open, cliogate installation | Complete |
| Workspace creation/registration, pull, diff, commit, history, Git remote push | Complete |
| Empty-first workspace + selective add-package UI + create GitHub repo from app | Complete |
| Global job toaster (top-right running-job indicator) | Complete |
| Auto-captured catalog cache (background prefetch on launch / env change) | Complete |
| Deploy from GitHub (clone/refresh a repo and push-workspace into an env) | Complete |
| SQL query runner with CSV/Excel export (grid capped at 5,000 rows) | Complete |
| clio CLI management — install / update / repair, with failure diagnosis | Complete |
| Push workspace to Creatio with drift guard and backup controls | Complete |
| Package browsing, actions, archive installation, Git workspace bridge | Complete |
| Package deployment between registered environments | Complete |
| Whole-application deployment between registered environments | Complete |
| GitHub login/account switching, Git identity, remote-ahead conflict guard | Complete |
| Phase-aware jobs and safe cancellation | Complete |
| Persistent package/application catalog cache | Complete |
| Creatio-inspired original DevHub UI | Complete |
| Signed GitHub Releases updater | Complete and published |
| Application descriptor details — enriched tiles + drill-down dialog | Complete |
| Automatic new-release notice in the header (no manual check needed) | Complete |
| Server errors (HTTP 500) fail a job even when the tool exits 0 | Complete |

## Verified release state

Release `v0.6.0` was built and published by GitHub Actions (2026-07-21).

- Workflow: `.github/workflows/release.yml` (run 29835736018, conclusion: success)
- Release: `DevHub v0.6.0` (not a draft)
- Update endpoint:
  `https://github.com/SmitSuthar8834/creatio-devhub/releases/latest/download/latest.json`
- Published artifacts:
  - `creatio-devhub_0.6.0_x64-setup.exe` (NSIS setup executable)
  - `creatio-devhub_0.6.0_x64-setup.exe.sig` (NSIS signature)
  - `creatio-devhub_0.6.0_x64_en-US.msi` (MSI installer)
  - `creatio-devhub_0.6.0_x64_en-US.msi.sig` (MSI signature)
  - `latest.json`
- `latest.json` reports version `0.6.0` with signed `windows-x86_64`, `windows-x86_64-nsis`,
  and `windows-x86_64-msi` entries.

Released so far: v0.2.1, v0.2.3 → v0.2.9, v0.3.0 → v0.3.2, v0.4.0, v0.5.0 → v0.5.2, v0.6.0.
The first
published/verified release was `v0.2.1`; the signed updater flow has been stable across every
release since.

The repository Actions secret `TAURI_SIGNING_PRIVATE_KEY` is configured. The private key is not in
Git and must remain private. On the original development machine it is stored at:

```text
%USERPROFILE%\.tauri\creatio-devhub.key
```

Back this key up securely. Losing it prevents existing installations from accepting future
updates. Never print it in logs, add it to the repository, or upload it as a release asset.

## Empty-first workspaces + GitHub-from-app (v0.2.4, 2026-07-18)

New onboarding flow so a workspace no longer has to download every package up front:

- **Start empty**: `create_workspace_flow` takes a new `skip_restore: bool`. When set, it runs
  `create-workspace` + git init + an "Initial empty workspace" commit and **skips
  `restore-workspace`** (no packages pulled, no drift baseline recorded yet). The New Workspace
  wizard defaults to "Start empty"; "Pull everything now" is the opt-in.
- **Add packages selectively**: the pre-existing `add_package_to_workspace` command
  (`cfg-worspace` — note that misspelling is clio's own alias, verified against installed clio,
  aliases `cfgw` — + `restore-workspace`) is now surfaced in the UI. `WorkspaceDetail` has an
  "➕ Add package" action + a package-picker dialog fed by `list_packages`.
- **Create GitHub repo from the app**: new `create_github_repo` command runs
  `gh repo create <name> [--private|--public] --source <dir> --remote origin --push`, wiring
  `origin` and pushing the initial commit in one job. Exposed via a guidance banner and the
  History-tab remote bar (shown only when the workspace has no remote yet). Requires `gh` signed
  in (Settings → GitHub). Non-cancellable job (push is the unsafe phase).
- **Guidance banner + progress strip** in `WorkspaceDetail`
  (✅ Workspace → Packages → GitHub repo → Pushed) drives new users through the four steps.
- Touched: `src-tauri/src/workspaces.rs`, `src-tauri/src/lib.rs`, `src/lib/ipc.ts`,
  `src/modules/workspaces/NewWorkspaceWizard.tsx`, `src/modules/workspaces/WorkspaceDetail.tsx`,
  `src/App.css`. Validated: `tsc --noEmit` clean, `cargo check`/`cargo test --lib` = 13 passed,
  full `tauri dev` build ran and launched. Shipped in v0.2.4.

## Global job toaster + auto-captured catalog cache (v0.2.5, 2026-07-18)

Two additive UX features:

- **Global job toaster** (`src/modules/jobs/JobToaster.tsx`, mounted once in `App.tsx`): a
  top-right indicator that subscribes to `job-update` and seeds from `get_jobs` on mount. Running
  jobs stay pinned (pulsing ⏳ with label/phase/env); terminal jobs flash ✅/✗/⊘ and auto-dismiss
  after 6s. Clicking opens the Jobs screen. Purely frontend — no backend change.
- **Auto-captured catalog state** (`src-tauri/src/catalog.rs` → `prefetch_env_catalog`): a silent
  background thread runs read-only `list-packages` + `list-apps` for an environment and writes both
  into `catalog-cache.json`, then emits `catalog-updated`. Wired to fire on app launch (active env)
  and whenever the default env changes — `clio::set_default_environment` now takes `AppHandle` and
  emits `environment-changed`. Applications and Packages pages listen for `catalog-updated` and
  reload from the freshened cache live.
- Touched: `src-tauri/src/catalog.rs` (new), `src-tauri/src/lib.rs`, `src-tauri/src/clio.rs`,
  `src/lib/ipc.ts`, `src/App.tsx`, `src/App.css`, `src/modules/jobs/JobToaster.tsx` (new),
  `src/modules/applications/ApplicationsPage.tsx`, `src/modules/packages/PackagesPage.tsx`.
  Validated: `tsc --noEmit` clean, `cargo check`/`cargo test --lib` = 13 passed, full `tauri dev`
  build ran and launched, shell renders with no new runtime errors. Shipped in v0.2.5.

## Deploy from GitHub (v0.2.6, 2026-07-18)

GitHub → Creatio: install a workspace straight from a repository into an environment — e.g. to
restore a broken environment from known-good source, or move a repo's packages onto a fresh env.

- `github::list_github_repos` (`gh repo list --json …`) and `github::list_repo_branches`
  (`gh api repos/{owner}/{repo}/branches`) feed the picker.
- `workspaces::deploy_from_github` runs as one env-locked job: clone via `gh repo clone` (git
  clone fallback) into `<destParent>/<repo-leaf>`, or **hard-refresh** an existing clone
  (`fetch` + `checkout` + `reset --hard origin/<branch>`); verify the `.clio` folder; then
  `push-workspace -e <targetEnv>` (unsafe/non-cancellable, `--skip-backup true` when the user
  opts out). Optionally registers the clone as a workspace (dedup by path).
- Frontend: `src/modules/workspaces/DeployFromGithubDialog.tsx`, opened from a "Deploy from
  GitHub" button on the Workspaces page. Repo dropdown (from gh) with a graceful manual
  owner/name + clone-URL fallback when gh isn't authed; branch dropdown/loader; target env;
  destination + Browse; "backup first" and "keep as workspace" toggles; an overwrite warning.
- Touched: `src-tauri/src/github.rs`, `src-tauri/src/workspaces.rs`, `src-tauri/src/lib.rs`,
  `src/lib/ipc.ts`, `src/modules/workspaces/DeployFromGithubDialog.tsx` (new),
  `src/modules/workspaces/WorkspacesPage.tsx`. Validated: `tsc` clean, `cargo check`/`cargo test
  --lib` = 13 passed, full `tauri dev` build + launch, dialog renders with working manual
  fallback. Shipped in v0.2.6.

## SQL query runner + CSV/Excel export (v0.2.7, 2026-07-19)

- Added a dedicated SQL workspace with environment selection, a keyboard-friendly query editor,
  sticky result headers, row/column metadata, and a 2,000-row display cap.
- Queries run through `clio execute-sql-script`; credentials remain owned by clio and the target
  environment must have cliogate installed.
- Complete results can be exported directly to semicolon-delimited CSV or Excel (`.xlsx`).
- Added a dedicated sidebar icon, friendly clio error handling, and parser tests for quoted
  semicolon values.
- Validated with TypeScript, Vite production build, Rust compilation, and 15 passing Rust tests.
  A live read-only query and both export formats succeeded against `187559-crm-bundle`.

## Clear deployment failure summaries (v0.2.8, 2026-07-19)

- Failed jobs now show a plain-language outcome and a separate technical-details panel above the
  complete raw log.
- Creatio package deployments that finish only partially are identified explicitly. For example,
  a locally modified target schema is named, the skipped content is explained, and the original
  Creatio message plus clio exit code remain visible.
- Bare trailing messages such as `[ERR] - Error` no longer hide the meaningful failure that
  appeared earlier in the installation log.

## Saved SQL queries (v0.2.9, 2026-07-19)

- SQL queries can be named and stored locally in the DevHub webview profile.
- Saved queries retain their original environment and can be reopened, updated, copied under a
  new name, deleted, or run again immediately.
- A saved query is never silently redirected: rerunning is blocked when its original environment
  is no longer registered.

## clio CLI management + SQL row cap (v0.3.0, 2026-07-20)

DevHub now manages the clio CLI it depends on, and diagnoses clio failures instead of
surfacing raw output.

- **SQL grid cap raised 2,000 → 5,000 rows** (`DISPLAY_CAP` in `src-tauri/src/sql.rs`).
  Exports are still uncapped (clio writes the file) and `row_count` reports the true total,
  so the "grid shows first N" pill stays accurate.
- **`clio::clio_status`** parses `clio ver`: installed version (`clio:`), cliogate (`gate:`),
  the update notice (`clio X is available`), dotnet presence, and a `broken` flag when clio
  starts but can't load its own assemblies.
- **`clio::install_or_update_clio(mode)`** — `install` | `update` | `repair`. Repair does
  `dotnet tool uninstall clio -g` then `install`, which is the only thing that fixes a damaged
  install. Output is **captured, not streamed**, so failures can be diagnosed.
- **`clio::diagnose(output)`** maps known failures to actionable guidance and is also applied to
  `list_applications` / `list_packages` errors:
  - `Could not load file or assembly …` → damaged install, use Repair.
  - `Access to the path … is denied` / `failed to uninstall tool package` → clio's files are
    **locked by a running clio process**; finish jobs / close terminals, or run as admin.
    (This is the real cause of a failed `dotnet tool update clio -g` while DevHub is busy.)
  - cliogate missing, dotnet missing.
- **`src/modules/clio/ClioBanner.tsx`** — header strip: blocking red banner when clio is missing
  (Install) or damaged (Repair); dismissible blue banner when an update exists (Update + Repair),
  dismissal stored per version. Re-checks itself when a clio job settles.

Tests added: `clio::tests::diagnoses_real_clio_failures` (asserts against both real failures above)
and `parses_clio_version_and_update_notice`. Suite: **17 passing**.

## Parked work and known environment state

- Branch **`parked/team-locks`** (local, not pushed) holds a complete team-locks feature
  (DataService `UsrTeamLock`/`UsrTeamActivity` shipped in a bundled package, hard package-lock via
  `clio lock-package`, push conflict pre-check). **Parked deliberately**: true per-schema "only I
  can edit" enforcement is only available through Creatio's **SVN** native lock, which is not
  achievable via Git/GitHub (Creatio's native VC tooling is SVN-only; Git has no lock primitive).
- Test env `187559-crm-bundle` still holds leftover experiment objects
  (`UsrDevHubCollabPackage`, an empty `UsrDevHubCollab`, and unused `UsrDevHubLockItem` /
  `UsrDevHubActivityItem` in `Custom`). Harmless; cliogate is installed there and should stay.

## Website (branch `gh-pages`)

Live at <https://smitsuthar8834.github.io/creatio-devhub/> — a landing/download page.

- **Deployment**: the page source is a single standalone `index.html` on the orphan-style
  `gh-pages` branch. It is deployed with a `git worktree` checkout of `gh-pages` so `main` is
  never touched:

  ```bash
  git worktree add /path/tmp gh-pages
  cp index.html /path/tmp/ && cd /path/tmp && git add -A && git commit && git push origin gh-pages
  git worktree remove /path/tmp --force
  ```

- **Design**: dark-only ("OLED") theme, slate ground with a `#22C55E` accent, Inter +
  JetBrains Mono, inline SVG icons (no emoji), a stylized DevHub window in the hero, feature grid,
  workflow strip, changelog, requirements, and a download CTA.
- **Live data**: on load it queries the GitHub Releases API to show real installer download
  counts, latest version and release count, and to **re-target every download link at the newest
  release's assets** — so the buttons don't go stale between releases. There is a static fallback
  if the fetch fails.
- **Footer**: credit "Developed by Smit Suthar", support email `sutharsmit574@gmail.com`, and an
  explicit "independent tool, not affiliated with Creatio" disclaimer.
- GitHub Pages is configured to serve the `gh-pages` branch root.

## Architecture

```text
React + TypeScript UI
        |
        | typed Tauri invoke/events
        v
Rust command layer
        |
        +-- Job engine: logs, phases, cancellation, process trees, environment locks
        +-- clio process adapter: Creatio environment/package/application operations
        +-- Git adapter: status, diff, commit, history, remote synchronization
        +-- GitHub adapter: gh authentication and global Git identity
        +-- App-data state: workspace registry and catalog cache
```

There is no DevHub server component. The desktop application invokes local command-line tools and
those tools communicate with Creatio or GitHub.

## Branding / icons (added 2026-07-18, later session)

- Icon source sheet: `Downloads\Generated image 1.png` cropped into `src/assets/icons/`
  (logo-mark, logo-wordmark, environments, workspaces, packages, applications, jobs, settings,
  local-desktop, app-icon-512). White backgrounds converted to alpha (luminance ramp ≥215) so the
  icons sit on the dark sidebar.
- Sidebar (`App.tsx` NAV + brand) uses the PNG icons; active nav item applies
  `filter: brightness(0) invert(1)`. Wordmark (`logo-wordmark.png`, white bg) is reserved for
  README/light surfaces.
- Sidebar switched from navy to a light treatment (`--side-bg #fbfcfe`, dark ink, border-right)
  so the blue-gradient icons render in true color — matches the icon sheet's design language.
- App icons (`src-tauri/icons/*` — .ico/.icns/pngs/Android mipmaps) regenerated from
  `app-icon-512.png` via `npx tauri icon`.
- Shipped in v0.2.3. A dedicated SQL (database) sidebar icon was added in v0.2.7.

## Important design decisions

| Decision | Rationale |
|---|---|
| Tauri 2 + Rust + React/TypeScript | Small Windows installer and reliable native process control |
| Invoke installed clio instead of calling Creatio APIs | Reuses supported clio behavior and registered environments |
| Never store Creatio credentials in DevHub | clio remains the credential owner |
| Use the system Git and GitHub CLI | Reuses the user's credential manager and active GitHub account |
| Represent every long mutation as a job | Provides streamed logs, serialization, status, and safe cancellation |
| Lock jobs per environment | Prevents concurrent mutations against the same Creatio environment |
| Typed confirmation for destructive/deployment actions | Makes the intended target explicit |
| Cache catalogs, not credentials | Improves navigation speed without expanding secret storage |
| Verify updater signatures | Prevents installation of releases not signed by the project key |

## Persistent state

DevHub writes only operational metadata beneath the Tauri application-data directory:

- `workspaces.json` — registered workspace metadata.
- `catalog-cache.json` — package and application lists keyed by environment, with timestamps.

The catalog cache is used immediately when revisiting a page or restarting DevHub. **Refresh**
bypasses it and updates the entry from clio. Neither file contains Creatio passwords or OAuth
client secrets.

clio owns environment configuration and credentials in its own application settings. Do not expose
raw `clio env` output because it can include plaintext passwords.

## Job and cancellation rules

- Jobs queued behind an environment lock can be cancelled.
- Safe local phases may terminate the complete Windows child-process tree.
- Package download, workspace restore, and GitHub browser login can be cancellable.
- Installation, server compilation, package deletion/activation/locking, credential changes, and
  Git pushes become non-cancellable once their unsafe phase begins.
- Source and target locks are acquired in stable sorted order for cross-environment deployment.

Do not make unsafe phases cancellable without proving that interruption cannot leave Creatio,
Git, or local workspace state inconsistent.

## Source map

```text
src/
  App.tsx                         shell and navigation
  App.css                         shared visual system
  lib/ipc.ts                      typed frontend IPC contract
  modules/environments/           environment hub and registration
  modules/workspaces/             workspace list, wizard, changes, history,
                                  DeployFromGithubDialog
  modules/packages/               package manager and package deployment
  modules/applications/           application catalog and deployment
  modules/sql/SqlPage.tsx         SQL editor, results grid, saved queries, CSV/XLSX export
  modules/clio/ClioBanner.tsx     clio install / update / repair header banner
  modules/jobs/                   job list, logs, cancellation, JobToaster
  modules/settings/               defaults, GitHub identity, updater

src-tauri/src/
  lib.rs                          plugin/state/command registration
  jobs.rs                         job state, locks, streaming, cancellation
  clio.rs                         clio helpers, parsing, clio_status /
                                  install_or_update_clio / diagnose
  cache.rs                        persistent catalog cache
  catalog.rs                      background catalog prefetch (packages + apps)
  sql.rs                          SQL execution and CSV/XLSX export via clio
  workspaces.rs                   workspace registry and synchronization
  packages.rs                     package actions and deployment
  applications.rs                 application listing and deployment
  git.rs                          local Git and remote-ahead checks
  github.rs                       GitHub CLI auth, repo/branch listing, Git identity
  tools.rs                        locating clio/git/gh/dotnet (PATH, live registry
                                  PATH, well-known dirs, user overrides)
  diagnostics.rs                  failure-signature catalog: raw CLI output ->
                                  summary, cause, and resolution steps

src-tauri/tauri.conf.json         app, bundling, and updater configuration
src-tauri/capabilities/           frontend permissions
.github/workflows/release.yml     signed Windows release automation
dev.cmd                           development wrapper
build.cmd                         local production build wrapper
README.md                         user and contributor documentation
```

## Build and validation

This development machine has both a partial Visual Studio Community toolset and a complete Visual
Studio Build Tools installation. Raw Cargo commands may select the incomplete toolset and fail with
`LNK1104: cannot open msvcrt.lib`.

Use the wrappers:

```powershell
.\dev.cmd
.\build.cmd
```

Frontend validation:

```powershell
npx tsc --noEmit
npm run build
```

Rust validation from the Build Tools environment:

```powershell
cd src-tauri
cargo test --lib
```

Latest verified result (2026-07-21, v0.6.0):

- TypeScript check: passed
- Vite production build: passed
- Rust tests: **51 passed, 0 failed**
- GitHub v0.6.0 release workflow: passed (run 29835736018)
- Published artifacts: signed NSIS + MSI, both signatures, `latest.json`
- Public updater feed: verified reporting `0.6.0` with signatures on all three platform entries
- Website `gh-pages` updated (commit 656135c)

Not covered by that run: the update banner has never been *seen* rendering, because the
development machine is always on the newest version and the check therefore finds nothing. To
exercise it, temporarily lower the version in `tauri.conf.json` below the published release and
run `dev.cmd`.

## Publishing the next release

1. Pull `main` and ensure the working tree is clean.
2. Implement and validate the change.
3. Increase the same version in **all four** files (a mismatch is easy to miss):
   - `package.json`
   - `src-tauri/Cargo.toml`
   - `src-tauri/tauri.conf.json`
   - `src-tauri/Cargo.lock` (the `creatio-devhub` package entry; `cargo` also rewrites it on the
     next build)

   Edit these with a tool that preserves encoding — a UTF-8 BOM in the JSON breaks the workflow
   (see the v0.2.2 note at the top). Verify with a BOM check before committing.
4. Update README and this handoff when behavior or operational requirements change.
5. Commit and push `main`.
6. Create and push the matching tag (example for the next patch after v0.3.0):

```powershell
git tag -a v0.3.1 -m "Creatio DevHub v0.3.1"
git push origin v0.3.1
```

7. Monitor **Publish DevHub release** in GitHub Actions.
8. Confirm the release contains both installers, both signatures, and `latest.json`.
9. Confirm the latest endpoint reports the new version before announcing the update.
10. Update the website (`gh-pages`) so the changelog and static fallbacks mention the new version.
    The download buttons re-target the newest release from the GitHub API at runtime, so they do
    not strictly need editing — but the changelog and fallback labels do.

Never reuse a version tag for different source. If a release fails, fix the workflow or secret and
rerun the same failed workflow only when its source/tag is unchanged.

## SmartScreen blocks the installer (open, costs money to fix)

Every new user hits *"Windows protected your PC — unknown publisher"* and must click
**More info → Run anyway**. Existing installs updating in place are not affected.

**The updater signature is not Windows code signing.** They solve different problems and are
easy to confuse:

- `TAURI_SIGNING_PRIVATE_KEY` (minisign) proves an update payload came from this project. Only
  DevHub's own updater checks it. Windows has never heard of that key.
- **Authenticode** is what Windows checks, and `src-tauri/tauri.conf.json` has no
  `bundle.windows.certificateThumbprint` or `signCommand` — the installers ship unsigned.

To remove the warning you need an Authenticode certificate. Since the 2023 CA/Browser Forum rule
change, **every** OV certificate must live on a hardware token or cloud HSM, so the old "cheap
cert in a file" route no longer exists:

- **Azure Trusted Signing** — roughly $10/month, cloud-signed, no token to keep. Cheapest realistic
  option; requires identity validation and Tauri's `bundle.windows.signCommand` pointing at the
  Azure signing tool. Check current individual-vs-organisation eligibility before committing.
- **OV certificate** (~$200–400/yr + token) — reputation with SmartScreen still builds gradually,
  so early downloads may warn anyway until enough installs accumulate.
- **EV certificate** (~$300–600/yr, hardware token) — immediate SmartScreen reputation, the only
  option that removes the warning from day one.

Whichever is chosen, the certificate must be reachable from the GitHub Actions runner, which means
another repository secret alongside the updater key. Until then, keep the warning documented in
the README and on the website download section rather than letting users guess.

## If the repository goes private

Planned 2026-07-21, **not executed** — the repo is still public. Written up because flipping the
visibility toggle alone would break distribution, and one of the breakages is silent.

Private repositories cost nothing in version control: full history, branches, tags, Actions and
unlimited collaborators all work. What breaks is that source, releases, updater feed and website
currently live in **one public repo**, and three of those stop working for anonymous clients.

### What breaks

1. **The updater dies silently.** `tauri.conf.json` points at
   `releases/latest/download/latest.json`; on a private repo that URL needs authentication and the
   Tauri updater has no credentials. Worse, `UpdateBanner` treats a failed check as silence by
   design (see the v0.5.0 section), so **no user sees anything** — they simply stop receiving
   updates. Only Settings → DevHub updates would show the failure.
2. **GitHub Pages may stop publishing.** Pages from a private repo requires GitHub Pro on a
   personal account. Confirm the plan before switching; on Free the site goes away.
3. **Every download link 404s** for anonymous visitors, and the landing page's live
   download-count fetch falls back to its static numbers.
4. Actions minutes stop being unlimited (public repos are free; private draws on the monthly
   quota). At ~8 minutes per release this is not a real constraint.

Unaffected: the signing key and pubkey, DevHub's own "Deploy from GitHub" (it uses the user's
authenticated `gh`), and existing clones. **Going private stops future exposure — it does not
retract code already published.** There is at least one fork, which keeps its copy regardless.

### The two shapes

- **Private source + public releases repo** — flip `creatio-devhub` private (all history hidden
  at once), add a public `creatio-devhub-releases` holding only installers, `latest.json` and the
  site. Costs an updater-endpoint change and a new site URL.
- **Public release host + private source repo** — this repo stays public but carries only the site
  and release assets; source moves to a new private repo. The updater endpoint and site URL never
  change, so existing installs are never at risk — but this repo's history still contains the
  source unless it is purged, and purging cannot reach the fork.

### Sequencing — the part that strands users if rushed

If the updater endpoint changes, **ship a release pointing at the new endpoint while the repo is
still public**, confirm the new feed serves it, and only then flip the switch. In the other order,
anyone who has not yet updated is stranded permanently: their client polls a URL it can no longer
read, says nothing, and they must find and reinstall by hand.

### Workflow change required

`release.yml` publishes with `tauri-action` and the built-in `GITHUB_TOKEN`, which can only
release into its own repo. To publish elsewhere, add a step after the build that copies the five
assets forward and rewrites the URLs inside `latest.json` (they are absolute and would otherwise
point back at the private repo):

```yaml
- name: Copy the release to the public distribution repo
  env:
    GH_TOKEN: ${{ secrets.RELEASES_REPO_TOKEN }}   # PAT with contents:write on the public repo
  run: |
    gh release download "$TAG" -D dist -R OWNER/creatio-devhub
    # point every platform url at the public repo before republishing
    node -e "const f='dist/latest.json',j=require('./'+f);for(const p of Object.values(j.platforms))
      p.url=p.url.replace(/.*\/(?=[^/]+$)/,'https://github.com/OWNER/creatio-devhub-releases/releases/download/$TAG/');
      require('fs').writeFileSync(f,JSON.stringify(j,null,2))"
    gh release create "$TAG" dist/* -R OWNER/creatio-devhub-releases --title "DevHub $TAG"
```

The signing key stays in the private repo; only signed output crosses over, so the pubkey in
`tauri.conf.json` keeps verifying and existing installs accept the updates.

## Known limitations and risks

- Mutating package operations have not all been exercised against a disposable live environment.
- Cross-environment package and whole-application deployments require final end-to-end validation
  against non-production targets before production use.
- Application deployment follows clio's descriptor and package dependency behavior; it is not a
  full Creatio ALM approval/promotion workflow.
- The workspace drift guard compares package names and versions. Server-side changes without a
  version change may not be detected.
- Workspace synchronization and some package operations require a compatible cliogate installation.
- DevHub currently depends on separately installed clio, Git, and GitHub CLI binaries. They are
  located by `tools.rs` rather than by `Command::new` alone: the inherited PATH is Explorer's
  login-time snapshot, so a tool installed after the last sign-in would otherwise read as "not
  installed" while working in any terminal. Resolution order is user override → inherited PATH →
  the live PATH read back from HKCU/HKLM via `reg.exe` → well-known install directories, with
  every `PATHEXT` variant tried so `.cmd`/`.bat` shims (scoop, npm) resolve too. Results are
  memoized until Refresh/Re-scan. Overrides are stored in app-data `tool-paths.json` and edited
  under Settings → Command-line tools.
- The update feed is public. Moving the source repository to private access requires a separate
  public release feed or an authenticated update service.
- Job history persists under app-data `jobs/` (`history.json`, capped at 200, + per-job log
  files written at completion). Jobs active during an app exit are shown as
  failed/"interrupted" on next start. Logs stream to memory during a run and are only flushed
  to disk at finish — a hard crash mid-job loses that job's log tail (record survives).
- Catalog cache invalidation is explicit via Refresh or selected successful mutations; it is not a
  real-time subscription to Creatio changes.
- **Killing the DevHub window orphans the clio processes it spawned.** Observed 2026-07-21: a
  clio child outlived the app by 86 minutes after the window was force-closed. Job cancellation
  terminates the whole Windows process tree, but application exit does not — nothing reaps
  children on shutdown. This matters beyond tidiness: a lingering clio process is exactly what
  makes `dotnet tool update clio -g` fail with `Access to the path … is denied`, so a user who
  force-closes DevHub can then find the Update button in the clio banner failing for no visible
  reason. A shutdown hook terminating live job process trees would fix it.
- **Windows only.** No macOS/Linux build exists — the release workflow produces NSIS + MSI on a
  Windows runner. Tauri and the clio/Git/gh dependencies are all cross-platform, so a macOS build
  is feasible, but it needs a `macos` runner job, a replacement for the Windows-specific
  process-tree cancellation, and Apple signing/notarization to avoid Gatekeeper warnings.
- **Open bug — "Start empty" workspace is not actually empty.** `create_workspace_flow` still
  passes `-e <env>` to `clio create-workspace`, so clio connects and populates the package
  selection instead of scaffolding an empty workspace (and it fails outright if the environment is
  unreachable or its credentials are stale). The correct invocation is
  `clio createw <name> --empty --directory <parent>` — no `-e`, no credentials. Deliberately
  deferred; note that `--empty` creates the subfolder itself, so the path handling needs adjusting
  with it.
- SQL execution runs **raw SQL** against the Creatio database through cliogate. `UPDATE`/`DELETE`
  are not sandboxed; the UI only warns. The grid is capped at 5,000 rows (exports are uncapped).
- clio itself is now installable/updatable/repairable from the app, but the **.NET SDK is still an
  external prerequisite** — without `dotnet` on PATH DevHub can only report the problem.

## Recommended next priorities

1. **Fix the "Start empty" workspace bug** (see Known limitations) — switch that path to
   `clio createw <name> --empty --directory <parent>` and adjust the destination handling.
2. **Reap clio child processes on app exit** (see Known limitations) — small, and it removes a
   confusing downstream failure in the clio Update button.
3. Run a controlled end-to-end validation matrix using disposable packages/applications and
   non-production environments.
4. Add automated integration tests around cache invalidation and deployment job locking.
5. Add a **macOS build** if there is demand: macOS runner job, non-Windows job cancellation, and
   Apple signing/notarization (needs a paid Apple Developer account).
6. Add configurable clio executable path and log retention/export settings.
7. Add optional scheduled workspace refresh / tray behavior.
8. Clean up the leftover experiment objects on `187559-crm-bundle` when that env is no longer
   needed for testing.
9. Design a proper ALM promotion flow if approvals, environment policies, or release gates are
   required.

Done previously and no longer open: persist job history (`JobStore` in `jobs.rs` + Clear-history
button), and "decide whether to bundle clio" — clio stays an external tool, but DevHub now
installs/updates/repairs it from the header banner (v0.3.0).

## clio behaviours worth knowing (verified live, clio 8.1.x)

These cost real trial-and-error; the built-in `--help` is wrong in places.

**Output parsing**
- clio prepends `[INF]` / `[WAR]` log lines to almost every command, including JSON responses.
  A `[WAR] - clio X is available…` line starts with `[`, which breaks a JSON stream parser —
  find the first `{` before parsing (see `locks.rs`/`sql.rs` handling).
- An unreachable or restarting environment answers with an **HTML error page** (e.g. 401), not
  JSON. Detect `<!doctype` / `<html` and report it as transient rather than dumping the page.

**Exit codes lie**
- clio can exit **0** after Creatio answered a request with **HTTP 500** — a `push-pkg` whose
  install failed server-side prints the error and still ends cleanly, so a job trusting only the
  exit code showed a red deployment as succeeded. `JobState::finish` therefore re-reads the log
  on a zero exit (`diagnostics::failure_despite_zero_exit`) and flips the job to **failed**, adds
  a `[DevHub]` line explaining the override, and attaches the `creatio-server-error` diagnosis.
  The needles are pinned to HTTP forms (`internal server error`, `(500)`, `http 500`, …) because a
  bare `500` shows up in row counts and version strings. Shipped in v0.5.0; `exit_code` still
  records the truthful `0`, and the override is stated in the job log as a `[DevHub]` line.

**Commands that require cliogate** (`clio install-gate -e <env>`)
- `execute-sql-script`, `lock-package` / `unlock-package`, `install-sql-schema`.
- **cliogate SQL works against Creatio Cloud**, not only local IIS installs — verified on
  `187559-crm-bundle` (`https://187559-crm-bundle.creatio.com`). This was an open question for
  the roadmap's environment-diff work: reading settings/features/packages does **not** need
  database access a cloud tenant would refuse.

**`compile-package` can block indefinitely (why roadmap #5's compile half was dropped)**
- `clio compile-package UsrDevHubCollabPackage -e 187559-crm-bundle` ran **22 minutes with zero
  bytes of output** — not even the per-package start message its help promises — and used 2.9
  seconds of CPU, i.e. it sat on a socket. It had to be killed; it never returned.
- A DevHub job wrapping it is **non-cancellable** (the "modifying environment" phase), so the
  user would get a pinned job with no output and no way out. Do not ship a compile action
  without a timeout, a cancellable phase, or at minimum a warning for cloud environments.
- Untested either way: whether the rebuild actually completed server-side after clio gave up.
- Consequently **a populated `errors[]` from `last-compilation-log` has never been observed.**
  A clean build returns `{"errors":[],"buildResult":0,"success":true}`. The element shape is
  clio's own `CreatioCompilationError(line, column, errorNumber, errorText, warning, fileName)`
  from `clio/Common/CompilationLogParser.cs` — read from source, never seen live. Note clio's
  non-`--raw` rendering prints `Error` for warnings too; it never inspects the `warning` flag.

**Entity-schema `.cs` files are regenerated on install**
- Writing C# into `Schemas/<Name>/<Name>.cs` of an entity schema and pushing it does **nothing**:
  Creatio regenerates the file from `metadata.json`, and a re-pull shows 0 bytes again. There is
  no way to inject a deliberate compile error this way — it is the wrong schema type.
- Real C# lives in a **Source code** schema. `clio create-schema --schema-name X --package-name Y`
  creates one on a remote environment and `update-schema` sets its body; `get-schema` reads it.
- Corollary: a `push-pkg` that ends on a bare `[ERR] - Error` after
  `Package installation finished` may have installed fine. That trailing-message case is already
  handled by the v0.2.8 work — do not read it as proof the payload was rejected.

**`save-state` is the whole environment in one file**
- `clio save-state <file> -e <env>` writes YAML with four top-level sections — `features`
  (per-audience values), `settings` (code/value), `webservices` (name/url), and `packages`
  (name, hash, maintainer, **and a per-schema name+hash list**). ~1 MB and ~2 minutes for
  dev-834. `show-diff --source A --target B --file F` emits the same shape containing only the
  differences, but took ~13 minutes for one pair.
- This is the data source for the roadmap's environment-diff feature: no CLI-text parsing is
  needed from either command, and the per-schema hashes give duplicate-element detection for
  free. Build it on cached `save-state` snapshots — a 13-minute live `show-diff` cannot back a
  responsive screen.
- `show-diff` between `dev-834` and `Dev-thoughtworks` produced 5,630 lines in three sections:
  `features` (221), `settings` (265) and `packages` (5,143 — 156 packages with hash, maintainer
  and per-schema hashes). No `webservices` section appeared even though `save-state` emits one,
  and there is **no package version and no lock state** in the diff: it keys on hash. A Packages
  comparison tab therefore needs three sources merged — `show-diff`/`save-state` for hashes,
  `list-packages` for versions, `package_lock_states` for locks.

**⚠ Environment state files contain live secrets — never export them unmasked**
- Both `save-state` and `show-diff` write **setting values in plaintext**. The dev-834 ↔
  Dev-thoughtworks diff contains a full `ApryseLicenseKey` among ordinary configuration. This is
  the same hazard as raw `clio env` output, but easier to miss because most of the file is dull.
- The roadmap's "Copy as report" button — markdown "suitable for pasting into Slack/Teams" —
  would leak these into a chat history. **Mask setting values by default** (show `differs`, not
  the values) and require a deliberate reveal per row; never mask only at copy time, because the
  values are also on screen and in whatever file the snapshot cache writes.
- **Do not rely on `SysSettings."ValueTypeName" = 'SecureText'` to identify secrets.** It is a
  true positive when present but badly incomplete — verified on 187559-crm-bundle, only 13
  settings carry it, and these credentials do **not**:

  | Setting | Type |
  |---|---|
  | `FacebookConsumerSecret`, `GoogleConsumerSecret`, `TwitterConsumerSecret` | ShortText |
  | `BingSearchApiKey`, `BpmAuthKey`, `FacebookConsumerKey`, `GoogleConsumerKey`, `TwitterConsumerKey` | ShortText |
  | `ApolloApiKey`, `mandrillApiKey`, `GoogleMapsApiKey`, `EventTrackingApiKey` | MediumText |

- Name matching alone is no better on its own — it over-matches harmless entries such as
  `PortalRecoveryPasswordEmailTemplate` (a Lookup pointing at an email template). Treat
  `SecureText` **plus** a `Code` pattern (`key|secret|password|token|auth|jwk|credential`) as a
  *hint for what to warn about*, not as the security boundary. The boundary is masking
  everything by default.

**`execute-sql-script`** (backs the SQL screen)
- `--View csv|xlsx` **requires** `--DestinationPath`; only the default `table` view prints to
  stdout. CSV output is **semicolon-delimited** with CRLF line endings.

**`create-entity-schema`** (only used by the parked locks work)
- Columns go after **one** `--column` as space-separated specs; repeating the flag is rejected.
- `--title` is required, and **`--parent BaseEntity`** is required — without a parent clio creates
  a stray `UsrId` primary key and DataService inserts then fail on a not-null violation.
- A newly created schema is SELECT-able immediately but **INSERT fails until `restart-web-app`**.
- `delete-schema` does **not** drop the physical table, so a name can't cleanly be reused — pick a
  fresh schema name instead.

**DataService via `clio dataservice`**
- SELECT columns use `expressionType 0` (SchemaColumn); INSERT/DELETE **values** use
  `expressionType 2` (Parameter) — using 0 there fails with `NotSupportedException: SchemaColumn`.
- Writing a DateTime is locale-fragile; store timestamps as Text (e.g. epoch seconds).

**Tool maintenance**
- `dotnet tool update clio -g` fails with `Access to the path … is denied` when any clio process
  is running — including ones DevHub itself spawned. Finish jobs first, or use Repair.
- A damaged install reports `Could not load file or assembly …`; only uninstall + reinstall
  (Repair) fixes it.

## Handoff checklist

Before changing the project:

- Read `README.md` and this file.
- Confirm active GitHub account and repository access.
- Keep the updater private key outside Git.
- Preserve unrelated local/user changes.
- Verify clio command names against the installed clio version.
- Test destructive/deployment changes only against disposable non-production targets.
- Run TypeScript, frontend build, and Rust tests before publishing.
- Update both documentation files when the user-visible workflow changes.
