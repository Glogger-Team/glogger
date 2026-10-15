# Widget: Statehelm Gifting

**ID:** `statehelm-summary` | **Default size:** Medium | **Component:** `widgets/StatehelmSummaryWidget.vue`

Skill-driven summary of weekly Statehelm gift progress:
- Header: total gifts given / max possible + weekly reset countdown
- Progress bar (gold fill)
- **Combat** section: the NPCs for the **2 currently equipped** combat skills plus
  the player's **top 4 combat skills** — up to `4 + equipped` slots, equipped first
  and deduped.
- **Non-Combat** section: the NPCs that train the player's **top 2 non-combat skills**.
- Each row shows `NpcInline` (hoverable), the driving skill name (a ✦ marks an
  equipped skill), and gift dots: filled (gold) for given, empty (dim) for remaining.

### Selection logic (`useStatehelmTracker`)

- **Skill ranking** is by **base level** (highest first), over the player's own skills.
- **Combat vs non-combat** comes from the CDN `combat` flag per skill.
- **NPC category** uses "combat wins ties": an NPC that trains *any* combat skill is
  treated as a combat NPC; only purely non-combat NPCs are eligible for the non-combat
  section.
- **Skill → NPC**: a skill can be trained by multiple Statehelm NPCs; the one with the
  **highest current favor standing** represents it.
- **Falloff**: an NPC drops off the list (and the next-highest skill of that category
  takes the slot) when any of these is true:
  - its 5 weekly gifts have been donated, or
  - it is at **Soul Mates** (top favor tier), or
  - it is at **Like Family** *and* offers no storage — storage NPCs keep scaling past
    Like Family, so those stay on the list.

Weekly reset is Monday 00:00 America/New_York (the game's own reset instant; 04:00/05:00 UTC). Uses the same `useStatehelmTracker` composable as the
full Statehelm tab — calls `loadGiftLog()` and `loadSkillMeta()` on mount.

**Data source:** `useStatehelmTracker` composable (gift log from database, NPC + skill data
from CDN, favor and skill levels from game state).
