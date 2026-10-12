<template>
  <div class="flex flex-col gap-4 h-full min-h-0">
    <!-- Header -->
    <div class="flex items-center justify-between shrink-0">
      <div>
        <h2 class="text-accent-gold text-xl font-semibold m-0">Market Values</h2>
        <p class="text-text-dim text-xs mt-1 mb-0">
          Player-to-player market values for item tooltips and wealth calculations.
          <span v-if="settingsStore.settings.marketPriceMode === 'universal'" class="text-text-secondary">
            Prices shared across all servers.
          </span>
          <span v-else class="text-text-secondary">
            Prices for {{ settingsStore.settings.activeServerName ?? 'current server' }}.
          </span>
        </p>
      </div>
      <div class="flex items-center gap-2">
        <select
          :value="settingsStore.settings.marketPriceMode"
          class="input text-xs"
          @change="updateMode(($event.target as HTMLSelectElement).value)">
          <option value="universal">Universal prices</option>
          <option value="per_server">Per-server prices</option>
        </select>
        <button class="btn btn-secondary text-xs" @click="doExport">Export</button>
        <button class="btn btn-secondary text-xs" @click="showImport = true">Import</button>
      </div>
    </div>

    <!-- Summary stats -->
    <div v-if="marketStore.values.length > 0" class="grid grid-cols-4 gap-3 shrink-0">
      <div class="bg-surface-elevated border border-border-default rounded px-3 py-2">
        <div class="text-[10px] uppercase tracking-wider text-text-secondary">Items Tracked</div>
        <div class="text-base text-text-primary font-semibold tabular-nums">{{ marketStore.values.length }}</div>
      </div>
      <div class="bg-surface-elevated border border-border-default rounded px-3 py-2">
        <div class="text-[10px] uppercase tracking-wider text-text-secondary">Total Market Value</div>
        <div class="text-base text-accent-gold font-semibold tabular-nums">{{ totalMarketValue.toLocaleString() }}g</div>
      </div>
      <div class="bg-surface-elevated border border-border-default rounded px-3 py-2">
        <div class="text-[10px] uppercase tracking-wider text-text-secondary">Avg Price</div>
        <div class="text-base text-text-primary font-semibold tabular-nums">{{ avgPrice.toLocaleString() }}g</div>
      </div>
      <div class="bg-surface-elevated border border-border-default rounded px-3 py-2">
        <div class="text-[10px] uppercase tracking-wider text-text-secondary">Highest Priced</div>
        <div class="text-base text-text-primary font-semibold tabular-nums truncate" :title="highestPricedItem?.item_name">
          {{ highestPricedItem ? `${highestPricedItem.market_value.toLocaleString()}g` : '--' }}
        </div>
        <div v-if="highestPricedItem" class="text-[10px] text-text-dim truncate">{{ highestPricedItem.item_name }}</div>
      </div>
    </div>

    <!-- Settings accordion -->
    <AccordionSection :default-open="false" class="shrink-0">
      <template #title>Valuation Settings</template>
      <div class="flex items-center gap-3 mt-2">
        <label class="text-text-muted text-xs whitespace-nowrap">Item valuation for calculations:</label>
        <select
          :value="settingsStore.settings.itemValuationMode"
          class="input text-xs flex-1"
          @change="updateValuationMode(($event.target as HTMLSelectElement).value)">
          <option value="highest_market_vendor">Highest of market or vendor value</option>
          <option value="highest_market_buy_used">Highest of market or buy-used value (2x vendor)</option>
          <option value="vendor_only">Vendor value only</option>
          <option value="buy_used_only">Buy-used value only (2x vendor)</option>
          <option value="market_only">Market value only</option>
        </select>
      </div>
      <p class="text-text-dim text-xs mt-1.5 mb-0">
        <template v-if="settingsStore.settings.itemValuationMode === 'vendor_only' || settingsStore.settings.itemValuationMode === 'buy_used_only' || settingsStore.settings.itemValuationMode === 'market_only'">
          Tooltips still show both vendor and market values, but only the selected source is used in wealth calculations.
        </template>
        <template v-else>
          Uses the higher of the two values for wealth calculations. Both are always shown in tooltips.
        </template>
      </p>
    </AccordionSection>

    <!-- Add item accordion -->
    <AccordionSection :default-open="false" class="shrink-0">
      <template #title>Add Market Value</template>
      <div class="flex gap-2 items-end mt-2">
        <div class="flex-1 relative">
          <label class="text-text-muted text-xs block mb-1">Item</label>
          <div v-if="addId" class="flex items-center gap-2 h-7.5 px-2 bg-surface-elevated border border-border-default rounded">
            <ItemInline :reference="addName" />
            <button
              class="text-text-muted hover:text-text-primary bg-transparent border-none cursor-pointer text-xs ml-auto"
              @click="clearSelectedItem">
              &times;
            </button>
          </div>
          <input
            v-else
            ref="itemSearchInput"
            v-model="addQuery"
            class="input w-full text-sm"
            placeholder="Search for an item..."
            autocomplete="off"
            @input="onItemSearch"
            @keydown.down.prevent="itemDropdownIndex = Math.min(itemDropdownIndex + 1, itemSuggestions.length - 1)"
            @keydown.up.prevent="itemDropdownIndex = Math.max(itemDropdownIndex - 1, 0)"
            @keydown.enter.prevent="selectSuggestion(itemSuggestions[itemDropdownIndex])"
            @keydown.escape="clearAddForm" />
          <ul
            v-if="itemSuggestions.length > 0 && !addId"
            class="absolute z-10 left-0 right-0 top-full mt-0.5 bg-surface-card border border-border-default rounded shadow-lg max-h-48 overflow-y-auto list-none m-0 p-0">
            <li
              v-for="(item, idx) in itemSuggestions"
              :key="item.id"
              class="px-3 py-1.5 text-sm cursor-pointer hover:bg-surface-elevated flex items-center gap-2"
              :class="{ 'bg-surface-elevated': idx === itemDropdownIndex }"
              @mousedown.prevent="selectSuggestion(item)">
              <ItemInline :reference="item.name" />
              <span class="text-text-dim text-[10px] ml-auto tabular-nums">#{{ item.id }}</span>
            </li>
          </ul>
          <div v-if="addQuery.length > 0 && !addId && itemSuggestions.length === 0 && !isSearching" class="text-red-400 text-[10px] mt-0.5">
            No items found matching "{{ addQuery }}"
          </div>
        </div>
        <div class="w-32">
          <label class="text-text-muted text-xs block mb-1">Price (councils)</label>
          <input
            ref="priceInput"
            v-model.number="addPrice"
            type="number"
            min="0"
            class="input w-full text-sm"
            placeholder="0"
            @keydown.enter.prevent="addValue"
            @keydown.escape="clearAddForm" />
          <div v-if="addPrice != null && addPrice < 0" class="text-red-400 text-[10px] mt-0.5">
            Price must be positive
          </div>
        </div>
        <div class="w-40">
          <label class="text-text-muted text-xs block mb-1">Notes (optional)</label>
          <input
            v-model="addNotes"
            class="input w-full text-sm"
            placeholder=""
            @keydown.enter.prevent="addValue"
            @keydown.escape="clearAddForm" />
        </div>
        <button
          class="btn btn-primary text-xs"
          :disabled="!canSubmit"
          @click="addValue">
          {{ existingEntry ? 'Update' : 'Save' }}
        </button>
        <button class="btn btn-secondary text-xs" @click="clearAddForm">Clear</button>
      </div>
      <!-- Duplicate warning -->
      <div v-if="existingEntry" class="flex items-center gap-2 mt-2 px-2 py-1.5 bg-amber-900/20 border border-amber-700/30 rounded text-xs">
        <span class="text-amber-400">&#9888;</span>
        <span class="text-amber-300">
          {{ addName }} already has a market value of <span class="font-semibold tabular-nums">{{ existingEntry.market_value.toLocaleString() }}g</span>.
          Saving will update it.
        </span>
      </div>
      <!-- Success feedback -->
      <div v-if="addSuccess" class="text-accent-green text-[10px] mt-1.5">
        {{ addSuccess }}
      </div>
    </AccordionSection>

    <!-- Stall price observations (other players' stalls) — experimental -->
    <AccordionSection :default-open="false" class="shrink-0">
      <template #title>Stall Price Observations (experimental)</template>
      <div class="shrink-0 mt-2 mb-2 p-2 rounded bg-status-warning/10 border border-status-warning/30 flex items-center gap-2 text-xs">
        <span class="text-status-warning font-semibold">Experimental</span>
        <span class="text-text-secondary">Capturing prices from other players' stalls is a work in progress and has not been verified against live stalls. Entries are manual (or bought-item sentinels you price yourself) and never overwrite your Market values.</span>
      </div>
      <p class="text-text-dim text-xs mt-2 mb-2">
        Prices seen at other players' stalls — typed in from the live capture panel
        (opens automatically when you browse a stall) or auto-detected purchases.
      </p>
      <EmptyState
        v-if="stallPriceStore.observations.length === 0"
        primary="No stall prices captured yet"
        secondary="Browse another player's stall in game — a capture panel will appear. Or scan the stall window from that panel." />
      <div v-else class="overflow-y-auto max-h-80 border border-border-default rounded">
        <table class="w-full text-xs">
          <thead class="sticky top-0 bg-surface-elevated">
            <tr class="text-left text-text-secondary">
              <th class="px-2 py-1.5 font-medium">Item</th>
              <th class="px-2 py-1.5 font-medium text-right">Qty</th>
              <th class="px-2 py-1.5 font-medium text-right">Price</th>
              <th class="px-2 py-1.5 font-medium">Stall</th>
              <th class="px-2 py-1.5 font-medium">Seen</th>
              <th class="px-2 py-1.5 font-medium">Source</th>
              <th class="px-2 py-1.5"></th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="obs in stallPriceStore.observations"
              :key="obs.id"
              class="border-t border-border-default">
              <td class="px-2 py-1.5">
                <ItemInline v-if="obs.item_type_id != null" :reference="obs.item_name" />
                <span v-else class="text-amber-400" :title="'Unresolved item name — click Set price to fix'">
                  {{ obs.item_name }} (unresolved)
                </span>
              </td>
              <td class="px-2 py-1.5 text-right tabular-nums">{{ obs.quantity }}</td>
              <td class="px-2 py-1.5 text-right tabular-nums">
                <template v-if="obs.source === 'purchase' && obs.price_unit === 0">
                  <input
                    v-model.number="stallPriceEdit[obs.id]"
                    type="number"
                    min="1"
                    class="input w-20 text-xs py-0.5"
                    placeholder="price?"
                    @keydown.enter="setPurchasePrice(obs)" />
                  <button
                    class="btn btn-secondary text-[10px] py-0.5 px-1.5 ml-1"
                    @click="setPurchasePrice(obs)">
                    Set
                  </button>
                </template>
                <template v-else>{{ obs.price_unit.toLocaleString() }}g</template>
              </td>
              <td class="px-2 py-1.5 text-text-secondary truncate max-w-40" :title="obs.stall_label">
                {{ obs.stall_label }}<span v-if="obs.owner_name" class="text-text-dim"> ({{ obs.owner_name }})</span>
              </td>
              <td class="px-2 py-1.5 text-text-dim whitespace-nowrap">{{ formatRelative(obs.observed_at) }}</td>
              <td class="px-2 py-1.5">
                <span
                  class="px-1.5 py-0.5 rounded-sm text-[10px] uppercase tracking-wide"
                  :class="obs.source === 'purchase' ? 'bg-sky-900/40 text-sky-300' : 'bg-emerald-900/40 text-emerald-300'">
                  {{ obs.source }}
                </span>
              </td>
              <td class="px-2 py-1.5 text-right">
                <button
                  class="text-text-muted hover:text-red-400 bg-transparent border-none cursor-pointer text-xs"
                  title="Delete observation"
                  @click="deleteObservation(obs)">
                  &times;
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </AccordionSection>

    <!-- Bulk action bar -->
    <div v-if="selectedIds.size > 0" class="flex items-center gap-3 shrink-0 px-3 py-2 bg-surface-elevated border border-accent-gold/30 rounded">
      <span class="text-text-primary text-xs font-semibold">{{ selectedIds.size }} selected</span>
      <button
        class="btn btn-secondary text-xs"
        @click="showBulkSetPrice = true">
        Set Price
      </button>
      <button
        class="btn btn-secondary text-xs"
        @click="showBulkAdjust = true">
        Adjust Prices
      </button>
      <button
        class="btn btn-secondary text-xs text-red-400 hover:text-red-300"
        @click="showBulkDeleteConfirm = true">
        Delete Selected
      </button>
      <button
        class="text-text-muted hover:text-text-primary bg-transparent border-none cursor-pointer text-xs ml-auto"
        @click="selectedIds = new Set()">
        Clear Selection
      </button>
    </div>

    <!-- Search / filter bar -->
    <div class="flex items-center gap-3 shrink-0">
      <input
        v-model="search"
        placeholder="Filter market values..."
        class="input flex-1 text-sm" />
      <span v-if="search && filteredValues.length !== marketStore.values.length" class="text-text-muted text-xs whitespace-nowrap">
        {{ filteredValues.length }} of {{ marketStore.values.length }}
      </span>
    </div>

    <!-- Market values table -->
    <div class="flex-1 min-h-0 overflow-auto border border-border-default rounded">
      <!-- Loading skeleton -->
      <div v-if="marketStore.loading" class="p-4">
        <div v-for="i in 8" :key="i" class="flex gap-4 py-2 border-b border-border-default/50">
          <div class="h-3 rounded animate-pulse bg-surface-elevated w-1/3" />
          <div class="h-3 rounded animate-pulse bg-surface-elevated w-16 ml-auto" />
          <div class="h-3 rounded animate-pulse bg-surface-elevated w-24" />
          <div class="h-3 rounded animate-pulse bg-surface-elevated w-20" />
        </div>
      </div>

      <EmptyState
        v-else-if="filteredValues.length === 0"
        variant="panel"
        :primary="search ? 'No matching market values' : 'No market values set'"
        :secondary="search ? undefined : 'Use item tooltips or the Add section above to set prices.'" />

      <table v-else class="w-full border-collapse text-xs">
        <thead class="sticky top-0 z-10 bg-surface-base border-b border-border-default">
          <tr>
            <th class="w-8 px-2 py-1">
              <input
                type="checkbox"
                class="accent-accent-gold cursor-pointer"
                :checked="allFilteredSelected"
                :indeterminate="someFilteredSelected && !allFilteredSelected"
                @change="toggleSelectAll" />
            </th>
            <th
              class="text-[10px] uppercase tracking-wider text-text-muted font-semibold text-left px-2 py-1 cursor-pointer hover:text-text-primary select-none"
              @click="toggleSort('item_name')">
              Item
              <span v-if="sortField === 'item_name'" class="ml-0.5">{{ sortAsc ? '\u25B2' : '\u25BC' }}</span>
            </th>
            <th
              class="text-[10px] uppercase tracking-wider text-text-muted font-semibold text-right px-2 py-1 cursor-pointer hover:text-text-primary select-none w-30"
              @click="toggleSort('market_value')">
              Price
              <span v-if="sortField === 'market_value'" class="ml-0.5">{{ sortAsc ? '\u25B2' : '\u25BC' }}</span>
            </th>
            <th class="text-[10px] uppercase tracking-wider text-text-muted font-semibold text-left px-2 py-1">
              Notes
            </th>
            <th
              class="text-[10px] uppercase tracking-wider text-text-muted font-semibold text-left px-2 py-1 cursor-pointer hover:text-text-primary select-none w-30"
              @click="toggleSort('updated_at')">
              Updated
              <span v-if="sortField === 'updated_at'" class="ml-0.5">{{ sortAsc ? '\u25B2' : '\u25BC' }}</span>
            </th>
            <th class="w-25 px-2 py-1"></th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="val in filteredValues"
            :key="val.item_type_id"
            class="border-b border-border-default/50 hover:bg-surface-row-hover"
            :class="{ 'bg-accent-gold/5': selectedIds.has(val.item_type_id) }">
            <td class="px-2 py-1">
              <input
                type="checkbox"
                class="accent-accent-gold cursor-pointer"
                :checked="selectedIds.has(val.item_type_id)"
                @change="toggleSelect(val.item_type_id)" />
            </td>
            <td class="px-2 py-1 text-text-primary">
              <ItemInline :reference="val.item_name" />
            </td>
            <td class="px-2 py-1 text-right tabular-nums">
              <span v-if="editingId !== val.item_type_id" class="text-accent-gold">
                {{ val.market_value.toLocaleString() }}g
              </span>
              <input
                v-else
                v-model.number="editPrice"
                type="number"
                min="0"
                class="w-24 bg-surface-dark border border-border-default rounded px-1.5 py-0.5 text-xs text-text-primary text-right tabular-nums"
                @keydown.enter="saveEdit(val)"
                @keydown.escape="editingId = null" />
            </td>
            <td class="px-2 py-1 text-text-dim">{{ val.notes ?? '' }}</td>
            <td class="px-2 py-1 text-text-dim">{{ formatDate(val.updated_at) }}</td>
            <td class="px-2 py-1 text-right">
              <div class="flex items-center justify-end gap-1">
                <button
                  v-if="editingId !== val.item_type_id"
                  class="text-text-muted hover:text-text-primary bg-transparent border-none cursor-pointer text-xs px-1.5 py-0.5 rounded hover:bg-surface-elevated"
                  @click="startEdit(val)">
                  Edit
                </button>
                <button
                  v-if="editingId === val.item_type_id"
                  class="text-accent-green hover:text-green-400 bg-transparent border-none cursor-pointer text-xs px-1.5 py-0.5 rounded hover:bg-surface-elevated"
                  @click="saveEdit(val)">
                  Save
                </button>
                <button
                  v-if="editingId === val.item_type_id"
                  class="text-text-muted hover:text-text-primary bg-transparent border-none cursor-pointer text-xs px-1.5 py-0.5 rounded hover:bg-surface-elevated"
                  @click="editingId = null">
                  Cancel
                </button>
                <button
                  v-if="editingId !== val.item_type_id"
                  class="text-text-muted hover:text-red-400 bg-transparent border-none cursor-pointer text-xs px-1.5 py-0.5 rounded hover:bg-surface-elevated"
                  @click="deleteValue(val.item_type_id)">
                  Del
                </button>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <!-- Import dialog -->
    <div v-if="showImport" class="fixed inset-0 bg-black/50 flex items-center justify-center z-50" @click.self="showImport = false">
      <div class="bg-surface-card border border-border-default rounded-lg p-6 w-full max-w-lg">
        <h3 class="text-text-primary text-lg mb-4 mt-0">Import Market Values</h3>
        <div class="mb-3">
          <label class="text-text-muted text-sm block mb-1">Paste JSON data</label>
          <textarea
            v-model="importJson"
            rows="6"
            class="input w-full font-mono text-xs"
            placeholder='[{"item_type_id": 123, "item_name": "Item", "market_value": 500, "notes": null, "updated_at": "2026-01-01 00:00:00"}]'
          ></textarea>
        </div>
        <div class="mb-4">
          <label class="text-text-muted text-sm block mb-1">Conflict resolution</label>
          <select v-model="importStrategy" class="input">
            <option value="newest">Accept newest (by updated_at)</option>
            <option value="overwrite">Overwrite all</option>
            <option value="keep_existing">Keep existing</option>
          </select>
        </div>
        <div v-if="importResult" class="text-sm mb-3 p-2 bg-surface-elevated rounded">
          Imported: {{ importResult.imported }} | Updated: {{ importResult.updated }} | Skipped: {{ importResult.skipped }}
        </div>
        <div class="flex gap-2 justify-end">
          <button class="btn btn-secondary" @click="showImport = false">Close</button>
          <button class="btn btn-primary" :disabled="!importJson.trim()" @click="doImport">Import</button>
        </div>
      </div>
    </div>
    <!-- Bulk delete confirmation dialog -->
    <div v-if="showBulkDeleteConfirm" class="fixed inset-0 bg-black/50 flex items-center justify-center z-50" @click.self="showBulkDeleteConfirm = false">
      <div class="bg-surface-card border border-border-default rounded-lg p-6 w-full max-w-sm">
        <h3 class="text-text-primary text-lg mb-2 mt-0">Delete {{ selectedIds.size }} Market Values?</h3>
        <p class="text-text-muted text-sm mb-4">This will permanently remove the selected market values. This cannot be undone.</p>
        <div class="flex gap-2 justify-end">
          <button class="btn btn-secondary" @click="showBulkDeleteConfirm = false">Cancel</button>
          <button class="btn btn-primary bg-red-600 hover:bg-red-500 border-red-600" @click="doBulkDelete">Delete</button>
        </div>
      </div>
    </div>

    <!-- Bulk set price dialog -->
    <div v-if="showBulkSetPrice" class="fixed inset-0 bg-black/50 flex items-center justify-center z-50" @click.self="showBulkSetPrice = false">
      <div class="bg-surface-card border border-border-default rounded-lg p-6 w-full max-w-sm">
        <h3 class="text-text-primary text-lg mb-2 mt-0">Set Price for {{ selectedIds.size }} Items</h3>
        <p class="text-text-muted text-sm mb-3">All selected items will be set to the same price.</p>
        <div class="mb-4">
          <label class="text-text-muted text-xs block mb-1">New Price (councils)</label>
          <input
            v-model.number="bulkSetPriceValue"
            type="number"
            min="0"
            class="input w-full text-sm"
            placeholder="0"
            @keydown.enter.prevent="doBulkSetPrice" />
        </div>
        <div class="flex gap-2 justify-end">
          <button class="btn btn-secondary" @click="showBulkSetPrice = false">Cancel</button>
          <button
            class="btn btn-primary"
            :disabled="bulkSetPriceValue == null || bulkSetPriceValue < 0"
            @click="doBulkSetPrice">
            Set Price
          </button>
        </div>
      </div>
    </div>

    <!-- Bulk adjust prices dialog -->
    <div v-if="showBulkAdjust" class="fixed inset-0 bg-black/50 flex items-center justify-center z-50" @click.self="showBulkAdjust = false">
      <div class="bg-surface-card border border-border-default rounded-lg p-6 w-full max-w-sm">
        <h3 class="text-text-primary text-lg mb-2 mt-0">Adjust Prices for {{ selectedIds.size }} Items</h3>
        <p class="text-text-muted text-sm mb-3">Apply a percentage adjustment to all selected items.</p>
        <div class="mb-2">
          <label class="text-text-muted text-xs block mb-1">Percentage (%)</label>
          <input
            v-model.number="bulkAdjustPercent"
            type="number"
            class="input w-full text-sm"
            placeholder="e.g. 10 for +10%, -5 for -5%"
            @keydown.enter.prevent="doBulkAdjust" />
        </div>
        <p class="text-text-dim text-xs mb-4">
          <template v-if="bulkAdjustPercent != null && bulkAdjustPercent !== 0">
            {{ bulkAdjustPercent > 0 ? 'Increase' : 'Decrease' }} all selected prices by {{ Math.abs(bulkAdjustPercent) }}%.
            <template v-if="bulkAdjustPreview">
              Example: {{ bulkAdjustPreview.before.toLocaleString() }}g &rarr; {{ bulkAdjustPreview.after.toLocaleString() }}g
            </template>
          </template>
          <template v-else>Enter a positive number to increase or negative to decrease.</template>
        </p>
        <div class="flex gap-2 justify-end">
          <button class="btn btn-secondary" @click="showBulkAdjust = false">Cancel</button>
          <button
            class="btn btn-primary"
            :disabled="bulkAdjustPercent == null || bulkAdjustPercent === 0"
            @click="doBulkAdjust">
            Apply
          </button>
        </div>
      </div>
    </div>

    <!-- Stall price capture overlay (teleports to body; shows on browse event) -->
    <StallPriceCapturePanel />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, nextTick } from 'vue'
