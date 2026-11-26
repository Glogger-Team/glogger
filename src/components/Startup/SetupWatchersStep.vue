<template>
  <div class="card p-6">
    <h2 class="screen-title mb-2">Startup Behavior</h2>
    <p class="text-text-muted text-sm mb-6">
      Choose what glogger should do automatically when it starts.
      You can change these later in Settings.
    </p>

    <div class="space-y-4 mb-6">
      <label class="flex items-center gap-3 cursor-pointer text-text-primary">
        <input
          type="checkbox"
          v-model="autoTailPlayerLog"
          class="size-5 cursor-pointer" />
        <div>
          <span class="text-sm">Watch Player.log on startup</span>
          <p class="text-text-muted text-xs mt-0.5">
            Track skill gains, survey events, and other game activity.
          </p>
        </div>
      </label>

      <label class="flex items-center gap-3 cursor-pointer text-text-primary">
        <input
          type="checkbox"
          v-model="autoTailChat"
          class="size-5 cursor-pointer" />
        <div>
          <span class="text-sm">Watch Chat logs on startup</span>
          <p class="text-text-muted text-xs mt-0.5">
            Capture chat messages for browsing and search.
          </p>
        </div>
      </label>
    </div>

    <div class="flex justify-between">
      <button @click="back" class="btn btn-secondary">Back</button>
      <button @click="next" class="btn btn-primary">Next</button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { useStartupStore } from "../../stores/startupStore";
import { useSettingsStore } from "../../stores/settingsStore";

const startupStore = useStartupStore();
const settingsStore = useSettingsStore();

const autoTailPlayerLog = ref(true);
const autoTailChat = ref(true);

function back() {
  startupStore.goToPhase("setup-path");
}

async function next() {
  await settingsStore.updateSettings({
    autoTailPlayerLog: autoTailPlayerLog.value,
    autoTailChat: autoTailChat.value,
  });
  startupStore.goToPhase("setup-character");
}
</script>
