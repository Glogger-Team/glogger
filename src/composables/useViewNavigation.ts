import type { InjectionKey } from "vue";
import { inject, provide, ref } from "vue";

export interface ViewNavigationTarget {
  view: string;
  subTab?: string;
  context?: Record<string, unknown>;
}

export type NavigateToViewFn = (target: ViewNavigationTarget) => void;

export const VIEW_NAV_KEY: InjectionKey<NavigateToViewFn> = Symbol("view-navigation");

export function provideViewNavigation(fn: NavigateToViewFn) {
  provide(VIEW_NAV_KEY, fn);
}

export function useViewNavigation() {
  const navigateToView = inject(VIEW_NAV_KEY, () => {
    console.warn("View navigation not provided");
  });
  return { navigateToView };
}

/** Pending watchword rule ID to navigate to — set by the widget, consumed by WatchwordsView */
export const pendingWatchwordRuleId = ref<number | null>(null);

/** Pending skill name to select — set by search navigation, consumed by SkillsScreen */
export const pendingSkillSelection = ref<string | null>(null)