import { useToast } from '../../composables/useToast'
import { formatRelative } from '../../composables/useTimestamp'
import { useMarketStore, type MarketValue, type ImportMarketValuesResult } from '../../stores/marketStore'
import { useSettingsStore } from '../../stores/settingsStore'
import { useGameDataStore } from '../../stores/gameDataStore'
import type { ItemInfo } from '../../types/gameData'
import EmptyState from '../Shared/EmptyState.vue'
import AccordionSection from '../Shared/AccordionSection.vue'
import ItemInline from '../Shared/Item/ItemInline.vue'
import StallPriceCapturePanel from './StallPriceCapturePanel.vue'
import { useStallPriceStore } from '../../stores/stallPriceStore'
import type { StallPriceObservation } from '../../types/stallPrices'

const toast = useToast()
const marketStore = useMarketStore()
const settingsStore = useSettingsStore()
const gameDataStore = useGameDataStore()
const stallPriceStore = useStallPriceStore()

const search = ref('')
const sortField = ref<'item_name' | 'market_value' | 'updated_at'>('item_name')
const sortAsc = ref(true)

// Add form
const addName = ref('')
const addId = ref<number | null>(null)
const addPrice = ref<number | null>(null)
const addNotes = ref('')
const addQuery = ref('')
const itemSuggestions = ref<ItemInfo[]>([])
const itemDropdownIndex = ref(0)
const itemSearchInput = ref<HTMLInputElement | null>(null)
const priceInput = ref<HTMLInputElement | null>(null)
const isSearching = ref(false)
const addSuccess = ref('')
let searchDebounce: ReturnType<typeof setTimeout> | null = null
let successTimeout: ReturnType<typeof setTimeout> | null = null

