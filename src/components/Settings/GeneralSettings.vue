<template>
  <div>
    <div class="settings-section">
      <h3>Game Data Directory</h3>

      <div class="mb-4">
        <label for="game-data-path" class="block text-text-secondary mb-2 text-sm">Project Gorgon Data Folder</label>
        <div class="flex gap-2">
          <input
            id="game-data-path"
            v-model="localGameDataPath"
            @blur="handleGameDataPathInput"
            @keyup.enter="handleGameDataPathInput"
            placeholder="Path to Elder Game\Project Gorgon folder..."
            class="input flex-1" />
          <button @click="browseGameDataFolder" class="btn btn-secondary whitespace-nowrap">
            Browse
          </button>
        </div>
        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          Default location: %APPDATA%\..\LocalLow\Elder Game\Project Gorgon\
          <br />
          This folder contains Player.log, ChatLogs, and Reports subfolders.
          Auto-detection works on Windows.
        </p>
      </div>

      <div class="flex flex-wrap gap-2">
        <button @click="autoDetectGamePath" class="btn btn-secondary">
          Auto-Detect Game Path
        </button>
        <button @click="autoDetectPlayerLogPath" class="btn btn-secondary">
          Auto-Detect Player.log Path
        </button>
        <button @click="resetPaths" class="btn btn-secondary">
          Reset Paths
        </button>
      </div>
      <p
        v-if="pathStatus"
        class="mt-2 text-xs leading-relaxed break-all"
        :class="pathStatusError ? 'text-red-400' : 'text-text-muted'">
        {{ pathStatus }}
      </p>

      <div class="mt-4">
        <label class="flex items-center gap-2 cursor-pointer text-text-primary">
          <input
            type="checkbox"
            v-model="autoDetectPathsOnStartup"
            @change="handleAutoDetectPathsToggle"
            class="size-5 cursor-pointer" />
          <span>Auto-detect game &amp; Player.log paths on startup</span>
        </label>
        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          When enabled, the game data and Player.log paths are reset to the platform
          defaults each time the app launches. Enable this if your game install or user
          profile moves between launches; leave it off if you've set custom paths.
        </p>
      </div>

      <div class="mt-4">
        <label class="flex items-center gap-2 cursor-pointer text-text-primary">
          <input
            type="checkbox"
            v-model="autoIngestPlayerPrev"
            @change="handleAutoIngestToggle"
            class="size-5 cursor-pointer" />
          <span>Back up Player-prev.log to the drop-rate database</span>
        </label>
        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          When the game restarts it rotates your last session into Player-prev.log. With this on,
          glogger ingests it into the lifetime kill/loot database — so sessions played without
          glogger running still count toward drop rates. Deduplicated against live data, so nothing
          is double-counted.
        </p>

        <div v-if="autoIngestPlayerPrev" class="mt-4 p-4 bg-surface-dark border border-border-default rounded">
          <label class="block text-text-secondary mb-1">Keep rotated Player.log copies:</label>
          <div class="flex items-center gap-2 mt-2 flex-wrap">
            <input
              type="number"
              v-model.number="archiveRetentionDays"
              @blur="handleArchiveLimitsChange"
              min="1"
              max="365"
              class="input w-25"
              placeholder="Days" />
            <span class="text-text-secondary">days</span>
            <input
              type="number"
              v-model.number="archiveMaxFiles"
              @blur="handleArchiveLimitsChange"
              min="1"
              max="200"
              class="input w-25"
              placeholder="Files" />
            <span class="text-text-secondary">copies</span>
            <input
              type="number"
              v-model.number="archiveMaxMb"
              @blur="handleArchiveLimitsChange"
              min="1"
              max="8192"
              class="input w-25"
              placeholder="MB" />
            <span class="text-text-secondary">MB total</span>
          </div>
          <p class="mt-2 text-text-muted text-xs leading-relaxed">
            Rotated sessions are archived in <code>&lt;app data&gt;/player-logs</code> so the weekly
            Statehelm gift tracker can re-read the game's own gift counts from them, and so the
            game's own notes reach further back than the two logs the client keeps. Whichever limit
            is tightest wins — a session's Player.log is 30–45 MB, so the MB budget usually binds
            first — and the oldest copies are deleted first. Lower any of them to reclaim disk space;
            nothing else on disk is touched.
          </p>
        </div>
      </div>
    </div>

    <div class="settings-section">
      <h3>Startup Behavior</h3>

      <div class="mb-4">
        <label class="flex items-center gap-2 cursor-pointer text-text-primary">
          <input
            type="checkbox"
            v-model="autoTailChat"
            @change="handleAutoTailChatToggle"
            class="size-5 cursor-pointer" />
          <span>Automatically start chat log watching on startup</span>
        </label>
        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          When enabled, the app will automatically begin tailing chat log files when it launches.
        </p>
      </div>

      <div>
        <label class="flex items-center gap-2 cursor-pointer text-text-primary">
          <input
            type="checkbox"
            v-model="autoTailPlayerLog"
            @change="handleAutoTailPlayerLogToggle"
            class="size-5 cursor-pointer" />
          <span>Automatically start Player.log watching on startup</span>
        </label>
        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          When enabled, the app will automatically begin tailing Player.log for skill updates,
          survey events, and other game data when it launches.
        </p>
      </div>
    </div>

    <div class="settings-section">
      <h3>Timestamp Display</h3>

      <div class="mb-4">
        <label class="block text-text-secondary mb-2 text-sm">Display timestamps in</label>
        <div class="flex gap-2">
          <button
            v-for="option in timestampOptions"
            :key="option.value"
            @click="handleTimestampModeChange(option.value)"
            :class="[
              'px-3 py-1.5 rounded text-sm border transition-colors',
              timestampMode === option.value
                ? 'bg-accent/20 border-accent text-accent'
                : 'border-border text-text-secondary hover:border-text-muted'
            ]">
            {{ option.label }}
          </button>
        </div>
        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          <span v-if="timestampMode === 'local'">
            Times are shown in your computer's local timezone.
          </span>
          <span v-else-if="timestampMode === 'server'">
            Times are shown in the game server's timezone
            <template v-if="serverOffsetLabel"> ({{ serverOffsetLabel }})</template>.
            <template v-if="!hasServerOffset">
              <br />Server timezone not yet detected — start chat log tailing to auto-detect from a login line.
            </template>
          </span>
          <span v-else>
            Times are shown in UTC (Coordinated Universal Time).
          </span>
        </p>
      </div>

      <div>
        <label class="block text-text-secondary mb-2 text-sm">Hour format</label>
        <div class="flex gap-2">
          <button
            v-for="option in hourFormatOptions"
            :key="String(option.value)"
            @click="handleHourFormatChange(option.value)"
            :class="[
              'px-3 py-1.5 rounded text-sm border transition-colors',
              use24Hour === option.value
                ? 'bg-accent/20 border-accent text-accent'
                : 'border-border text-text-secondary hover:border-text-muted'
            ]">
            {{ option.label }}
          </button>
        </div>
        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          <span v-if="use24Hour">Times are shown in 24-hour format (e.g. 14:30).</span>
          <span v-else>Times are shown in 12-hour format with AM/PM (e.g. 2:30 PM).</span>
        </p>
      </div>
    </div>

    <div class="settings-section">
      <h3>Crafting</h3>

      <div>
        <label class="flex items-center gap-2 cursor-pointer text-text-primary">
          <input
            type="checkbox"
            v-model="excludeMaxEnchanted"
            @change="handleExcludeMaxEnchantedToggle"
            class="size-5 cursor-pointer" />
          <span>Exclude "Max-Enchanted" recipes from automated selection</span>
        </label>
        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          When enabled, Max-Enchanted recipe variants are excluded from the leveling optimizer,
          work order matching, and intermediate craft resolution. These recipes use extremely rare
          ingredients and are unlikely choices for leveling or work orders. You can still add them
          to projects manually.
        </p>
      </div>
    </div>
    <div class="settings-section">
      <h3>Characters</h3>

      <div v-if="characters.length === 0" class="text-sm text-text-muted">
        No characters yet — add one from the character picker in the header.
      </div>
      <div v-else class="space-y-2">
        <div
          v-for="char in characters"
          :key="`${char.character_name}-${char.server_name}`"
          class="flex items-center gap-2 text-sm">
          <span class="font-medium text-text-primary">{{ char.character_name }}</span>
          <span class="text-text-muted">{{ char.server_name }}</span>
          <span v-if="isActive(char)" class="text-xs text-accent-gold">(active)</span>
          <button
            v-if="!renameTarget || renameTarget.character_name !== char.character_name || renameTarget.server_name !== char.server_name"
            class="btn btn-secondary text-xs px-2 py-1 ml-auto"
            @click="startRename(char)">
            Rename / move
          </button>
        </div>
        <div v-if="renameTarget" class="p-2 space-y-2 border border-border-default rounded">
          <p class="text-sm text-text-primary m-0">
            Rename <strong>{{ renameTarget.character_name }}</strong>
            <span class="text-text-muted">({{ renameTarget.server_name }})</span> — all history moves with it.
          </p>
          <div class="flex gap-2">
            <input
              v-model="renameName"
              placeholder="New character name"
              class="input flex-1 text-sm"
              @keydown.enter="doRename" />
            <input
              v-model="renameServer"
              placeholder="New server"
              list="rename-server-suggestions"
              class="input flex-1 text-sm"
              @keydown.enter="doRename" />
            <datalist id="rename-server-suggestions">
              <option v-for="s in serverList" :key="s" :value="s" />
            </datalist>
            <button
              class="btn btn-primary text-xs px-2 py-1"
              :disabled="!renameName.trim() || !renameServer.trim() || renameBusy"
              @click="doRename">
              {{ renameBusy ? 'Renaming…' : 'Rename' }}
            </button>
            <button class="btn btn-secondary text-xs px-2 py-1" @click="cancelRename">
              Cancel
            </button>
          </div>
          <p v-if="renameError" class="error-box m-0">{{ renameError }}</p>
        </div>
        <p v-if="renameDone" class="success-box m-0">{{ renameDone }}</p>
      </div>
      <p class="mt-2 text-text-muted text-xs leading-relaxed">
        Renaming (or server-moving) a character carries all of its history — reports, skills, favor, survey sessions, kills, everything keyed to the character — to the new name. Deletion stays in the header character picker.
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { useSettingsStore } from "../../stores/settingsStore";
import { useGameStateStore } from "../../stores/gameStateStore";
import { useCharacterStore } from "../../stores/characterStore";
import type { UserCharacter } from "../../stores/startupStore";

