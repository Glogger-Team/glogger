<template>
  <div>
    <div class="settings-section">
      <h3>Notifications</h3>

      <p class="mt-2 text-text-muted text-xs leading-relaxed">
        Notifications are live through Chat watch rules. A matching message can
        highlight in chat, pop up a toast, and play a sound. Recent matches also
        appear in the Watchword Alerts dashboard widget.
      </p>
      <button @click="openWatchwords" class="btn btn-secondary mt-3">
        Open Chat Watchwords
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useViewNavigation } from "../../composables/useViewNavigation";

const { navigateToView } = useViewNavigation();

function openWatchwords() {
  navigateToView({ view: "chat", subTab: "watchwords" });
}
</script>