/** Check if the selected item already has a market value */
const existingEntry = computed(() => {
  if (!addId.value) return null
  return marketStore.valuesByItemId[addId.value] ?? null
})

const canSubmit = computed(() => {
  return addName.value.trim() && addId.value && addPrice.value != null && addPrice.value >= 0
})

// Edit
const editingId = ref<number | null>(null)
const editPrice = ref(0)

// Bulk operations
const selectedIds = ref(new Set<number>())
const showBulkDeleteConfirm = ref(false)
const showBulkSetPrice = ref(false)
const showBulkAdjust = ref(false)
const bulkSetPriceValue = ref<number | null>(null)
const bulkAdjustPercent = ref<number | null>(null)

function toggleSelect(itemTypeId: number) {
  const newSet = new Set(selectedIds.value)
  if (newSet.has(itemTypeId)) {
    newSet.delete(itemTypeId)
  } else {
    newSet.add(itemTypeId)
  }
  selectedIds.value = newSet
}

function toggleSelectAll() {
  if (allFilteredSelected.value) {
    // Deselect all filtered
    const newSet = new Set(selectedIds.value)
    for (const v of filteredValues.value) newSet.delete(v.item_type_id)
    selectedIds.value = newSet
  } else {
    // Select all filtered
    const newSet = new Set(selectedIds.value)
    for (const v of filteredValues.value) newSet.add(v.item_type_id)
    selectedIds.value = newSet
  }
}