const settingsStore = useSettingsStore();
const pathStatus = ref("");
const pathStatusError = ref(false);
const localGameDataPath = ref(settingsStore.settings.gameDataPath);
const autoTailChat = ref(settingsStore.settings.autoTailChat);
const autoTailPlayerLog = ref(settingsStore.settings.autoTailPlayerLog);
const excludeMaxEnchanted = ref(settingsStore.settings.excludeMaxEnchantedRecipes);
const timestampMode = ref(settingsStore.settings.timestampDisplayMode);
const use24Hour = ref(settingsStore.settings.use24HourTime);
const autoDetectPathsOnStartup = ref(settingsStore.settings.autoDetectPathsOnStartup);
const autoIngestPlayerPrev = ref(settingsStore.settings.autoIngestPlayerPrev);
const archiveRetentionDays = ref(settingsStore.settings.playerLogArchiveRetentionDays);
const archiveMaxFiles = ref(settingsStore.settings.playerLogArchiveMaxFiles);
const archiveMaxMb = ref(settingsStore.settings.playerLogArchiveMaxMb);

const gameStateStore = useGameStateStore();
const characterStore = useCharacterStore();
const characters = ref<UserCharacter[]>([]);
const serverList = ref<string[]>([]);
const renameTarget = ref<UserCharacter | null>(null);
const renameName = ref("");
const renameServer = ref("");
const renameBusy = ref(false);
const renameError = ref("");
const renameDone = ref("");

