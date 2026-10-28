<template>
  <div class="flex flex-col gap-6">
    <p class="text-sm text-text-muted m-0">
      Getting started with Glogger and Project: Gorgon.
    </p>

    <!-- Initial Setup -->
    <section class="card p-5">
      <h2 class="text-sm font-bold text-text-primary uppercase tracking-wider m-0 mb-4 flex items-center gap-2">
        <span class="text-accent-gold">&#9654;</span>
        Initial Setup
      </h2>
      <div class="flex flex-col gap-2.5">
        <SetupStep number="1" title="Set your Game Data path">
          Go to <button class="text-accent-gold bg-transparent border-none cursor-pointer underline p-0 font-inherit text-inherit" @click="$emit('navigate', 'settings')">Settings &rarr; General</button>
          and set the path to your Project: Gorgon data folder.
          This is the folder containing <code class="text-accent-gold bg-surface-dark px-1.5 py-0.5 rounded-sm text-xs">ChatLogs</code> and <code class="text-accent-gold bg-surface-dark px-1.5 py-0.5 rounded-sm text-xs">Reports</code>. On Windows <code class="text-accent-gold bg-surface-dark px-1.5 py-0.5 rounded-sm text-xs">Player.log</code> lives here too; on macOS it is auto-detected from <code class="text-accent-gold bg-surface-dark px-1.5 py-0.5 rounded-sm text-xs">~/Library/Logs/</code>.
        </SetupStep>
        <SetupStep number="2" title="Enable Chat Logging in-game">
          In Project Gorgon, open Settings &rarr; GUI and enable
          <span class="text-text-primary font-medium">"Log All Chat"</span>. It is all the way at the bottom of this settings menu.
          This lets Glogger read chat messages for item linking, watchwords, and other chat-driven features.
        </SetupStep>
        <SetupStep number="3" title="Import your Character JSON (optional)">
          Go to Settings -> VIP in Project Gorgon and run Export Character as JSON. This populates your skills, recipes, and quest data. Enable Automatically Export on Logout in the same screen to keep exports fresh (it also re-exports on every zone change).
        </SetupStep>
        <SetupStep number="4" title="Import your VIP Inventory JSON (optional, requires VIP)">
          Go to Settings -> VIP in Project Gorgon and run Export Storage as JSON. This gives accurate item quantities and mod data.
        </SetupStep>
      </div>
    </section>

    <!-- Keyboard Shortcuts -->
    <section class="card p-5">
      <h2 class="text-sm font-bold text-text-primary uppercase tracking-wider m-0 mb-3 flex items-center gap-2">
        <span class="text-accent-gold">&#9000;</span>
        Keyboard Shortcuts
      </h2>
      <div class="flex flex-col gap-0.5">
        <ShortcutRow keys="Ctrl+F" description="Quick search overlay" />
        <ShortcutRow keys="Ctrl+K" description="Quick search (alias)" />
        <ShortcutRow keys="Ctrl+D" description="Toggle Data Browser" />
        <ShortcutRow keys="Ctrl+[" description="Previous sub-tab" />
        <ShortcutRow keys="Ctrl+]" description="Next sub-tab" />
      </div>
    </section>

    <!-- In-Game Commands -->
    <section class="card p-5">
      <h2 class="text-sm font-bold text-text-primary uppercase tracking-wider m-0 mb-3 flex items-center gap-2">
        <span class="text-accent-gold">/</span>
        Useful In-Game Commands
      </h2>
      <div class="flex flex-col gap-0.5">
        <ShortcutRow keys="/behaviorreport" description="Display (and let glogger save) some character stats" />
        <ShortcutRow keys="/age" description="Display (and let glogger save) some playtime data" />
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import SetupStep from './SetupStep.vue'
import ShortcutRow from './ShortcutRow.vue'

defineEmits<{
  navigate: [view: string]
}>()
</script>
