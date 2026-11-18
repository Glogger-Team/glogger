# Implementation Checklist

Quick checklist for implementing common features in the glogger data architecture.

## Adding a New Chat Status Event

See [`live-event-streams.md`](live-event-streams.md) for full details.

- [ ] Add `ChatStatusEvent` variant in [`chat_status_parser.rs`](../../src-tauri/src/chat_status_parser.rs)
- [ ] Add `try_xxx` function with string matching logic
- [ ] Chain into `parse_status_message()` via `.or_else()`
- [ ] Add test cases using `status_msg()` helper
- [ ] Handle in coordinator if persistence needed
- [ ] Update `live-event-streams.md`

## Adding a New Database Table

- [ ] Add table schema to [`migrations.rs`](../../src-tauri/src/db/migrations.rs) in `migration_v1_unified_schema`
- [ ] Add indexes for common queries
- [ ] Add unique constraints if needed for deduplication
- [ ] Create query module in `src-tauri/src/db/` (e.g., `my_feature_commands.rs`)
- [ ] Implement insert/update/select functions
- [ ] Add error handling with proper Result types
- [ ] Add Tauri commands for frontend access (if needed)
- [ ] Register commands in [`lib.rs`](../../src-tauri/src/lib.rs)
- [ ] Add TypeScript types for frontend (if needed)
- [ ] Test database operations
- [ ] Update documentation

## Adding a New Coordinator Operation

- [ ] Add `OperationType` variant in [`coordinator.rs`](../../src-tauri/src/coordinator.rs)
- [ ] Implement `start_*` method with operation locking
- [ ] Implement `stop_*` method with cleanup
- [ ] Update `get_status()` to include new operation
- [ ] Add progress tracking (if long-running)
- [ ] Emit status change events
- [ ] Add Tauri commands for start/stop
- [ ] Register commands in [`lib.rs`](../../src-tauri/src/lib.rs)
- [ ] Add frontend store actions
- [ ] Add UI controls (if needed)
- [ ] Test operation lifecycle
- [ ] Test conflict detection with other operations
- [ ] Update documentation

## Adding a New Settings Field

- [ ] Add field to `AppSettings` struct in [`settings.rs`](../../src-tauri/src/settings.rs)
- [ ] Add `#[serde(default)]` if it's optional
- [ ] Update `Default` implementation
- [ ] Add getter method to `SettingsManager` (if needed)
- [ ] Add field to `AppSettings` interface in [`settingsStore.ts`](../../src/stores/settingsStore.ts)
- [ ] Add field to `BackendSettings` interface
- [ ] Update `toBackendSettings()` conversion
- [ ] Update `fromBackendSettings()` conversion
- [ ] Update `getDefaultSettings()`
- [ ] Add update method in store (if needed)
- [ ] Add UI control in Settings component
- [ ] Test saving and loading
- [ ] Update documentation

## Adding a New Frontend Store

- [ ] Create store file in `src/stores/`
- [ ] Define state with `ref()`
- [ ] Add computed properties with `computed()`
- [ ] Add Tauri event listeners with `listen()`
- [ ] Implement actions (async functions)
- [ ] Add error handling
- [ ] Return public API from store
- [ ] Import and use in components
- [ ] Test reactivity
- [ ] Update documentation

## Adding a New Tauri Command

- [ ] Implement function in appropriate Rust module
- [ ] Add `#[tauri::command]` attribute
- [ ] Use `State<'_, T>` for managed state
- [ ] Return `Result<T, String>` for error handling
- [ ] Import command in [`lib.rs`](../../src-tauri/src/lib.rs)
- [ ] Add to `invoke_handler` in [`lib.rs`](../../src-tauri/src/lib.rs)
- [ ] Call from frontend with `invoke()`
- [ ] Add TypeScript types for parameters/return
- [ ] Test error cases
- [ ] Update documentation

## Performance Optimization

- [ ] Profile with `cargo flamegraph` or similar
- [ ] Check database query plans with `EXPLAIN QUERY PLAN`
- [ ] Add indexes for slow queries
- [ ] Batch database operations where possible
- [ ] Use `INSERT OR IGNORE` to skip duplicates
- [ ] Adjust polling intervals if needed
- [ ] Monitor memory usage
- [ ] Test with large datasets
- [ ] Update documentation

## Testing

- [ ] Add unit tests in Rust module
- [ ] Test happy path
- [ ] Test error cases
- [ ] Test edge cases (empty input, null, etc.)
- [ ] Run `cargo test`
- [ ] Test in dev mode (`npm run tauri dev`)
- [ ] Test with real game data
- [ ] Test auto-start behavior
- [ ] Test across app restarts
- [ ] Update test documentation

## Release Preparation

- [ ] All tests passing
- [ ] No compiler warnings
- [ ] Database migrations tested
- [ ] Settings upgrade path tested
- [ ] Performance acceptable
- [ ] Error handling complete
- [ ] Logging/debugging info added
- [ ] Documentation updated
- [ ] User-facing changes noted
- [ ] Breaking changes documented

## Documentation

- [ ] Update architecture docs if structural change
- [ ] Add/update code comments
- [ ] Update TypeScript types
- [ ] Add example usage
- [ ] Document any breaking changes
- [ ] Update migration guide (if needed)
- [ ] Add to changelog

## Quick Reference: File Locations

**Rust Backend:**

- Main entry: `src-tauri/src/lib.rs`
- Coordinator: `src-tauri/src/coordinator.rs`
- Log watchers: `src-tauri/src/log_watchers.rs`
- Settings: `src-tauri/src/settings.rs`
- Database: `src-tauri/src/db/`
  - Schema: `migrations.rs`
  - Queries: `queries.rs`
  - Commands: `*_commands.rs`

**Frontend:**

- Main app: `src/App.vue`
- Stores: `src/stores/`
- Components: `src/components/`
- Types: `src/types/`

**Documentation:**

- Architecture: `docs/architecture/`
- Features: `docs/features/`
- Plans: `docs/plans/`