const timestampOptions = [
  { value: 'local' as const, label: 'Local Time' },
  { value: 'server' as const, label: 'Server Time' },
  { value: 'utc' as const, label: 'UTC' },
];

const hourFormatOptions = [
  { value: true, label: '24-hour' },
  { value: false, label: '12-hour' },
];

const hasServerOffset = computed(() => {
  return settingsStore.settings.manualTimezoneOverride != null
    || settingsStore.settings.timezoneOffsetSeconds != null;
});

const serverOffsetLabel = computed(() => {
  const offset = settingsStore.settings.manualTimezoneOverride
    ?? settingsStore.settings.timezoneOffsetSeconds;
  if (offset == null) return '';
  const sign = offset >= 0 ? '+' : '-';
  const abs = Math.abs(offset);
  const h = Math.floor(abs / 3600);
  const m = Math.floor((abs % 3600) / 60);
  return `UTC${sign}${h}${m > 0 ? ':' + String(m).padStart(2, '0') : ''}`;
});

watch(
  () => settingsStore.settings.gameDataPath,
  (newPath) => {
    localGameDataPath.value = newPath;
  }
);

watch(
  () => settingsStore.settings.autoTailChat,
  (val) => { autoTailChat.value = val; }
);

watch(
  () => settingsStore.settings.autoTailPlayerLog,
  (val) => { autoTailPlayerLog.value = val; }
);

