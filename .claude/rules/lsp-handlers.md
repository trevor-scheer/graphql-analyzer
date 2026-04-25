---
description: LSP handler conventions for the sync main-loop architecture
paths:
  - "crates/lsp/**"
---

# LSP Handler Conventions

This crate runs a **synchronous, single-threaded main loop** on `lsp-server` +
`crossbeam-channel`, with read-only Salsa queries dispatched to a `threadpool`. There is no
async runtime in the LSP crate. The pre-migration tower-lsp / `spawn_blocking` constraints no
longer apply.

- All handlers go in `crates/lsp/src/handlers/` (`navigation`, `display`, `editing`,
  `document_sync`, `custom`).
- Register new request handlers in `main_loop::handle_request` via `on_pool` or `on_main`;
  register notification handlers in `handle_notification` via `NotificationDispatcher::on`.
- Advertise new server capabilities in `server::initialize_capabilities` (or the equivalent
  initialize handler).
- Return `None` / empty results rather than errors for graceful degradation.
- Prefer incremental document sync over full sync.

## Pick the right dispatcher

| Helper                       | When                                                                                            | Signature                                                            |
| ---------------------------- | ----------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| `on_pool`                    | Read-only request keyed off a single URI (hover, completion, goto-def, references, ...)         | `fn(GlobalStateSnapshot, R::Params) -> R::Result` — runs on a worker |
| `on_main`                    | Request that needs `&mut GlobalState`, traverses all hosts, or has trivial cost (resolve, ping) | `fn(&mut GlobalState, R::Params) -> R::Result` — runs on main        |
| `NotificationDispatcher::on` | All notifications (`did_open`/`did_change`/`did_save`/`did_close`/`did_change_watched_files`)   | `fn(&mut GlobalState, N::Params)` — runs on main                     |

Notification handlers mutate `GlobalState` and `AnalysisHost` directly; no locks needed,
because the main thread is the sole writer.

## Diagnostics: use the spawn helpers, not the snapshot directly

For non-trivial diagnostics computation (any Salsa query), spawn it onto the worker pool
instead of running it inline on the main thread:

```rust
// Single-URI (e.g. did_change): generation-checked, stale results are dropped.
state.spawn_diagnostics_for_uri(uri, move || {
    snapshot.diagnostics(&file_path).into_iter().map(convert_ide_diagnostic).collect()
});

// Multi-URI (e.g. did_save, project-wide): not generation-checked.
state.spawn_diagnostics_batch(move || project_wide_diagnostics(&snapshot));
```

`spawn_diagnostics_for_uri` bumps a per-URI counter in `diagnostics_seq` and tags the worker's
result with the captured generation. `main_loop::handle_task` drops the result when a newer
keystroke for the same URI has already incremented the counter. This is the sync equivalent of
cancelling an in-flight diagnostics computation.

The exception is `did_open`: the snapshot is fresh, the file has just been added, and the
handler publishes diagnostics inline so the editor sees them immediately on open.

## Cancellation

`$/cancelRequest` is handled in `main_loop::handle_notification` against
`GlobalState::in_flight: HashSet<RequestId>`:

- `handle_request` inserts `req.id` into `in_flight` before dispatching.
- `respond` removes it.
- A late worker response whose id is no longer in `in_flight` is logged and dropped in
  `handle_task` rather than sent to the client.

Don't bypass `respond`. Don't write to the connection sender directly from a worker.

## What used to apply (and no longer does)

The previous tower-lsp architecture required `spawn_blocking` around every Salsa query and
helpers named `with_analysis` / `blocking`. **None of that exists in the current code.** If
you see those names referenced in a doc, agent, or commit message, treat it as historical.
The whole class of async/sync boundary bugs (DashMap shard locks across `.await`, runtime
starvation when a setter ran on the async thread) was structurally eliminated by going sync.