async function doBulkDelete() {
  try {
    await marketStore.bulkDeleteValues([...selectedIds.value])
    toast.success(`Deleted ${selectedIds.value.size} market values`)
    selectedIds.value = new Set()
  } catch (e) {
    toast.error('Bulk delete failed: ' + String(e))
  } finally {
    showBulkDeleteConfirm.value = false
  }
}

async function doBulkSetPrice() {
  if (bulkSetPriceValue.value == null || bulkSetPriceValue.value < 0) return
  const updates = [...selectedIds.value].map(id => ({
    item_type_id: id,
    market_value: bulkSetPriceValue.value!,
  }))
  try {
    await marketStore.bulkUpdateValues(updates)
    toast.success(`Set price for ${selectedIds.value.size} items to ${bulkSetPriceValue.value.toLocaleString()}g`)
    selectedIds.value = new Set()
  } catch (e) {
    toast.error('Bulk set price failed: ' + String(e))
  } finally {
    showBulkSetPrice.value = false
    bulkSetPriceValue.value = null
  }
}

async function doBulkAdjust() {
  if (bulkAdjustPercent.value == null || bulkAdjustPercent.value === 0) return
  const pct = bulkAdjustPercent.value
  const updates = [...selectedIds.value]
    .map(id => {
      const existing = marketStore.valuesByItemId[id]
      if (!existing) return null
      return {
        item_type_id: id,
        market_value: Math.max(0, Math.round(existing.market_value * (1 + pct / 100))),
      }
    })
    .filter((u): u is { item_type_id: number; market_value: number } => u !== null)
  try {
    await marketStore.bulkUpdateValues(updates)
    const direction = pct > 0 ? 'increased' : 'decreased'
    toast.success(`Prices ${direction} by ${Math.abs(pct)}% for ${updates.length} items`)
    selectedIds.value = new Set()
  } catch (e) {
    toast.error('Bulk adjust failed: ' + String(e))
  } finally {
    showBulkAdjust.value = false
    bulkAdjustPercent.value = null
  }
}

