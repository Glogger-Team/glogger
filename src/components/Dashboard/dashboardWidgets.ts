import type { Component } from "vue";
import ContextBar from "./ContextBar.vue";
import ContextBarConfig from "./ContextBarConfig.vue";
import ZoneNpcsWidgetConfig from "./widgets/ZoneNpcsWidgetConfig.vue";
import PlayerNotes from "./PlayerNotes.vue";
import SkillTrackingWidget from "./widgets/SkillTrackingWidget.vue";
import XpRateWidget from "./widgets/XpRateWidget.vue";
import ProdigyTrackerWidget from "./widgets/ProdigyTrackerWidget.vue";
import CombatWisdomWidget from "./widgets/CombatWisdomWidget.vue";
import ItemsIncomingWidget from "./widgets/ItemsIncomingWidget.vue";
import ItemsOutgoingWidget from "./widgets/ItemsOutgoingWidget.vue";
import CouncilsWidget from "./widgets/CouncilsWidget.vue";
import FavorChangesWidget from "./widgets/FavorChangesWidget.vue";
import CriticalResourcesWidget from "./widgets/CriticalResourcesWidget.vue";
import WatchwordDetectionsWidget from "./widgets/WatchwordDetectionsWidget.vue";
import DeathTrackerWidget from "./widgets/DeathTrackerWidget.vue";
import RecipeItemsWidget from "./widgets/RecipeItemsWidget.vue";
import ZoneNpcsWidget from "./widgets/ZoneNpcsWidget.vue";
import GiftWatcherWidget from "./widgets/GiftWatcherWidget.vue";
import TeleportCodesWidget from "./widgets/TeleportCodesWidget.vue";
import MilkingTimersWidget from "./widgets/MilkingTimersWidget.vue";
import MushroomFarmingWidget from "./widgets/MushroomFarmingWidget.vue";
import StatTrackerWidget from "./widgets/StatTrackerWidget.vue";
import StatTrackerConfig from "./widgets/StatTrackerConfig.vue";
import WordsOfPowerWidget from "./widgets/WordsOfPowerWidget.vue";
import TimerWidget from "./widgets/TimerWidget.vue";
import TripPlannerWidget from "./widgets/TripPlannerWidget.vue";
import TripPlannerWidgetConfig from "./widgets/TripPlannerWidgetConfig.vue";
import GardenAlmanacWidget from "./widgets/GardenAlmanacWidget.vue";
import HarvestAlmanacWidget from "./widgets/HarvestAlmanacWidget.vue";
import MessagesWidget from "./widgets/MessagesWidget.vue";
import HoplologyWidget from "./widgets/HoplologyWidget.vue";
import VendorCouncilWidget from "./widgets/VendorCouncilWidget.vue";
import RouletteWidget from "./widgets/RouletteWidget.vue";
import ArenaWidget from "./widgets/ArenaWidget.vue";
import StatehelmSummaryWidget from "./widgets/StatehelmSummaryWidget.vue";
export type WidgetSize = "small" | "medium" | "large";

export interface DashboardWidget {
  id: string;
  name: string;
  component: Component;
  configComponent?: Component;
  defaultSize: WidgetSize;
}

/**
 * Column spans for each size class in the responsive grid.
 * Grid uses repeat(auto-fill, minmax(280px, 1fr)) so column count adapts to screen width.
 * Small = 1 column, Medium = 2 columns, Large = full row.
 */
export const SIZE_CLASSES: Record<WidgetSize, string> = {
  small: "",
  medium: "col-span-2",
  large: "col-span-4",
};

/**
 * Master widget registry. Order here defines the default card order
 * for new users (before any preferences are saved).
 */
