# Feature Handoff: Marketing Content Migration (Campaigns / Bulk Email)

**Status:** implemented locally — automated validation complete, live app verification pending.
Written 2026-07-22 after the entire flow was proven
manually, end-to-end, against real environments (source `187559-crm-bundle` cloud
trial → target `dev-834` local IIS). Everything below is verified behavior, not
guesswork. The working reference implementation (Node scripts) lives at
`C:\Users\Lenovo\creatio-content-migrate\` — port its logic, don't reinvent it.

## What the feature does

The existing Migration screen copies **lookup values**. This feature adds a second
migration type: **marketing content** — Campaigns, Bulk Emails, email templates,
dynamic content, and the campaign *flow diagrams* — copied source→target with
**GUIDs preserved** (FK references and inline HTML references stay intact) and
**idempotently** (re-run inserts only missing rows).

Real-world use case it solves: a trial/demo bundle has standard campaigns and
emails; a fresh dev instance has the schemas and lookups but zero content. Plain
package tooling can't move this because it's row data spread across system and
business tables with cross-references.

## Verified domain knowledge (do not rediscover this the hard way)

### Copy set and order (FK parents first)

| # | Entity | Rows (bundle) | Write path | Notes |
|---|--------|------|-----------|-------|
| 1 | `BfEmailTemplate` | 25 | OData | email designer templates (PageJson/PageHtml, ~25KB each) |
| 2 | `DCTemplate` | 25 | OData | dynamic content headers |
| 3 | `DCReplica` | 26 | OData | DC variants; FK → DCTemplate |
| 4 | `Campaign` | 7 | OData | see CampaignSchemaUId note below |
| 5 | `BulkEmail` | 23 | OData | inline HTML in TemplateBody/TemplateConfig; FK → Campaign |
| 6 | campaign flow schemas (`SysSchema`) | 7 | **SQL only** | OData insert → 403 (protected system object) |
| 7 | `CampaignVersion` | 2 | OData | draft diagrams, plain JSON in `Data` |
| 8 | `CampaignItem` | 92 | OData + SQL | 18/92 rows have empty `Name` → app validation rejects → SQL |
| 9 | `SysLocalizableValue` (flow schemas only) | 135 | OData | element captions; OData insert IS allowed here |

### Transformation rules (all mandatory)

1. **Preserve `Id`** on every insert. Never regenerate GUIDs.
2. **Drop audit columns**: `CreatedById`, `ModifiedById`, `CreatedOn`,
   `ModifiedOn`, `ProcessListeners` (target defaults them).
3. **Strip zero-GUID lookups** (`00000000-0000-0000-0000-000000000000`): OData
   returns empty lookups as zero-GUIDs; sending one back causes a *false* FK
   violation on Postgres. Omit the field instead (→ NULL).
4. **Remap the Supervisor contact**: the source's Supervisor Contact Id differs
   per environment (bundle: `76929f8c-7e15-4c64-bdb0-adc62d383727`; dev-834:
   `410006e1-ca4e-4502-a9ec-e54d922d2c00`). Resolve the target's Supervisor via
   `Contact?$filter=Name eq 'Supervisor'` at runtime and remap ANY field whose
   value equals the source Supervisor Id (Owner, etc.). Do NOT hardcode.
5. **Skip OData media/nav annotations** (`*@odata.*` keys) and object values.

### Campaign flow diagrams (the tricky part)

- A campaign's diagram is a `SysSchema` row: `ManagerName='CampaignSchemaManager'`,
  `Name` like `UsrCampaign5`, package = `Custom`. `Campaign.CampaignSchemaUId`
  points at its `UId`.
- `MetaData` (the diagram, plain JSON 8–19KB) and `Descriptor` (5 bytes, `{}`)
  are OData **media links**: `GET {uri}/0/odata/SysSchema(<rowId>)/MetaData`
  returns the raw bytes losslessly. This is the ONLY reliable read path
  (DataService SelectQuery mangles blobs; direct DB is unavailable on cloud).
- **Writing `SysSchema` via OData is blocked** (403 "insufficient permissions to
  use OData" even as Supervisor — system-object protection). Write via SQL:
  `INSERT INTO "SysSchema" (...)` with `MetaData`/`Descriptor` as
  `decode('<hex>','hex')` bytea literals. See `gen-flow-sql.mjs` for the exact
  column list; `ON CONFLICT ("Id") DO NOTHING` gives idempotency.
- The `Custom` package has the **same Id and UId in every Creatio install**
  (`f4cea815-b5cd-4b3d-ae47-099332b5f9fb` / `a00051f4-cde3-4f3f-b08e-c5ad1a5c735a`)
  — `SysPackageId` copies verbatim.
- After inserting schemas: PATCH each `Campaign.CampaignSchemaUId` (regular
  OData PATCH, allowed), then **clear the env's redis db + restart the app** —
  the schema manager caches aggressively; without restart the designer won't see
  the flows. dev-834 redis is db 6 (from `ConnectionStrings.config`). clio does
  both: `clio clear-redis-db -e <env>` + `clio restart-web-app -e <env>`.
  Warn the user: local IIS cold start takes several minutes.

### Odds and ends

- `SysLocalizableValue` OData filter must use nav-path syntax:
  `$filter=SysSchema/Id eq <guid>`. The flat `SysSchemaId eq <guid>` errors out.
- `CampaignItem` rows with empty `Name` fail OData insert with "Name field must
  be filled in" (app-level validation). The DB column is `NOT NULL DEFAULT ''`,
  so SQL insert with `''` preserves the source faithfully. 18 of 92 rows hit this.
- `SysImage` is deliberately **out of scope**: verified that every image in the
  demo email/DC HTML is an external absolute URL (creatio.com) — zero local
  references. Also Image-type columns 500 on OData select and are lossy via
  DataService. If a future source DOES embed local images, that's a new problem
  — detect it (scan HTML for `/0/rest/|ImageService|SysImage|GetFile`) and warn
  rather than silently migrate broken emails.
- `EmailTemplate` (old transactional templates, multi-MB bodies) is out of scope.
- Auth: Creatio forms login `POST {uri}/ServiceModel/AuthService.svc/Login` with
  `{UserName, UserPassword}` → cookies + `BPMCSRF` cookie; send `BPMCSRF` as a
  header on every subsequent request. Works identically on cloud and local
  (both envs are IsNetCore:false).

## Proposed implementation in DevHub

### Backend (new module `src-tauri/src/content.rs`, registered in lib.rs)

**New architectural piece: an HTTP client.** The app currently shells out to clio
for everything, but content reads MUST hit OData on the source (cloud tenants
have no DB access, and clio has no OData command). Add `reqwest` (blocking,
`cookie_store` feature) to Cargo.toml.

Credentials: read clio's `appsettings.json`
(`%LOCALAPPDATA%\creatio\clio\appsettings.json` → `Environments[name]`:
`Uri`/`Login`/`Password`) the same way env registration already works elsewhere.
**Never log or surface the password** (existing house rule: clio owns creds).

Commands (mirror the refdata.rs command style — `Result<T, String>`, jobs
integration for long ops):

1. `content_analyze(source_env, target_env) -> ContentGapReport`
   Per entity: source count, target count, missing count (compare Id sets).
   Also: campaigns whose `CampaignSchemaUId` has no matching target `SysSchema`
   (broken flows), and an HTML scan flagging local-image references (see above).
   This is the "dry run" — the UI shows it before any write.
2. `content_migrate(source_env, target_env, entities: Vec<String>) -> ContentMigrateReport`
   Executes the OData copy in dependency order with the transformation rules.
   Per-row error capture like `migrate_lookups`. Skip-existing by Id set.
3. `content_migrate_flows(source_env, target_env) -> FlowMigrateReport`
   Reads SysSchema rows + MetaData/Descriptor media links from source, builds
   the INSERT SQL (hex bytea), runs it through the existing
   `clio execute-sql-script` path (reuse `sql.rs` / the refdata pattern —
   remember clio exits 0 on SQL errors; parse output for SQLSTATE lines).
   Then PATCHes `Campaign.CampaignSchemaUId` via OData, then copies
   `SysLocalizableValue` + `CampaignItem` (+ SQL fallback rows) +
   `CampaignVersion`.
4. `content_finalize(target_env)` — clear redis + restart via clio, with a
   confirmation dialog in the UI (destructive: kicks users off the env).
5. `content_verify(source_env, target_env) -> ContentGapReport` — re-run the
   analyze; everything should show missing=0 and flows resolvable.

Rollback: follow the refdata precedent — before inserting, write a rollback
`.sql` (`DELETE FROM "<table>" WHERE "Id" IN (...)` for exactly the inserted
Ids) into the same snapshots dir.

### Frontend (`src/modules/migration/`)

Add a mode switch or second tab on MigrationPage: **"Lookups" | "Marketing
content"**. Content tab flow:

1. Pick source + target env (reuse existing env selectors).
2. **Analyze** button → gap table (entity / source / target / missing / notes
   column carrying warnings: broken flows, local images, empty-Name counts).
3. Checkbox per entity group, **Migrate** (content) → results per entity with
   per-row failures expandable — same presentation as lookup migration results.
4. **Migrate flows** as an explicit second step (it writes SysSchema via SQL —
   label it clearly, e.g. "Flow diagrams (writes system tables via SQL)").
5. **Finalize** button (restart) gated behind a confirm dialog; show the
   "cold start takes minutes" warning.
6. Verify button → green/red gap table.

### Tests

- Unit-test the pure functions: row cleaning (audit-drop, zero-GUID strip,
  remap), SQL generation (hex bytea correctness, quoting/escaping of `'` in
  captions — campaign names contain quotes and unicode dashes), dependency
  ordering, gap diffing. Follow the existing refdata.rs test style (74 tests).