watch(
  () => settingsStore.settings.excludeMaxEnchantedRecipes,
  (val) => { excludeMaxEnchanted.value = val; }
);

watch(
  () => settingsStore.settings.timestampDisplayMode,
  (val) => { timestampMode.value = val; }
);

watch(
  () => settingsStore.settings.use24HourTime,
  (val) => { use24Hour.value = val; }
);

watch(
  () => settingsStore.settings.autoDetectPathsOnStartup,
  (val) => { autoDetectPathsOnStartup.value = val; }
);

watch(
  () => settingsStore.settings.autoIngestPlayerPrev,
  (val) => { autoIngestPlayerPrev.value = val; }
);

watch(
  () => settingsStore.settings.playerLogArchiveRetentionDays,
  (val) => { archiveRetentionDays.value = val; }
);

watch(
  () => settingsStore.settings.playerLogArchiveMaxFiles,
  (val) => { archiveMaxFiles.value = val; }
);

watch(
  () => settingsStore.settings.playerLogArchiveMaxMb,
  (val) => { archiveMaxMb.value = val; }
);

async function browseGameDataFolder() {
  const selected = await open({
    directory: true,
    multiple: false,
  });
  if (selected) {
    localGameDataPath.value = selected;
    settingsStore.updateGameDataPath(selected);
  }
}

function handleGameDataPathInput() {
  settingsStore.updateGameDataPath(localGameDataPath.value);
}

// Platform-aware default game data folder (Windows/macOS resolved in the
// backend; empty on unsupported OSes like Linux).
async function resolveDefaultGamePath(): Promise<string> {
  return await invoke<string>("get_default_game_data_path_command");
}

// Resolve a concrete Player.log path. The backend returns an explicit path on
// macOS (logs live separately from game data) and empty on Windows, where
// Player.log sits inside the game data folder — so fall back to that.
async function resolveDefaultPlayerLogPath(gameDataPath: string): Promise<string> {
  const direct = await invoke<string>("get_default_player_log_path_command");
  if (direct) return direct;
  const base = gameDataPath || (await resolveDefaultGamePath());
  return base ? base.replace(/[\\/]+$/, "") + "/Player.log" : "";
}

async function autoDetectGamePath() {
  const path = await resolveDefaultGamePath();
  if (!path) {
    pathStatusError.value = true;
    pathStatus.value =
      "Couldn't auto-detect the game data folder on this operating system. Please set it manually with Browse.";
    return;
  }
  localGameDataPath.value = path;
  await settingsStore.updateGameDataPath(path);
  pathStatusError.value = false;
  pathStatus.value = `Game data folder set to: ${path}`;
}

async function autoDetectPlayerLogPath() {
  const path = await resolveDefaultPlayerLogPath(settingsStore.settings.gameDataPath);
  if (!path) {
    pathStatusError.value = true;
    pathStatus.value =
      "Couldn't auto-detect Player.log. Set the game data folder first, or pick the file manually.";
    return;
  }
  await settingsStore.updateLogFilePath(path);
  pathStatusError.value = false;
  pathStatus.value = `Player.log path set to: ${path}`;
}

// Restore both paths to the platform defaults reported by the backend.
async function resetPaths() {
  const gamePath = await resolveDefaultGamePath();
  // Backend default: explicit on macOS, empty on Windows (derived from game
  // data folder at read time), empty on unsupported OSes.
  const logPath = await invoke<string>("get_default_player_log_path_command");
  localGameDataPath.value = gamePath;
  await settingsStore.updateGameDataPath(gamePath);
  await settingsStore.updateLogFilePath(logPath);
  pathStatusError.value = !gamePath;
  pathStatus.value = gamePath
    ? "Paths reset to the platform defaults."
    : "Paths reset, but this operating system has no known default — set them manually.";
}

