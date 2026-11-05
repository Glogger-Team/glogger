// Hand mirror of MushroomFarmingInfo in
// src-tauri/src/game_data/mushroom_farming.rs.
// Serde renames nothing: Rust snake_case arrives verbatim, so the fields stay
// snake_case here (unlike the usual camelCase mirrors).
// Source: wiki chart (F:/ProjectGorgon/pg-rag/data/wiki/Mushroom_Farming_ee8545db.txt:92-143).

import type { MoonPhaseName } from "../../composables/useMoonPhase";

export interface MushroomFarmingInfo {
  name: string;
  mycology_req: number | null;
  grow_hours: number;
  adequate_substrate: string | null;
  very_well_substrate: string;
  robust_phases: MoonPhaseName[];
  poor_phases: MoonPhaseName[];
}
