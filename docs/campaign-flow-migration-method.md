# How campaign + designer flow-diagram migration worked

Reference notes for moving Creatio **Campaigns, Bulk Emails, and their designer
flow diagrams** between environments with **original GUIDs preserved**. The
in-app "Marketing content" migration was removed from DevHub on 2026-07-23 (it
fought the platform too hard for non-demo targets — see the trade-offs at the
end). This document preserves the method; the working standalone reference
implementation (Node) still lives at `C:\Users\Lenovo\creatio-content-migrate\`.

> **Use this only to seed a fresh/demo/dev target.** These are operational
> business records densely wired to per-environment data (contacts, packages,
> audiences). Do not push them into a real/pre-prod environment with live
> contacts — recreate campaigns natively there instead.

## 0. Authenticate (both source and target)

`POST {uri}/ServiceModel/AuthService.svc/Login` with `{ "UserName", "UserPassword" }`.
Keep the returned cookies and send the `BPMCSRF` cookie value as a `BPMCSRF`
header on every later request. Identical on cloud and local IIS. Read the URL /
login / password from clio's `appsettings.json` (`Environments[name]`); never log
or surface the password.

## 1. Copy the ordinary rows over OData (parents first)

Order matters (foreign keys point at parents):

1. `BfEmailTemplate` — email designer templates
2. `DCTemplate` — dynamic content headers
3. `DCReplica` — dynamic content variants (FK → DCTemplate)
4. `Campaign`
5. `BulkEmail` — inline HTML in TemplateBody/TemplateConfig (FK → Campaign)

Each row: `GET {uri}/0/odata/{Entity}` (paged with `$top`/`$skip`), then
`POST {uri}/0/odata/{Entity}` on the target.

**Transformation rules (all mandatory):**

- **Preserve `Id`** on every insert — never regenerate GUIDs.
- **Drop audit columns**: `CreatedById`, `ModifiedById`, `CreatedOn`,
  `ModifiedOn`, `ProcessListeners` (the target defaults them).
- **Strip zero-GUID lookups** (`00000000-…-0`): OData returns empty lookups as
  zero-GUIDs; sending one back causes a *false* FK violation. Omit the field → NULL.
- **Remap the Supervisor contact**: its Id differs per environment. Resolve the
  target's via `Contact?$filter=Name eq 'Supervisor'` and remap any field equal to
  the source Supervisor Id (Owner, etc.). Never hardcode.
- **Resolve other foreign keys**: any remaining per-environment reference (e.g.
  `Campaign.OwnerId → Contact`) is remapped to the target row with the **same
  Name** (exactly-one match), else the field is cleared, else — for a required
  content parent — the row is skipped and reported. Read the target's real FK
  columns from `pg_constraint` to know which columns to check.
- **Skip OData nav/media annotations** (`*@odata.*`) and object/array values.

## 2. Move the designer flow diagram (the hard part)

A campaign's diagram is a **`SysSchema` row**: `ManagerName='CampaignSchemaManager'`,
`Name` like `UsrCampaign5`, in the `Custom` package. `Campaign.CampaignSchemaUId`
points at that row's `UId`.

- **Read** `MetaData` (the diagram JSON, 8–19 KB) and `Descriptor` (5 bytes, `{}`)
  as OData **media links**: `GET {uri}/0/odata/SysSchema(<rowId>)/MetaData` returns
  the raw bytes losslessly. This is the only reliable read path (DataService
  mangles blobs; cloud has no DB access).
- **Writing `SysSchema` over OData is blocked** (403, system-object protection),
  so insert via **direct SQL** through cliogate:
  `INSERT INTO "SysSchema" (…) VALUES (…, decode('<hex>','hex'), …)` with
  `MetaData`/`Descriptor` as hex `bytea` literals, `ON CONFLICT ("Id") DO NOTHING`.
- **Remap `SysPackageId`** to the target's package of the **same Name** — the
  package Id differs per environment and is the real cause of the `23503`
  SysSchema FK failure. (Do **not** try to disable the FK's constraint trigger:
  `ALTER TABLE … DISABLE TRIGGER ALL` needs superuser and is refused with `42501`
  for the ordinary Creatio DB account.)
- **Respect the `(UId, SysPackageId)` unique key** (`IUSysSchemaUIdSysPackageId`).
  A flow already on the target under the same `UId`+package but a *different* Id
  raises `23505`. Treat a match on **either** `Id` **or** `(UId, package)` as
  "already present" and skip the insert — the campaign still links by `UId`.

## 3. Repoint the campaigns and finish the dependent rows

- **Repoint**: `PATCH Campaign(<id>)` with `{ "CampaignSchemaUId": "<uid>" }`
  (regular OData PATCH is allowed). Works whether the schema was just inserted or
  already existed under a different Id — the link is by `UId`.
- **`CampaignVersion`** — draft diagrams, plain JSON in `Data`; copy over OData.
- **`CampaignItem`** (flow elements) — copy over OData; rows with an empty `Name`
  fail OData validation ("Name field must be filled in"), so insert those via SQL
  with `''` (the DB column is `NOT NULL DEFAULT ''`). Guard `CampaignId → Campaign`:
  skip items whose campaign isn't on the target rather than aborting the batch.
- **`SysLocalizableValue`** (element captions, flow schemas only) — copy over
  OData. The filter must use nav-path syntax: `$filter=SysSchema/Id eq <guid>`
  (flat `SysSchemaId eq …` errors out).

## 4. Finalize the target

The schema manager caches aggressively — the designer won't see new flows until
you **clear Redis and restart the app**: `clio clear-redis-db -e <env>` then
`clio restart-web-app -e <env>`. A local IIS cold start takes several minutes and
signs users out.

## Gotchas learned the hard way

- **HTTP client must decompress** — IIS/proxies gzip large responses; a client
  without gzip fails on the big ones (e.g. `CampaignVersion.Data`) with "error
  decoding response body" while small ones pass. Enable gzip/deflate.
- **Only pull what you need** — analysis needs ids, not the big blob columns.
- **clio exits 0 on rejected SQL** — parse the output for the SQLSTATE line; never
  trust the exit code alone.
- **`SysImage` / local images** are out of scope — the demo content used absolute
  external URLs. Scan HTML for `/0/rest/|ImageService|SysImage|GetFile` and warn
  rather than silently migrate broken emails.

## Why this was removed from DevHub

Every failure above is the same lesson: campaigns/bulk emails are **transactional
data**, not configuration, and the supported way to promote configuration is
**packages (clio / T.I.D.E.)**, not row-copying business records. Getting rows in
often required **bypassing the app's own validation with SQL**, which can leave
records the application considers invalid. It's a fine tool for seeding a demo/dev
instance, but for a real target the right shape is: promote config via packages,
copy reusable **templates + dynamic content**, and **rebuild campaigns natively**
against the target's own contacts and audiences.
