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
    id: "statehelm-offpage",
    severity: "bug",
    title: "Statehelm tracker may lag behind when off-page",
    description:
      "The Statehelm gifting tracker updates from the database, but real-time updates rely on the favor activity feed. If you notice stale data, navigating to the Statehelm page will refresh it.",
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
      'Inventory is reconstructed from the shop log event history. If the in-game log was truncated before an item was added (older entries scroll out of range), the baseline is lost and the quantity can briefly go negative until a fresh "added" event resets the tier stack. Open your shop log in-game regularly, or use Import to backfill from an exported book file.',
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