// Import
const showImport = ref(false)
const importJson = ref('')
const importStrategy = ref('newest')
const importResult = ref<ImportMarketValuesResult | null>(null)

// Summary stats
const totalMarketValue = computed(() =>
  marketStore.values.reduce((sum, v) => sum + v.market_value, 0)
)

const avgPrice = computed(() => {
  if (marketStore.values.length === 0) return 0
  return Math.round(totalMarketValue.value / marketStore.values.length)
})

const highestPricedItem = computed(() => {
  if (marketStore.values.length === 0) return null
  return marketStore.values.reduce((max, v) => v.market_value > max.market_value ? v : max, marketStore.values[0])
})

const filteredValues = computed(() => {
  let vals = [...marketStore.values]

  if (search.value.trim()) {
    const q = search.value.toLowerCase()
    vals = vals.filter(v =>
      v.item_name.toLowerCase().includes(q) ||
      v.item_type_id.toString().includes(q) ||
      (v.notes?.toLowerCase().includes(q) ?? false)
    )
  }

  vals.sort((a, b) => {
    let cmp = 0
    if (sortField.value === 'item_name') {
      cmp = a.item_name.localeCompare(b.item_name)
    } else if (sortField.value === 'market_value') {
      cmp = a.market_value - b.market_value
    } else {
      cmp = a.updated_at.localeCompare(b.updated_at)
    }
    return sortAsc.value ? cmp : -cmp
  })

  return vals
})

