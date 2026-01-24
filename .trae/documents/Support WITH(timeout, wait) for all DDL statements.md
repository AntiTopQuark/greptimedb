## Current State (from code)
- SQL parsing: only `ALTER TABLE ... WITH (...)` attaches a trailing `WITH` map; other DDLs don’t parse trailing `WITH` at all ([alter_parser.rs](file:///home/antitopquark/project/greptimedb/src/sql/src/parsers/alter_parser.rs#L174-L189)).
- `CREATE TABLE/EXTERNAL TABLE` and `CREATE DATABASE` already have a `WITH (...)`, but it is treated as table/db options; `validate_table_option` currently (incorrectly for this feature) accepts DDL keys like `timeout`/`wait` too ([requests.rs](file:///home/antitopquark/project/greptimedb/src/table/src/requests.rs#L105-L124)), so those keys can leak into persisted options.
- Operator: DDL submit options are parsed by `parse_ddl_options`, but they’re only applied to repartition; every other `SubmitDdlTaskRequest` uses defaults (`wait=true`, `timeout=60s`) ([ddl.rs](file:///home/antitopquark/project/greptimedb/src/operator/src/statement/ddl.rs#L128-L158), [ddl.rs](file:///home/antitopquark/project/greptimedb/src/operator/src/statement/ddl.rs#L1459-L1473)).
- Meta: `SubmitDdlTaskRequest.wait/timeout` are effectively ignored for all non-repartition DDLs because `DdlManager` always calls `execute_procedure_and_wait` with no timeout ([ddl_manager.rs](file:///home/antitopquark/project/greptimedb/src/common/meta/src/ddl_manager.rs#L552-L570)).
- Repartition semantics: timeout is procedure-level and can abort the procedure with `ExceededDeadline` ([repartition group timeout](file:///home/antitopquark/project/greptimedb/src/meta-srv/src/procedure/repartition/group.rs#L511-L518)).

## Semantics to Implement
- `WITH (wait=false)`: submit DDL asynchronously and return the procedure id immediately for *all* supported DDLs.
- `WITH (timeout='...')`:
  - For `wait=true`: bound how long the server waits for procedure completion. If it times out, return a timeout error (procedure keeps running; caller can resubmit with `wait=false` to get id immediately).
  - For repartition: keep existing behavior (timeout is also passed into the procedure and may abort it). This preserves current semantics and avoids silently changing repartition’s meaning.

## SQL Layer Changes
- **Add DDL options to AST**
  - Introduce a `ddl_options: OptionMap` (or `Option<OptionMap>`) field in statement structs that represent DDLs: drop/create database/table/view/flow, truncate table, comment on, create/drop trigger (feature gated), and ensure `AlterTable`’s trailing `WITH` is clearly treated as DDL options (rename current `options` to `ddl_options`).
- **Add a parser helper for trailing DDL `WITH`**
  - Create `parse_ddl_with_options_if_present()` in [utils.rs](file:///home/antitopquark/project/greptimedb/src/sql/src/parsers/utils.rs) that parses `WITH (...)` only if present and validates keys are exactly `{wait, timeout}`.
  - Apply it uniformly in all DDL parsers that currently have no `WITH` support (DROP TABLE/DATABASE/VIEW/FLOW, TRUNCATE TABLE, COMMENT ON, CREATE VIEW/FLOW, etc.).
- **Split DDL options from table/db options where `WITH` already exists**
  - For `CREATE TABLE/EXTERNAL TABLE` and `CREATE DATABASE`, keep a single `WITH (...)` syntax but split into `(table_or_db_options, ddl_options)` by key membership.
  - Ensure `wait/timeout` are not persisted into table/db options (i.e., removed before building `CreateTableExpr`/database task).
- **Validation clean-up**
  - Stop using `table::requests::validate_table_option` for DDL submit options.
  - Either (a) introduce a dedicated DDL option validator in SQL layer, or (b) refactor `table::requests` to provide `validate_ddl_option` separately so table validation no longer “accepts” DDL keys.
- **Parser tests**
  - Add positive + negative cases per statement:
    - Accept: `WITH (wait=false)`, `WITH (timeout='5m')`, combined.
    - Reject: unknown keys, invalid types (e.g. `wait='abc'`, `timeout=123`).
    - For create table/db: verify `ddl_options` and `table/db options` are separated.

## Operator Layer Changes
- **Plumb ddl_options into every DDL submission path**
  - In [ddl.rs](file:///home/antitopquark/project/greptimedb/src/operator/src/statement/ddl.rs), for each statement handler that builds a `SubmitDdlTaskRequest`, parse `ddl_options` via the existing `parse_ddl_options` and set `request.wait` and `request.timeout`.
  - For CREATE TABLE/DB: ensure the options passed into `CreateTableExpr`/database tasks have DDL keys removed.
  - Extend comment handling similarly in [comment.rs](file:///home/antitopquark/project/greptimedb/src/operator/src/statement/comment.rs).
- **Uniform output behavior**
  - When `wait=false`: return `procedure id` output for all DDLs (same pattern as repartition).
  - When `wait=true`: keep returning existing “affected rows 0” (or existing output), preserving current UX.

## Meta Server Layer Changes
- **Honor wait/timeout for all DDLs**
  - Refactor `DdlManager` so each DDL submit path can:
    - submit a `ProcedureWithId`,
    - if `wait=false` return `(id, None)` immediately,
    - if `wait=true` wait with `tokio::time::timeout(request.timeout, watcher::wait(..))`.
  - Keep repartition’s special-case where timeout is also passed into procedure construction.
- **Timeout error mapping**
  - On wait-timeout, return a stable meta error mapped to a timeout/cancel status (and include the procedure id in the error message for debuggability without changing protobuf).

## Tests & Docs
- Unit tests: parsing + option splitting; `parse_ddl_options` edge cases.
- Integration tests under `tests/cases`:
  - `WAIT=false` for representative DDLs returns a procedure id.
  - `WAIT=true` + very small timeout returns timeout error while procedure eventually completes (where feasible), or at least does not hang.
- Docs/examples: show `WITH (timeout='...', wait=...)` on several DDLs.

## Implementation Order (to reduce churn)
1. Add SQL AST `ddl_options` fields and trailing `WITH` parser helper + tests.
2. Plumb options through operator to `SubmitDdlTaskRequest` for all DDLs.
3. Make metasrv `DdlManager` honor `wait/timeout` generically.
4. Add/adjust integration tests and docs.

If you confirm this plan, I’ll implement it end-to-end with compilation/tests.