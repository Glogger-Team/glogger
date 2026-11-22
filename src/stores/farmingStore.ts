import { defineStore, acceptHMRUpdate } from "pinia";

// Shell: all live surfaces moved out (loot stats read straight from the DB
// via get_enemy_kill_stats; kills/deaths/XP live in the Combat view).
export const useFarmingStore = defineStore("farming", () => {
  return {};
});

if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useFarmingStore, import.meta.hot))
}
