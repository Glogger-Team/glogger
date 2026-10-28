# Crafting — XP Leveling Optimizer

## Overview

Given a skill and target level, computes a crafting plan considering XP rewards, first-time bonuses, and cost efficiency.

## Inputs

- **Skill** — any crafting skill (auto-populated list from CDN)
- **Current level** — auto-filled from latest Character report
- **Target level** — desired level
- **Strategy** — Combined, First-Time Rush, or Cost-Efficient
- **Include unlearned recipes** — toggle to show recipes not yet known
- **Excluded recipes** — manually remove specific recipes from consideration

## XP Calculation

- XP needed is computed from CDN `xp_tables` (per-level amounts, not cumulative)
- Per-recipe XP uses `reward_skill_xp` (standard) and `reward_skill_xp_first_time` (bonus for first craft)
- First-time bonus XP is not modified by XP buffs — it is always applied at its base value
- Recipes are matched by `reward_skill` (not `skill`) — some recipes grant XP in a different skill than the one used to craft
- First-time bonus eligibility checked against `character_recipe_completions` from character export
- `reward_skill_xp_drop_off_level` flags recipes that become inefficient past a certain level

## Strategies

- **First-Time Rush** — craft each unlearned recipe once for bonus XP, then grind the most efficient recipe
- **Cost-Efficient** — minimize gold spent per XP gained
- **Combined** — first-time bonuses first, then cost-efficient grinding

## Recipe Color Coding

Recipes in the list are color-coded by status relative to the current planning level:

**Full XP range** (at or above recipe level, below drop-off):
- **White** — known and previously crafted (standard XP only)
- **Green** — known but never crafted (eligible for first-time bonus XP)
- **Red** — unknown/unlearned recipe

**Reduced XP range** (at or above `reward_skill_xp_drop_off_level`):
- **Dark grey** — known and crafted (minimal value)
- **Dark green** — known but uncrafted (first-time bonus still available)
- **Dark red** — unknown recipe in drop-off range

**Above current level:**
- **Blue** — recipe level requirement exceeds current planning level

## Output

Results are displayed grouped by level transition (e.g., "Lv 33 → 34", "Lv 34 → 35"):
- Each level shows the recipes to craft, quantities, XP gained, and estimated cost
- Summary totals for total crafts, total cost, and XP breakdown
- One-click "Create Crafting Project" to convert the plan into a project with all recipe entries
