<template>
  <div class="flex flex-col gap-6">
    <!-- Header -->
    <p class="text-sm text-text-muted m-0">
      Glogger is currently in
      <span class="text-accent-gold font-medium">beta</span>. Below are known
      issues and limitations we're aware of.
    </p>

    <!-- Known Issues -->
    <section class="card p-5">
      <h2
        class="text-sm font-bold text-text-primary uppercase tracking-wider m-0 mb-4 flex items-center gap-2">
        <span class="text-accent-red">!</span>
        Known Issues
      </h2>
      <div class="flex flex-col gap-2">
        <IssueCard
          v-for="issue in knownIssues"
          :key="issue.id"
          :issue="issue" />
      </div>
    </section>

    <!-- Limitations -->
    <section class="card p-5">
      <h2
        class="text-sm font-bold text-text-primary uppercase tracking-wider m-0 mb-4 flex items-center gap-2">
        <span class="text-accent-warning">~</span>
        Known Limitations
      </h2>
      <div class="flex flex-col gap-2">
        <IssueCard
          v-for="issue in limitations"
          :key="issue.id"
          :issue="issue" />
      </div>
    </section>

    <!-- Tips -->
    <section class="card p-5">
      <h2
        class="text-sm font-bold text-text-primary uppercase tracking-wider m-0 mb-4 flex items-center gap-2">
        <span class="text-accent-gold">&#9733;</span>
        Tips
      </h2>
      <div class="flex flex-col gap-2">
        <div
          v-for="tip in tips"
          :key="tip"
          class="flex gap-3 text-sm text-text-muted px-3 py-2.5 rounded-lg hover:bg-surface-base/50 transition-colors">
          <span class="text-accent-gold/60 shrink-0 mt-0.5">&#8226;</span>
          <span>{{ tip }}</span>
        </div>
      </div>
    </section>

    <!-- Footer -->
    <div class="text-xs text-text-dim border-t border-border-default pt-4">
      Found a bug not listed here? Report it to Zenith on the Project: Gorgon
      discord. Or post an issue
      <a
        href="https://github.com/Glogger-Team/glogger/issues"
        class="text-accent-gold hover:text-text-primary transition-colors"
        >on GitHub</a
      >.
    </div>
  </div>
</template>

<script setup lang="ts">
import IssueCard from "./IssueCard.vue";

interface KnownIssue {
  id: string;
  severity: "bug" | "limitation" | "cosmetic";
  title: string;
  description: string;
}

const knownIssues: KnownIssue[] = [
  {
    id: "item-mod-names",
    severity: "bug",
    title: "Items with mods may not resolve correctly",
    description:
      'Items with TSys mod prefixes/suffixes (e.g., "Amazing Iron Sword") may not always link to their base item in the CDN. We\'ve added prefix/suffix stripping as a fallback, but some edge cases may remain.',
  },
  {
    id: "statehelm-gift-counts",
    severity: "bug",
    title: "Statehelm gift counts aren't shown",
    description:
      "The Statehelm page shows favor tiers but not per-NPC weekly gift counts — the view doesn't pass counts to its NPC cards, and the dashboard summary widget was removed. Favor data itself still refreshes in the background.",
  },
  {
    id: "item-quantities",
    severity: "bug",
    title: "Item quantities may be inaccurate",
    description:
      "The Player.log doesn't always provide exact stack sizes. Glogger infers quantities from chat status messages, but this isn't always reliable. Run Export Storage as JSON in Settings → VIP and import it for accurate data.",
  },
  {
    id: "rez-tracker",
    severity: "bug",
    title: "Resurrection tracker may miss events",
    description:
      "The rez tracker parses [Action Emotes] chat messages. If this channel is disabled in your game settings, or if the game uses a different message format, rez events won't be captured.",
  },
  {
    id: "arena-bets-dark",
    severity: "bug",
    title: "Arena personal bets are no longer recorded",
    description:
      "Personal bets were read from Player.log talk screens, which the current game client no longer writes. Match results still track via chat narration, and historic bets remain visible in the Dashboard Arena widget.",
  },
  {
    id: "poems-dark",
    severity: "bug",
    title: "New poem recitals are no longer recorded",
    description:
      "Poems were captured from Player.log talk screens when another player recited at a poetry podium, which the current game client no longer writes. Historic poems remain, and Scan logs for past poems still finds poems in older logs.",
  },
  {
    id: "survey-auto-start-dark",
    severity: "bug",
    title: "Survey sessions no longer auto-start",
    description:
      "Automatic detection needs Player.log survey events, which the current game client no longer writes. Start sessions manually with Start New Session and record each map with the Record survey use control.",
  },
  {
    id: "chat-logging-required",
    severity: "bug",
    title: "Chat-driven features stop without in-game chat logging",
    description:
      'Item linking, watchwords, rez tracking, and other chat-driven features need "Log All Chat" enabled in-game (Settings → GUI) plus a game data path set. Messages on excluded channels are never stored, so check Settings → Chat Logs before resetting channels to defaults.',
  },
  {
    id: "quest-completed-reexport",
    severity: "bug",
    title: "Completed quests may be empty on older exports",
    description:
      "Completed quests are only recorded by newer game clients. If the Completed tab is empty, re-export the character in-game (Export Character as JSON in Settings → VIP) to populate it.",
  },
  {
    id: "rename-old-exports",
    severity: "bug",
    title: "Renamed characters can reappear from old export files",
    description:
      "Renaming moves all history, but game-written Reports/Character_<old-name>_*.json files still carry the old name. Remove them or re-export after renaming, or the next report scan recreates the old identity.",
  },
];

const limitations: KnownIssue[] = [
  {
    id: "no-item-mods",
    severity: "limitation",
    title: "Item mods/augments not shown in live inventory",
    description:
      "The Player.log doesn't include TSys mod or augment data for items. This information is only available through Export Storage as JSON in Settings → VIP. Report imports do include this data.",
  },
  {
    id: "no-equipment-details",
    severity: "limitation",
    title: "Current equipment lacks item details",
    description:
      "The Player.log only provides appearance keys for equipped items, not full item names or stats. A detailed equipment view would require the VIP JSON export.",
  },
  {
    id: "session-only-feeds",
    severity: "limitation",
    title: "Activity feeds are session-only",
    description:
      "Items incoming/outgoing, favor changes, and council changes are tracked per-session and reset when you restart Glogger. Historical data is preserved in the database but not shown in the live feeds.",
  },
  {
    id: "stall-inventory-truncation",
    severity: "limitation",
    title: "Stall Tracker inventory may show incomplete or negative quantities",
    description:
      'Inventory is reconstructed from the shop log event history. If the in-game log was truncated before an item was added (older entries scroll out of range), the baseline is lost and the quantity can briefly go negative until a fresh "added" event resets the tier stack. Count your stock in Manual stock entry to set a baseline, or use Import to backfill from an exported book file. Rebuild from log removes all manual counts for the current character and cannot restore missing events — use it only when the saved log has enough history to reconstruct stock, or recount affected items afterward.',
  },
];

const tips = [
  "Run Export Storage as JSON in Settings → VIP regularly for the most accurate inventory data.",
  "Run Export Character as JSON in Settings → VIP to populate skills, recipes, and quest data. Enable Automatically Export on Logout to keep them fresh.",
  "Use Ctrl+K (or Cmd+K) to open quick search — you can search items, NPCs, skills, recipes, and more.",
  "Dashboard widgets can be reordered by dragging. Hide ones you don't need in the settings panel on the right.",
  "The Build Planner lets you plan mod configurations without committing in-game.",
];
</script>