export const DASHBOARD_WIDGETS: DashboardWidget[] = [
  {
    id: "context-bar",
    name: "Status",
    component: ContextBar,
    configComponent: ContextBarConfig,
    defaultSize: "small",
  },
  {
    id: "skill-tracking",
    name: "Live Skill Tracking",
    component: SkillTrackingWidget,
    defaultSize: "large",
  },
  {
    id: "xp-rate",
    name: "XP Rate",
    component: XpRateWidget,
    defaultSize: "small",
  },
  {
    id: "prodigy-tracker",
    name: "Prodigy Tracker",
    component: ProdigyTrackerWidget,
    defaultSize: "small",
  },
  {
    id: "combat-wisdom",
    name: "Combat Wisdom",
    component: CombatWisdomWidget,
    defaultSize: "medium",
  },
  {
    id: "zone-npcs",
    name: "Zone NPCs",
    component: ZoneNpcsWidget,
    configComponent: ZoneNpcsWidgetConfig,
    defaultSize: "medium",
  },
  {
    id: "items-incoming",
    name: "Items Incoming",
    component: ItemsIncomingWidget,
    defaultSize: "medium",
  },
  {
    id: "items-outgoing",
    name: "Items Outgoing",
    component: ItemsOutgoingWidget,
    defaultSize: "medium",
  },
  {
    id: "councils",
    name: "Councils",
    component: CouncilsWidget,
    defaultSize: "medium",
  },
  {
    id: "vendor-councils",
    name: "Vendor Councils",
    component: VendorCouncilWidget,
    defaultSize: "medium",
  },

  {
    id: "favor-changes",
    name: "Favor Changes",
    component: FavorChangesWidget,
    defaultSize: "medium",
  },
  {
    id: "player-notes",
    name: "Notes",
    component: PlayerNotes,
    defaultSize: "medium",
  },

  {
    id: "critical-resources",
    name: "Critical Resources",
    component: CriticalResourcesWidget,
    defaultSize: "small",
  },
  // Statehelm: weekly gift counts are Player.log-only (dark on the current
  // client) but the summary stays useful for favor tiers/snapshots — keep
  // the widget; the per-NPC gift rows simply stay at 0/x until counts return.
  {
    id: "statehelm-summary",
    name: "Statehelm",
    component: StatehelmSummaryWidget,
    defaultSize: "medium",
  },
  {
    id: "watchword-detections",
    name: "Watchword Alerts",
    component: WatchwordDetectionsWidget,
    defaultSize: "medium",
  },
  {
    id: "death-tracker",
    name: "Death Tracker",
    component: DeathTrackerWidget,
    defaultSize: "medium",
  },
  {
    id: "recipe-items",
    name: "Recipe Items",
    component: RecipeItemsWidget,
    defaultSize: "medium",
  },
  {
    id: "gift-watcher",
    name: "Gift Watcher",
    component: GiftWatcherWidget,
    defaultSize: "medium",
  },
  {
    id: "teleport-codes",
    name: "Teleport Machine Codes",
    component: TeleportCodesWidget,
    defaultSize: "medium",
  },
  {
    id: "milking-timers",
    name: "Milking Timers",
    component: MilkingTimersWidget,
    defaultSize: "small",
  },
  {
    id: "mushroom-farming",
    name: "Mushroom Farming",
    component: MushroomFarmingWidget,
    defaultSize: "large",
  },
  {
    id: "stat-tracker",
    name: "Stat Tracker",
    component: StatTrackerWidget,
    configComponent: StatTrackerConfig,
    defaultSize: "small",
  },
  {
    id: "words-of-power",
    name: "Words of Power",
    component: WordsOfPowerWidget,
    defaultSize: "medium",
  },
  {
    id: "timers",
    name: "Timers",
    component: TimerWidget,
    defaultSize: "small",
  },
  {
    id: "trip-planner",
    name: "Trip Planner",
    component: TripPlannerWidget,
    configComponent: TripPlannerWidgetConfig,
    defaultSize: "medium",
  },
  {
    id: "gardening",
    name: "Gardening",
    component: GardenAlmanacWidget,
    defaultSize: "small",
  },
  {
    id: "harvest-almanac",
    name: "Almanac of Corruption",
    component: HarvestAlmanacWidget,
    defaultSize: "small",
  },
  {
    id: "messages",
    name: "Messages",
    component: MessagesWidget,
    defaultSize: "medium",
  },
  {
    id: "hoplology",
    name: "Hoplology",
    component: HoplologyWidget,
    defaultSize: "small",
  },
  {
    id: "roulette",
    name: "Casino Roulette",
    component: RouletteWidget,
    defaultSize: "medium",
  },
  // Arena: personal bets are Player.log-only (dark on the current client,
  // banner inside the widget says so) but match history tracks via Kuzavek
  // chat narration — keep the widget so matches stay visible.
  {
    id: "arena",
    name: "Arena",
    component: ArenaWidget,
    defaultSize: "medium",
  },
];

/** Default card order — used when no user preferences exist */
export const DEFAULT_CARD_ORDER = DASHBOARD_WIDGETS.map((w) => w.id);
