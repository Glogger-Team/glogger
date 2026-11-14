<template>
  <div>
    <div class="settings-section">
      <h3>Appearance</h3>

      <div class="mb-4">
        <label for="ui-font-family" class="block text-text-secondary mb-2 text-sm">Interface Font</label>
        <select
          id="ui-font-family"
          v-model="uiFontFamily"
          @change="handleFontChange"
          class="input">
          <option
            v-for="option in fontOptions"
            :key="option.value"
            :value="option.value">
            {{ option.label }}
          </option>
        </select>
        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          Changes the font used throughout the app. The default monospace font keeps
          numbers and tables aligned; other fonts may be easier to read but can affect
          column alignment.
        </p>
      </div>

      <div class="mb-4">
        <label for="ui-font-size" class="block text-text-secondary mb-2 text-sm">
          Interface Size — {{ uiFontSize }}px
        </label>
        <div class="flex items-center gap-3">
          <input
            id="ui-font-size"
            type="range"
            :min="MIN_FONT_SIZE"
            :max="MAX_FONT_SIZE"
            step="1"
            v-model.number="uiFontSize"
            @change="handleFontSizeChange"
            class="flex-1 cursor-pointer" />
          <button @click="resetFontSize" class="btn btn-secondary whitespace-nowrap">
            Reset
          </button>
        </div>
        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          Scales the entire interface — text and spacing — uniformly (default
          {{ DEFAULT_FONT_SIZE }}px). Larger sizes improve readability; very large sizes
          may cause some layouts to wrap or scroll.
        </p>
      </div>

      <div class="mb-4">
        <label for="ui-scale" class="block text-text-secondary mb-2 text-sm">
          Application Scale
        </label>
        <div class="flex items-center gap-3">
          <input
            id="ui-scale"
            type="range"
            :min="MIN_UI_SCALE"
            :max="MAX_UI_SCALE"
            step="5"
            v-model.number="uiScale"
            @input="handleUiScaleInput"
            @change="handleUiScaleChange"
            class="flex-1 cursor-pointer" />
          <div class="flex items-center gap-1 shrink-0">
            <input
              type="number"
              :min="MIN_UI_SCALE"
              :max="MAX_UI_SCALE"
              step="1"
              v-model.number="uiScale"
              @change="handleUiScaleCommit"
              @keyup.enter="handleUiScaleCommit"
              class="input w-16 text-right"
              aria-label="Application scale percent" />
            <span class="text-text-muted text-sm">%</span>
          </div>
          <button @click="resetUiScale" class="btn btn-secondary whitespace-nowrap">
            Reset
          </button>
        </div>
        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          Zooms the whole application (default {{ DEFAULT_UI_SCALE }}%). Lower values shrink
          everything at once — useful on 4K / high-DPI displays where the interface renders
          too large. Applies instantly as you drag.
        </p>
      </div>

      <div class="mb-2">
        <label class="block text-text-secondary mb-2 text-sm">Preview</label>
        <div
          class="border border-border-default rounded p-3 bg-surface-inset"
          :style="{ fontFamily: uiFontFamily, fontSize: uiFontSize + 'px' }">
          <p class="text-text-primary">The quick brown fox jumps over the lazy dog.</p>
          <p class="text-text-secondary">0123456789 — ()[]{}#&amp;@ +1,234,567 gold</p>
        </div>
      </div>
    </div>

    <div class="settings-section">
      <h3>Dashboard</h3>

      <div class="mb-2">
        <label for="widget-opacity" class="block text-text-secondary mb-2 text-sm">
          Widget background opacity — {{ dashboardWidgetOpacity }}%
        </label>
        <div class="flex items-center gap-3">
          <input
            id="widget-opacity"
            type="range"
            :min="MIN_WIDGET_OPACITY"
            :max="MAX_WIDGET_OPACITY"
            step="1"
            v-model.number="dashboardWidgetOpacity"
            @change="handleWidgetOpacityChange"
            class="flex-1 cursor-pointer" />
          <button @click="resetWidgetOpacity" class="btn btn-secondary whitespace-nowrap">
            Reset
          </button>
        </div>
        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          Controls how opaque the blue background of Dashboard widgets is (default
          {{ DEFAULT_WIDGET_OPACITY }}%). Lower values let the page background show
          through for a more subtle look; 0% makes the widget background fully
          transparent.
        </p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from "vue";
import {
  useSettingsStore,
  applyUiScale,
  FONT_FAMILY_OPTIONS,
  DEFAULT_FONT_SIZE,
  MIN_FONT_SIZE,
  MAX_FONT_SIZE,
  DEFAULT_WIDGET_OPACITY,
  MIN_WIDGET_OPACITY,
  MAX_WIDGET_OPACITY,
  DEFAULT_UI_SCALE,
  MIN_UI_SCALE,
  MAX_UI_SCALE,
} from "../../stores/settingsStore";

const settingsStore = useSettingsStore();
const fontOptions = FONT_FAMILY_OPTIONS;

const uiFontFamily = ref(settingsStore.settings.uiFontFamily);
const uiFontSize = ref(settingsStore.settings.uiFontSize);
const dashboardWidgetOpacity = ref(settingsStore.settings.dashboardWidgetOpacity);
const uiScale = ref(settingsStore.settings.uiScale);

watch(
  () => settingsStore.settings.uiFontFamily,
  (val) => { uiFontFamily.value = val; }
);

watch(
  () => settingsStore.settings.uiFontSize,
  (val) => { uiFontSize.value = val; }
);

watch(
  () => settingsStore.settings.dashboardWidgetOpacity,
  (val) => { dashboardWidgetOpacity.value = val; }
);

watch(
  () => settingsStore.settings.uiScale,
  (val) => { uiScale.value = val; }
);

function handleFontChange() {
  settingsStore.updateSettings({ uiFontFamily: uiFontFamily.value });
}

function handleFontSizeChange() {
  settingsStore.updateSettings({ uiFontSize: uiFontSize.value });
}

function resetFontSize() {
  uiFontSize.value = DEFAULT_FONT_SIZE;
  settingsStore.updateSettings({ uiFontSize: DEFAULT_FONT_SIZE });
}

function handleWidgetOpacityChange() {
  settingsStore.updateSettings({ dashboardWidgetOpacity: dashboardWidgetOpacity.value });
}

function resetWidgetOpacity() {
  dashboardWidgetOpacity.value = DEFAULT_WIDGET_OPACITY;
  settingsStore.updateSettings({ dashboardWidgetOpacity: DEFAULT_WIDGET_OPACITY });
}

// Live-apply the zoom as the slider drags (visual only), then persist on release
// so we don't write settings to disk on every intermediate tick.
function handleUiScaleInput() {
  applyUiScale(uiScale.value);
}

function handleUiScaleChange() {
  settingsStore.updateSettings({ uiScale: uiScale.value });
}

// Typed-in value from the number box: clamp into range (handles empty/out-of-range
// entries), reflect the clamped value back into the field, apply, and persist.
function handleUiScaleCommit() {
  const clamped = Math.min(
    MAX_UI_SCALE,
    Math.max(MIN_UI_SCALE, Math.round(uiScale.value || DEFAULT_UI_SCALE)),
  );
  uiScale.value = clamped;
  applyUiScale(clamped);
  settingsStore.updateSettings({ uiScale: clamped });
}

function resetUiScale() {
  uiScale.value = DEFAULT_UI_SCALE;
  settingsStore.updateSettings({ uiScale: DEFAULT_UI_SCALE });
}
</script>