function handleAutoDetectPathsToggle() {
  settingsStore.updateSettings({ autoDetectPathsOnStartup: autoDetectPathsOnStartup.value });
}

function handleAutoIngestToggle() {
  settingsStore.updateSettings({ autoIngestPlayerPrev: autoIngestPlayerPrev.value });
}

/** Save all three archive limits together, clamped to the same minimums the
 *  backend enforces (Rust clamps to >= 1 as well). */
function handleArchiveLimitsChange() {
  const atLeast = (value: number, min: number, max: number) =>
    Math.min(max, Math.max(min, Math.floor(value || min)));
  archiveRetentionDays.value = atLeast(archiveRetentionDays.value, 1, 365);
  archiveMaxFiles.value = atLeast(archiveMaxFiles.value, 1, 200);
  archiveMaxMb.value = atLeast(archiveMaxMb.value, 1, 8192);
  settingsStore.updateSettings({
    playerLogArchiveRetentionDays: archiveRetentionDays.value,
    playerLogArchiveMaxFiles: archiveMaxFiles.value,
    playerLogArchiveMaxMb: archiveMaxMb.value,
  });
}

function handleAutoTailChatToggle() {
  settingsStore.updateSettings({ autoTailChat: autoTailChat.value });
}

function handleAutoTailPlayerLogToggle() {
  settingsStore.updateSettings({ autoTailPlayerLog: autoTailPlayerLog.value });
}

function handleExcludeMaxEnchantedToggle() {
  settingsStore.updateSettings({ excludeMaxEnchantedRecipes: excludeMaxEnchanted.value });
}

function handleTimestampModeChange(mode: 'local' | 'server' | 'utc') {
  timestampMode.value = mode;
  settingsStore.updateSettings({ timestampDisplayMode: mode });
}

function handleHourFormatChange(value: boolean) {
  use24Hour.value = value;
  settingsStore.updateSettings({ use24HourTime: value });
}

function isActive(char: UserCharacter): boolean {
  return char.character_name === settingsStore.settings.activeCharacterName
    && char.server_name === settingsStore.settings.activeServerName;
}

async function loadCharacters() {
  try {
    characters.value = await invoke<UserCharacter[]>("get_user_characters");
    serverList.value = await invoke<string[]>("get_server_list");
  } catch (e) {
    console.error("Failed to load user characters:", e);
  }
}

function startRename(char: UserCharacter) {
  renameTarget.value = char;
  renameName.value = char.character_name;
  renameServer.value = char.server_name;
  renameError.value = "";
  renameDone.value = "";
}

function cancelRename() {
  renameTarget.value = null;
  renameName.value = "";
  renameServer.value = "";
  renameError.value = "";
}

async function doRename() {
  const target = renameTarget.value;
  const newName = renameName.value.trim();
  const newServer = renameServer.value.trim();
  if (!target || !newName || !newServer || renameBusy.value) return;
  renameBusy.value = true;
  renameError.value = "";
  renameDone.value = "";
  try {
    await invoke("rename_character", {
      characterName: target.character_name,
      serverName: target.server_name,
      newCharacterName: newName,
      newServerName: newServer,
    });
    // Same cache invalidation as delete (rows moved identity, not gone):
    // clear cross-character caches + bump generations so in-flight loads
    // drop stale results. Active identity followed the rename backend-side.
    if (isActive(target)) {
      settingsStore.settings.activeCharacterName = newName;
      settingsStore.settings.activeServerName = newServer;
    }
    characterStore.allCharacterItems = [];
    characterStore.allCharacterItemsLoaded = false;
    characterStore.allCharacterItemsGeneration++;
    characterStore.allCharacterWorkOrders = [];
    characterStore.allCharacterWorkOrdersLoaded = false;
    characterStore.allCharacterWorkOrdersGeneration++;
    gameStateStore.resetSessionSkills();
    gameStateStore.clearLiveInventory();
    await gameStateStore.loadAll();
    characterStore.initForActiveCharacter();
    renameDone.value = `Renamed ${target.character_name} (${target.server_name}) to ${newName} (${newServer}) — all history moved.`;
    renameTarget.value = null;
    await loadCharacters();
  } catch (e) {
    renameError.value = e instanceof Error ? e.message : String(e);
  } finally {
    renameBusy.value = false;
  }
}

onMounted(loadCharacters);
</script>