- **Live verification is mandatory before release** (see HANDOFF.md v0.5.x
  lesson): run dev.cmd, do a real analyze+migrate dev-834←bundle (or re-run on
  an env where it's a no-op — idempotency makes this safe), and open a campaign
  in the target designer. `tsc` + unit tests alone are NOT a release gate.

### Acceptance criteria

- [ ] Analyze on `187559-crm-bundle` → `dev-834` reports all entities at
      missing=0 (they're already migrated — the no-op run proves idempotency
      and read-path health).
- [ ] Against a fresh target (or after deleting a test row), migrate restores
      it with the same Id.
- [ ] Flow migration produces byte-identical `MetaData` (compare lengths at
      minimum) and campaigns resolve their schema post-restart.
- [ ] No password ever appears in logs, UI, or error messages.
- [ ] Rollback SQL file written before every write batch.
- [ ] Rust tests green; live run performed and noted in HANDOFF.md.

## Reference implementation (proven, keep as oracle)

`C:\Users\Lenovo\creatio-content-migrate\`:
- `lib.mjs` — auth (AuthService login, BPMCSRF), OData GET/POST/paging,
  DataService SelectQuery
- `migrate.mjs` — content copy: tables, order, cleaning rules, dry-run/live
- `gen-flow-sql.mjs` — SysSchema flow export → `flow-insert.sql` (hex bytea)
- `repoint.mjs` — Campaign.CampaignSchemaUId PATCH
- `mig-items.mjs` — CampaignItem + SysLocalizableValue (incl. OData 403/validation
  fallback logic)
- `gen-items-sql.mjs` — empty-Name CampaignItem SQL fallback
- `verify-flows.mjs` — post-migration verification
- `README.md` — the same knowledge as above, condensed

When behavior differs between the Rust port and these scripts, the scripts are
right — they did the real migration on 2026-07-22.

## Implementation note (2026-07-22)

The Rust OData client, five Tauri commands, rollback generation, flow-schema SQL,
pure-function tests, typed IPC surface, and Migration → Marketing content tab are
implemented. `cargo test --lib` passes 83 tests and the TypeScript/production Vite
builds pass. The required live analyze/no-op migration and post-restart designer
check have not been performed in this implementation session; do not treat this
as release-ready until those acceptance checks are completed.

## FK auto-resolution + per-record selection (2026-07-22, second pass)

The first real GUI run (Dev-thoughtworks → pre-thoughtworks, Campaign only)
failed for 4 of 15 campaigns with Postgres `23503` on
`FK0dIuly2u9hURhe40fYhfTeLzE` — resolved live to **`Campaign.OwnerId` →
`Contact`**. The Supervisor remap (rule 4 above) only covers the Supervisor
contact; any other owner GUID is per-environment and broke the insert. Fixes,
all in `content.rs`:

- **FK auto-resolution.** `fk_rules_for()` reads the target's FK constraints
  (`pg_constraint`, one query for all content tables, via
  `refdata::run_select` — needs cliogate; on failure the run degrades to plain
  inserts). Before each insert every FK GUID is verified against the target
  (batched OData existence checks, `Resolver::prime`). A missing reference is
  **remapped to the target row with the same `Name`** (exactly-one match
  required — verified live: 'Philippa Massyn' remaps, 'Smit Suthar' is absent
  and clears), otherwise the field is **cleared**; both are reported per row as
  `adjustments` in the result (`action: remapped | cleared | auto-included`),
  never silent. References to *content-set* tables (Campaign etc.) are never
  cleared — the row is blocked with a "include that record" failure instead,
  after `close_over_parents` has already auto-included any parent available in
  the source. Self-FKs (`TwkParentCampaignId`) insert parents-first via
  `order_rows_parents_first`.
- **Per-record selection.** New `content_list_records(source, target, entity)`
  → `{id, name, existsInTarget}`; `content_migrate` takes optional
  `selections: {entity: [ids]}`. The UI (ContentMigration.tsx) shows a
  "Choose … records" picker for **Campaign and BulkEmail**; unselected missing
  rows count as `notSelected` in the result. Selecting a BulkEmail whose
  Campaign is neither on the target nor selected auto-includes that Campaign
  (reported as an adjustment on the Campaign entity).

89 Rust tests green (6 new), tsc + vite build clean. **Live-verified 2026-07-22:
the user re-ran the Campaign migration through the GUI (Dev-thoughtworks →
pre-thoughtworks) after the fix and it succeeded** — the 4 previously failing
`23503 OwnerId` campaigns now migrate. The content write path has had its first
successful real run.

## Overwrite existing records + flow FK bypass + loading UI (2026-07-22, third pass)

Three user-requested changes on top of the second pass. **Not yet live-verified —
the native window can't be driven here; gates met so far are 94 Rust tests green
(5 new) + tsc + vite build clean. A `dev.cmd` run is still required.**

- **Overwrite existing records (Campaign / BulkEmail).** `content_migrate` gained
  an `overwrite: Option<Vec<String>>` arg (entity names). `copy_entity` now routes
  each source row through the pure `plan_row(exists, selected, overwrite)` →
  `RowPlan::{Process{update}, Skip, NotSelected}`; a selected row already on the
  target is PATCHed (Id dropped from the body) instead of skipped, counted as a new
  `updated` field on `EntityMigrateResult`. Safety: the all-missing default never
  overwrites (needs an explicit selection); the DELETE rollback already excludes
  existing ids, and a JSON snapshot of every overwritten target row is written to
  `overwrite-backup-<env>-<ts>.json` (`ContentMigrateReport.overwrite_backup_path`)
  before the run. UI: a per-entity "Overwrite records already on the target" Switch
  in the Campaign/BulkEmail picker un-disables on-target rows so they can be chosen.

- **Flow FK bypass.** `content_migrate_flows` gained `bypass_fk: Option<bool>`
  (default true). When on, the SysSchema insert transaction is wrapped with
  `ALTER TABLE "SysSchema" DISABLE TRIGGER ALL; … ENABLE TRIGGER ALL;` so a flow
  whose package/owner FK differs on the target still lands. It's one transaction,
  so any failure rolls the DISABLE back too. Fixes the live pre-thoughtworks
  `23503 SysSchema` failure. UI: a "Bypass foreign key checks" Switch (default on)
  in the Flow diagrams step. Needs table-owner rights on the target DB user; the
  toggle lets a user turn it off if their account can't alter triggers.

- **Diagnostics bug fixed.** `23503` (and `23502`) contain `503`/`502`, so the FK
  error was being misdiagnosed as "environment could not be reached". Added a
  `sql-fk-violation` rule ahead of `creatio-unreachable`, made that rule's HTTP
  needles specific (`(503)`, `http 503`, `503 service unavailable`, …) and guarded
  its `none` with `23503`/`23502`/`foreign key constraint`. Real HTTP 503 still
  reads as unreachable (test added).

- **Loading UI.** New `src/components/ui/spinner.tsx` (`Spinner`, `LoadingOverlay`).
  ContentMigration shows a full-panel `LoadingOverlay` during initial env load and
  every busy action, inline `Spinner`s in the primary buttons, and a spinner on the
  record-picker "Loading records…" line.