const allFilteredSelected = computed(() => {
  if (filteredValues.value.length === 0) return false
  return filteredValues.value.every(v => selectedIds.value.has(v.item_type_id))
})

const someFilteredSelected = computed(() => {
  return filteredValues.value.some(v => selectedIds.value.has(v.item_type_id))
})

const bulkAdjustPreview = computed(() => {
  if (bulkAdjustPercent.value == null || bulkAdjustPercent.value === 0) return null
  const first = filteredValues.value.find(v => selectedIds.value.has(v.item_type_id))
  if (!first) return null
  const before = first.market_value
  const after = Math.max(0, Math.round(before * (1 + bulkAdjustPercent.value / 100)))
  return { before, after }
})

function toggleSort(field: 'item_name' | 'market_value' | 'updated_at') {
  if (sortField.value === field) {
    sortAsc.value = !sortAsc.value
  } else {
    sortField.value = field
    sortAsc.value = true
  }
}

function formatDate(dateStr: string): string {
  if (!dateStr) return ''
  return formatRelative(dateStr)
}

function onItemSearch() {
  addId.value = null
  addName.value = ''
  itemDropdownIndex.value = 0

  if (searchDebounce) clearTimeout(searchDebounce)
  const q = addQuery.value.trim()
  if (!q) {
    itemSuggestions.value = []
    isSearching.value = false
    return
  }
  isSearching.value = true
  searchDebounce = setTimeout(async () => {
    try {
      let items = await gameDataStore.searchItems(q)
      if (!settingsStore.settings.showUnobtainableItems) {
        items = items.filter(i => !i.keywords.includes('Lint_NotObtainable'))
      }
      itemSuggestions.value = items.slice(0, 20)
    } catch {
      itemSuggestions.value = []
    } finally {
      isSearching.value = false
    }
  }, 200)
}

