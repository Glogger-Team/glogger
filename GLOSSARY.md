# glogger

Local-first Project Gorgon log ingester and dashboard (Tauri v2 + Vue 3): watches game logs, parses into SQLite, renders inventory/crafting/survey/economy views.

## Language

**Slice**:
One `mise run lint:*` entry plus the CI job that runs it.
_Avoid_: check, gate, job

**Scope**:
The path set deciding whether a slice runs on a given PR.
_Avoid_: filter, paths, target
