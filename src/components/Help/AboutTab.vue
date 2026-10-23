<template>
  <div class="max-w-lg flex flex-col gap-5">
    <!-- Hero -->
    <div class="card flex flex-col items-center text-center py-8 px-6">
      <img
        src="/glogger.png"
        alt="glogger"
        class="size-20 rounded-2xl mb-4 ring-2 ring-accent-gold/30 shadow-lg" />
      <h2 class="m-0 text-accent-gold text-2xl tracking-wide font-semibold">{{ appName }}</h2>
      <p class="m-0 mt-2 text-text-secondary text-sm">
        By <span class="text-text-primary font-medium">Zenith</span> of Dreva
      </p>
      <p class="m-0 mt-0.5 text-text-dim text-xs">
        (Also known as Daniel Auchenpaugh)
      </p>
      <p class="m-0 mt-0.5 text-text-dim text-xs">
        Maintained by the
        <a href="https://github.com/Glogger-Team">Glogger Team</a> &middot; originally by Daniel Auchenpaugh, built on the
        <a href="https://github.com/crisp-oddio/glogger-oddio">oddio fork</a>
      </p>

      <span class="inline-block mt-3 px-3 py-1 rounded-full bg-surface-dark border border-border-default text-text-muted text-xs font-mono">
        v{{ appVersion }}
      </span>

      <p class="m-0 mt-4 text-text-secondary text-sm leading-relaxed max-w-sm">
        Built to help alievate some spreadsheet tracking, make some data more
        accessable.
      </p>
      <p class="m-0 mt-2 text-text-dim text-xs">
        Some portions copyright 2026 Elder Game, LLC.
      </p>

      <button
        @click="openLink('https://buymeacoffee.com/danielout')"
        class="btn btn-primary mt-5 px-5 py-2">
        Buy Me a Coffee
      </button>
    </div>

    <!-- Special Thanks -->
    <div class="card p-5">
      <h3 class="text-text-primary mt-0 mb-4 text-sm font-bold uppercase tracking-wider flex items-center gap-2">
        <span class="text-accent-gold">&#9830;</span>
        Special Thanks
      </h3>
      <div class="flex flex-col gap-1">
        <div
          v-for="person in thanks"
          :key="person.name"
          class="flex gap-3 px-3 py-2.5 rounded-lg hover:bg-surface-base/50 transition-colors">
          <div class="shrink-0 mt-0.5 w-1.5 h-1.5 rounded-full bg-accent-gold/60" />
          <div>
            <span class="text-text-primary text-sm font-medium">{{
              person.name
            }}</span>
            <span v-if="person.location" class="text-text-dim text-xs ml-1.5"
              >{{ person.location }}</span
            >
            <p
              class="m-0 mt-0.5 text-text-muted text-xs leading-relaxed"
              v-html="person.reason"
              @click="handleReasonClick"></p>
          </div>
        </div>
      </div>
    </div>

    <!-- Built With -->
    <div class="card p-5">
      <h3 class="text-text-primary mt-0 mb-4 text-sm font-bold uppercase tracking-wider flex items-center gap-2">
        <span class="text-accent-gold">&#9881;</span>
        Built With
      </h3>
      <div class="flex flex-wrap gap-2">
        <span
          v-for="tech in techStack"
          :key="tech"
          class="px-3 py-1.5 rounded-lg bg-surface-dark border border-border-default text-text-secondary text-xs font-medium">
          {{ tech }}
        </span>
      </div>
      <p class="text-sm text-text-muted mt-4 mb-0">
        Project: Gorgon app developer? You might find
        <a
          href="https://github.com/danielout/GorgonLogViewer"
          class="text-accent-gold hover:text-text-primary transition-colors"
          >Gorgon Log Viewer</a
        >
        to be a handy tool!
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from "vue";
import { getName, getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";

const appName = ref("glogger");
const appVersion = ref("");

const thanks = [
  {
    name: "Citan & Co",
    location: null,
    reason:
      "For not only making PG, but providing so much info for 3rd party app developers. Without dev support for things like this, Glogger would never be possible.",
  },
  {
    name: "Deradon",
    location: "Dreva @ Stall CJ-2",
    reason:
      "Absolutely amazing work on the stall tracker feature. Hands down incredible.",
  },
  {
    name: "Kaeus",
    location: "Dreva @ Stall CT-10",
    reason:
      'Great mind to bounce dev ideas off of. Go check out their amazing <a href="https://github.com/kaeus/GorgonSurveyTracker">Gorgon Survey Tracker</a>.',
  },
  {
    name: "Reyetta",
    location: "Dreva @ Stall CI-6",
    reason:
      "Endless suggestions and tips, particularly around crafting and gourmand features.",
  },
  {
    name: "Wogan",
    location: "Dreva @ Stall CP-2",
    reason:
      "The bovine surveyor for their help with gems and gem related activities.",
  },
  {
    name: "Cakedydidooda",
    location: "Dreva @ Stall CS-1",
    reason: "For economy related feature feedback.",
  },
  {
    name: "Fidge",
    location: "Dreva @ Stall CP-8",
    reason: "Non-VIP feedback and bugfinding.",
  },
  {
    name: "DisasterGaymer",
    location: "Dreva @ Stall CU-5",
    reason: "Feedback, ideas, and groceries..",
  },
];

const techStack = ["Tauri v2", "Vue 3", "Rust", "SQLite", "Tailwind CSS"];

onMounted(async () => {
  appName.value = await getName();
  appVersion.value = await getVersion();
});

function openLink(url: string) {
  openUrl(url);
}

function handleReasonClick(e: MouseEvent) {
  const target = e.target as HTMLElement;
  if (target.tagName === "A" && target.getAttribute("href")) {
    e.preventDefault();
    openUrl(target.getAttribute("href")!);
  }
}
</script>

<style scoped>
:deep(p a) {
  color: var(--color-accent-gold);
  text-decoration: underline;
  text-underline-offset: 2px;
}

:deep(p a:hover) {
  color: var(--color-text-primary);
}
</style>