function selectSuggestion(item: ItemInfo | undefined) {
  if (!item) return
  addId.value = item.id
  addName.value = item.name
  addQuery.value = item.name
  itemSuggestions.value = []
  // Auto-focus price input for fast entry
  nextTick(() => {
    priceInput.value?.focus()
    priceInput.value?.select()
  })
}

function clearSelectedItem() {
  addId.value = null
  addName.value = ''
  addQuery.value = ''
  addPrice.value = null
  nextTick(() => itemSearchInput.value?.focus())
}

function clearAddForm() {
  addQuery.value = ''
  addName.value = ''
  addId.value = null
  addPrice.value = null
  addNotes.value = ''
  itemSuggestions.value = []
  addSuccess.value = ''
}

async function addValue() {
  if (!canSubmit.value) return
  const wasUpdate = !!existingEntry.value
  const name = addName.value.trim()
  const price = addPrice.value!
  await marketStore.setValue(addId.value!, name, price, addNotes.value || undefined)

  // Show success feedback
  if (successTimeout) clearTimeout(successTimeout)
  addSuccess.value = wasUpdate
    ? `Updated ${name} to ${price.toLocaleString()}g`
    : `Added ${name} at ${price.toLocaleString()}g`
  successTimeout = setTimeout(() => { addSuccess.value = '' }, 3000)

  // Reset form for rapid batch entry (keep section open)
  addQuery.value = ''
  addName.value = ''
  addId.value = null
  addPrice.value = null
  addNotes.value = ''
  itemSuggestions.value = []
  // Focus back on item search for next entry
  nextTick(() => itemSearchInput.value?.focus())
}

function startEdit(val: MarketValue) {
  editingId.value = val.item_type_id
  editPrice.value = val.market_value
}

async function saveEdit(val: MarketValue) {
  await marketStore.setValue(val.item_type_id, val.item_name, editPrice.value, val.notes ?? undefined)
  editingId.value = null
}

async function deleteValue(itemTypeId: number) {
  await marketStore.deleteValue(itemTypeId)
}

async function doExport() {
  try {
    const json = await marketStore.exportValues()
    await navigator.clipboard.writeText(json)
    toast.success('Market values copied to clipboard')
  } catch (e) {
    console.error('Export failed:', e)
  }
}

async function doImport() {
  try {
    importResult.value = await marketStore.importValues(importJson.value, importStrategy.value)
  } catch (e) {
    console.error('Import failed:', e)
    toast.error('Import failed: ' + String(e))
  }
}

async function updateMode(mode: string) {
  await settingsStore.updateSettings({ marketPriceMode: mode })
  await marketStore.loadAll()
}

async function updateValuationMode(mode: string) {
  await settingsStore.updateSettings({ itemValuationMode: mode })
}

onMounted(() => {
  if (marketStore.values.length === 0) {
    marketStore.loadAll()
  }
  if (stallPriceStore.observations.length === 0) {
    void stallPriceStore.loadObservations()
  }
})

// ── Stall price observations ────────────────────────────────────────────────

/** Per-row price edit inputs for purchase-sentinel rows. */
const stallPriceEdit = ref<Record<number, number | undefined>>({})

async function setPurchasePrice(obs: StallPriceObservation): Promise<void> {
  const value = stallPriceEdit.value[obs.id]
  if (value == null || value < 1 || !Number.isFinite(value)) return
  try {
    await stallPriceStore.updateObservation(obs.id, value)
    stallPriceEdit.value[obs.id] = undefined
  } catch (e) {
    toast.error('Failed to set price: ' + String(e))
  }
}

async function deleteObservation(obs: StallPriceObservation): Promise<void> {
  try {
    await stallPriceStore.deleteObservation(obs.id)
  } catch (e) {
    toast.error('Failed to delete observation: ' + String(e))
  }
}
</script>
