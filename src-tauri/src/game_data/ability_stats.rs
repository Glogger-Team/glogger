//! Pure formula engine that folds a build's assigned gear-mod contributions into
//! an ability's combat stats, producing the same kind of computed numbers the
//! in-game ability tooltip shows (damage, DoTs, heals/restores, costs).
//!
//! Project: Gorgon's direct-damage formula (wiki: Combat):
//! ```text
//! final = base * (1 + ΣModBaseDamage% + ΣModDamage%) + ΣDeltaDamage * (1 + ΣModDamage%)
//! ```
//! DoTs and SpecialValues share the same base-value + delta/mod shape, so a single
//! [`apply_formula`] helper drives all of them (with no base-only % bucket for those).
//!
//! All game-formula logic lives here so it can be unit-tested without the CDN or a
//! running app; the Tauri command in `cdn_commands.rs` only does data extraction.

use serde::Serialize;
use std::collections::HashSet;

use super::abilities::{CombatStats, DotEffect, SpecialValue};

/// A single resolved gear-mod effect that may feed one of an ability's attribute
/// buckets. `token` is the raw attribute key (e.g. `MOD_SKILL_UNARMED`).
#[derive(Debug, Clone)]
pub struct ModEffect {
    pub token: String,
    pub value: f64,
    /// Display source, e.g. `"Martial … of Deadly Fists (Hands)"`.
    pub source: String,
    /// Human-readable attribute label, e.g. `"Unarmed Base Damage %"`.
    pub label: String,
    /// Attribute `DisplayType` (`AsBuffMod`, `AsBuffDelta`, …) for frontend formatting.
    pub display_type: String,
}

/// One contributing mod line attached to a computed stat, for the breakdown UI.
#[derive(Debug, Serialize, Clone)]
pub struct ContributionLine {
    pub source: String,
    pub label: String,
    pub value: f64,
    pub display_type: String,
    /// Which formula bucket this fed: `added` | `base_mod` | `damage_mod` |
    /// `crit_mod` | `delta` | `mod` | `delta_base`.
    pub bucket: String,
}

/// A base value and its build-modified effective value, with the mods responsible.
#[derive(Debug, Serialize, Clone, Default)]
pub struct ValueBreakdown {
    pub base: f64,
    pub effective: f64,
    /// Flat amount added by mods (before/with the % multipliers).
    pub added: f64,
    /// Base-only percentage total (fraction, e.g. 0.45 == +45%). Damage only.
    pub base_pct: f64,
    /// All-damage percentage total (fraction).
    pub all_pct: f64,
    pub contributions: Vec<ContributionLine>,
    /// True when the base was zero but a mod made it non-zero (dormant → active).
    pub dormant_activated: bool,
}

impl ValueBreakdown {
    /// Whether any mod changed this value (drives "show the with-build column").
    pub fn is_modified(&self) -> bool {
        !self.contributions.is_empty() && (self.effective - self.base).abs() > f64::EPSILON
    }
}

/// One direct (up-front) damage component of an ability. Project: Gorgon abilities can carry up
/// to three: normal `Damage`, armor-bypassing `HealthSpecificDamage`, and `ArmorSpecificDamage`.
/// All three are governed by the same direct-damage modifier arrays (there are no per-shape mod
/// arrays in the data), so each is folded with the same buckets — only the base differs.
#[derive(Debug, Serialize, Clone)]
pub struct DirectDamageBreakdown {
    /// Which component this is: `normal` | `health` | `armor`. The frontend maps this to a label
    /// ("Damage" / "Damage to Health" / "Damage to Armor").
    pub kind: String,
    pub value: ValueBreakdown,
}

#[derive(Debug, Serialize, Clone)]
pub struct DotBreakdown {
    pub damage_type: Option<String>,
    pub num_ticks: f64,
    pub duration: Option<f64>,
    pub per_tick: ValueBreakdown,
    pub total_base: f64,
    pub total_effective: f64,
}

#[derive(Debug, Serialize, Clone)]
pub struct SpecialValueBreakdown {
    pub label: Option<String>,
    pub suffix: Option<String>,
    pub display_type: Option<String>,
    pub value: ValueBreakdown,
}

/// Full computed combat picture for one ability under a given build.
#[derive(Debug, Serialize, Clone, Default)]
pub struct AbilityBuildStats {
    pub damage_type: Option<String>,
    /// Every up-front damage component the ability has (normal / health-specific / armor-specific),
    /// in that order. Empty when the ability deals no direct damage (pure DoT / utility).
    pub direct_damages: Vec<DirectDamageBreakdown>,
    pub dots: Vec<DotBreakdown>,
    pub special_values: Vec<SpecialValueBreakdown>,
    pub power_cost: Option<ValueBreakdown>,
    pub rage_cost: Option<ValueBreakdown>,
    /// True if any component is affected by the build's mods.
    pub any_modified: bool,
}

/// The PG combat formula. `base_pct` is the base-only multiplier bucket (damage only);
/// `all_pct` multiplies both the base and the flat `added` amount.
fn apply_formula(base: f64, added: f64, base_pct: f64, all_pct: f64) -> f64 {
    base * (1.0 + base_pct + all_pct) + added * (1.0 + all_pct)
}

/// Sum the values of every [`ModEffect`] whose token is in `tokens`, pushing each as a
/// labeled contribution line into `out`.
fn collect_bucket(
    mods: &[ModEffect],
    tokens: &[String],
    bucket: &str,
    out: &mut Vec<ContributionLine>,
) -> f64 {
    if tokens.is_empty() {
        return 0.0;
    }
    let set: HashSet<&str> = tokens.iter().map(|s| s.as_str()).collect();
    let mut total = 0.0;
    for m in mods {
        if set.contains(m.token.as_str()) {
            total += m.value;
            out.push(ContributionLine {
                source: m.source.clone(),
                label: m.label.clone(),
                value: m.value,
                display_type: m.display_type.clone(),
                bucket: bucket.to_string(),
            });
        }
    }
    total
}

/// Compute one direct-damage component's breakdown from a base value, using the full PG formula
/// (added / base-mult / damage-mult buckets) and the ability's shared direct-damage modifier
/// arrays. Crit-damage mods are surfaced as informational contribution lines but excluded from
/// the non-crit `effective`.
///
/// Like DoTs, direct damage also picks up the generic per-damage-type and universal *direct*
/// attributes (`BOOST_/MOD_<TYPE>_DIRECT`, `*_UNIVERSAL_DIRECT`) selected by the ability's
/// `damage_type`. Flat-damage gear/items (e.g. a focus's "Direct Psychic Damage +104") feed
/// these tokens, and abilities never list them in their own delta/mod arrays.
fn direct_damage_value(
    base: f64,
    stats: &CombatStats,
    damage_type: Option<&str>,
    mods: &[ModEffect],
) -> ValueBreakdown {
    let mut contributions = Vec::new();

    let mut delta_tokens = stats.attributes_that_delta_damage.clone();
    delta_tokens.extend(direct_tokens("BOOST", damage_type));
    let mut damage_mod_tokens = stats.attributes_that_mod_damage.clone();
    damage_mod_tokens.extend(direct_tokens("MOD", damage_type));

    let added = collect_bucket(mods, &delta_tokens, "added", &mut contributions);
    let base_pct = collect_bucket(
        mods,
        &stats.attributes_that_mod_base_damage,
        "base_mod",
        &mut contributions,
    );
    let all_pct = collect_bucket(mods, &damage_mod_tokens, "damage_mod", &mut contributions);
    // Crit mods are informational only (don't change the listed hit).
    collect_bucket(
        mods,
        &stats.attributes_that_mod_crit_damage,
        "crit_mod",
        &mut contributions,
    );

    let effective = apply_formula(base, added, base_pct, all_pct);
    ValueBreakdown {
        base,
        effective,
        added,
        base_pct,
        all_pct,
        dormant_activated: base == 0.0 && effective != 0.0,
        contributions,
    }
}

/// Build the ordered list of direct-damage components present on the ability — normal `Damage`,
/// armor-bypassing `HealthSpecificDamage`, and `ArmorSpecificDamage` — each folded with the
/// build's direct-damage mods. Components the ability doesn't have are omitted.
fn direct_damages(
    stats: &CombatStats,
    damage_type: Option<&str>,
    mods: &[ModEffect],
) -> Vec<DirectDamageBreakdown> {
    let mut out = Vec::new();
    for (kind, base) in [
        ("normal", stats.damage),
        ("health", stats.health_specific_damage),
        ("armor", stats.armor_specific_damage),
    ] {
        if let Some(base) = base {
            out.push(DirectDamageBreakdown {
                kind: kind.to_string(),
                value: direct_damage_value(base as f64, stats, damage_type, mods),
            });
        }
    }
    out
}

/// Generic *direct*-damage attribute tokens that apply to any direct hit of `damage_type`, plus
/// the universal (all-type) direct modifier — the direct counterpart of [`indirect_tokens`].
/// `prefix` is `BOOST` (flat) or `MOD` (percent). e.g. ("BOOST", Some("Psychic")) →
/// ["BOOST_UNIVERSAL_DIRECT", "BOOST_PSYCHIC_DIRECT", "BOOST_PSYCHIC"].
///
/// The bare `BOOST_<TYPE>` / `MOD_<TYPE>` form is the dual-scope "X Damage (Direct & Per-Tick)"
/// attribute family (e.g. `{MOD_ELECTRICITY}`, `{BOOST_TRAUMA}`): it moves direct hits *and* DoT
/// ticks of that type, so both this and [`indirect_tokens`] include it.
fn direct_tokens(prefix: &str, damage_type: Option<&str>) -> Vec<String> {
    let mut v = vec![format!("{prefix}_UNIVERSAL_DIRECT")];
    if let Some(dt) = damage_type {
        let up = dt.to_uppercase();
        v.push(format!("{prefix}_{up}_DIRECT"));
        v.push(format!("{prefix}_{up}"));
    }
    v
}

/// DoT damage is *indirect* damage, governed only by indirect-damage modifiers — the DoT's
/// own per-ability tokens plus the generic per-damage-type and universal indirect attributes
/// (`BOOST_/MOD_<TYPE>_INDIRECT`, `*_UNIVERSAL_INDIRECT`, applied globally by the DoT's
/// damage type). Base-skill-damage % and the ability's *direct*-damage modifiers do NOT
/// affect ticks.
fn dot_breakdown(dot: &DotEffect, mods: &[ModEffect]) -> DotBreakdown {
    let base = dot.damage_per_tick as f64;
    let mut contributions = Vec::new();

    let dtype = dot.damage_type.as_deref();
    let mut flat_tokens = dot.attributes_that_delta.clone();
    flat_tokens.extend(indirect_tokens("BOOST", dtype));
    let mut pct_tokens = dot.attributes_that_mod.clone();
    pct_tokens.extend(indirect_tokens("MOD", dtype));

    let added = collect_bucket(mods, &flat_tokens, "delta", &mut contributions);
    let all_pct = collect_bucket(mods, &pct_tokens, "mod", &mut contributions);
    let per_tick_eff = apply_formula(base, added, 0.0, all_pct);
    let ticks = dot.num_ticks as f64;
    DotBreakdown {
        damage_type: dot.damage_type.clone(),
        num_ticks: ticks,
        duration: dot.duration.map(|d| d as f64),
        total_base: base * ticks,
        total_effective: per_tick_eff * ticks,
        per_tick: ValueBreakdown {
            base,
            effective: per_tick_eff,
            added,
            base_pct: 0.0,
            all_pct,
            dormant_activated: base == 0.0 && per_tick_eff != 0.0,
            contributions,
        },
    }
}

/// Generic indirect-damage attribute tokens that apply to any DoT of `damage_type`, plus
/// the universal (all-type) indirect modifier. `prefix` is `BOOST` (flat per-tick) or
/// `MOD` (percent). e.g. ("BOOST", Some("Psychic")) →
/// ["BOOST_UNIVERSAL_INDIRECT", "BOOST_PSYCHIC_INDIRECT", "BOOST_PSYCHIC"].
///
/// The bare `BOOST_<TYPE>` / `MOD_<TYPE>` form is the dual-scope "X Damage (Direct & Per-Tick)"
/// family — it applies per tick here and to the up-front hit via [`direct_tokens`].
fn indirect_tokens(prefix: &str, damage_type: Option<&str>) -> Vec<String> {
    let mut v = vec![format!("{prefix}_UNIVERSAL_INDIRECT")];
    if let Some(dt) = damage_type {
        let up = dt.to_uppercase();
        v.push(format!("{prefix}_{up}_INDIRECT"));
        v.push(format!("{prefix}_{up}"));
    }
    v
}

/// Damage-type names whose uppercase form matches the `*_<TYPE>_INDIRECT` attribute tokens.
/// Also used by `cdn_commands` to sniff the damage type of prose-named DoTs ("Poison Slice").
pub(crate) const DAMAGE_TYPES: &[&str] = &[
    "Acid",
    "Cold",
    "Crushing",
    "Darkness",
    "Demonic",
    "Divine",
    "Electricity",
    "Fire",
    "Fungus",
    "Nature",
    "Piercing",
    "Poison",
    "Psychic",
    "Seafood",
    "Slashing",
    "Smiting",
    "Sonic",
    "Trauma",
];

/// True when a special value represents indirect (damage-over-time) damage — detected by a
/// DoT/indirect attribute token in its own arrays (`*ABILITYDOT*`, `*_INDIRECT`). This keeps
/// heals/armor-over-time (which use `*HEAL*`/`*ARMOR*` tokens) from picking up damage mods.
fn sv_is_indirect_damage(sv: &SpecialValue) -> bool {
    sv.attributes_that_delta_base
        .iter()
        .chain(&sv.attributes_that_delta)
        .chain(&sv.attributes_that_mod)
        .any(|t| t.contains("ABILITYDOT") || t.contains("_INDIRECT"))
}

/// Find the damage type named in a special value's label/suffix (e.g. "Trauma damage" →
/// "Trauma") so the matching generic indirect tokens can be selected. Falls back to `None`
/// (universal-only) when no type is named.
fn parse_damage_type(sv: &SpecialValue) -> Option<String> {
    let text = format!(
        "{} {}",
        sv.label.as_deref().unwrap_or(""),
        sv.suffix.as_deref().unwrap_or("")
    )
    .to_lowercase();
    DAMAGE_TYPES
        .iter()
        .find(|t| text.contains(&t.to_lowercase()))
        .map(|t| t.to_string())
}

fn special_value_breakdown(sv: &SpecialValue, mods: &[ModEffect]) -> SpecialValueBreakdown {
    let base = sv.value as f64;
    let mut contributions = Vec::new();

    // A special value that carries a DoT/indirect token (e.g. reflect-style "they suffer X
    // Trauma damage over time") is indirect damage, so the generic per-damage-type and
    // universal indirect modifiers apply on top of its own tokens — same rule as DoTs.
    let mut delta_tokens = sv.attributes_that_delta.clone();
    let mut mod_tokens = sv.attributes_that_mod.clone();
    if sv_is_indirect_damage(sv) {
        let dtype = parse_damage_type(sv);
        delta_tokens.extend(indirect_tokens("BOOST", dtype.as_deref()));
        mod_tokens.extend(indirect_tokens("MOD", dtype.as_deref()));
    }

    // DeltaBase adds to the base before the multiplier; Delta adds afterward. Both end up
    // multiplied by (1 + mod%), so they fold into `added` for the arithmetic but keep
    // distinct bucket labels for the breakdown display.
    let delta_base = collect_bucket(
        mods,
        &sv.attributes_that_delta_base,
        "delta_base",
        &mut contributions,
    );
    let delta = collect_bucket(mods, &delta_tokens, "delta", &mut contributions);
    let mod_pct = collect_bucket(mods, &mod_tokens, "mod", &mut contributions);
    let effective = apply_formula(base + delta_base, delta, 0.0, mod_pct);
    SpecialValueBreakdown {
        label: sv.label.clone(),
        suffix: sv.suffix.clone(),
        display_type: sv.display_type.clone(),
        value: ValueBreakdown {
            base,
            effective,
            added: delta_base + delta,
            base_pct: 0.0,
            all_pct: mod_pct,
            dormant_activated: base == 0.0 && effective != 0.0,
            contributions,
        },
    }
}

/// Cost breakdown (power/rage): `(base + ΣDelta) * (1 + ΣMod%)`.
fn cost_breakdown(
    base: Option<f32>,
    delta_tokens: &[String],
    mod_tokens: &[String],
    mods: &[ModEffect],
) -> Option<ValueBreakdown> {
    let base = base? as f64;
    let mut contributions = Vec::new();
    let added = collect_bucket(mods, delta_tokens, "delta", &mut contributions);
    let mod_pct = collect_bucket(mods, mod_tokens, "mod", &mut contributions);
    let effective = (base + added) * (1.0 + mod_pct);
    Some(ValueBreakdown {
        base,
        effective,
        added,
        base_pct: 0.0,
        all_pct: mod_pct,
        dormant_activated: false,
        contributions,
    })
}

/// Fold a build's `mods` into `stats`, producing the full computed picture.
pub fn compute(
    stats: &CombatStats,
    damage_type: Option<String>,
    mods: &[ModEffect],
) -> AbilityBuildStats {
    let direct = direct_damages(stats, damage_type.as_deref(), mods);
    let dots: Vec<DotBreakdown> = stats.dots.iter().map(|d| dot_breakdown(d, mods)).collect();
    let special_values: Vec<SpecialValueBreakdown> = stats
        .special_values
        .iter()
        .map(|s| special_value_breakdown(s, mods))
        .collect();
    let power_cost = cost_breakdown(
        stats.power_cost,
        &stats.attributes_that_delta_power_cost,
        &stats.attributes_that_mod_power_cost,
        mods,
    );
    let rage_cost = cost_breakdown(
        stats.rage_cost,
        &stats.attributes_that_delta_rage,
        &stats.attributes_that_mod_rage,
        mods,
    );

    let any_modified = direct.iter().any(|d| d.value.is_modified())
        || dots.iter().any(|d| d.per_tick.is_modified())
        || special_values.iter().any(|s| s.value.is_modified())
        || power_cost.as_ref().is_some_and(|c| c.is_modified())
        || rage_cost.as_ref().is_some_and(|c| c.is_modified());

    AbilityBuildStats {
        damage_type,
        direct_damages: direct,
        dots,
        special_values,
        power_cost,
        rage_cost,
        any_modified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(token: &str, value: f64) -> ModEffect {
        ModEffect {
            token: token.to_string(),
            value,
            source: format!("Mod {token}"),
            label: token.to_string(),
            display_type: "AsBuffMod".to_string(),
        }
    }

    /// Punch: base 6, +45% base (MOD_SKILL_UNARMED), +10 flat (BOOST_ABILITY_PUNCH),
    /// +50% all (MOD_ABILITY_PUNCH) → 6×(1+0.45+0.50) + 10×(1+0.50) = 11.7 + 15 = 26.7.
    #[test]
    fn punch_direct_damage_matches_pg_formula() {
        let stats = CombatStats {
            damage: Some(6.0),
            attributes_that_delta_damage: vec!["BOOST_ABILITY_PUNCH".into()],
            attributes_that_mod_base_damage: vec!["MOD_SKILL_UNARMED".into()],
            attributes_that_mod_damage: vec!["MOD_ABILITY_PUNCH".into()],
            ..Default::default()
        };
        let mods = vec![
            m("MOD_SKILL_UNARMED", 0.45),
            m("BOOST_ABILITY_PUNCH", 10.0),
            m("MOD_ABILITY_PUNCH", 0.50),
        ];
        let out = compute(&stats, Some("Crushing".into()), &mods);
        assert_eq!(out.direct_damages.len(), 1);
        assert_eq!(out.direct_damages[0].kind, "normal");
        let dd = &out.direct_damages[0].value;
        assert!((dd.base - 6.0).abs() < 1e-6);
        assert!((dd.effective - 26.7).abs() < 1e-6, "got {}", dd.effective);
        assert!((dd.added - 10.0).abs() < 1e-6);
        assert!((dd.base_pct - 0.45).abs() < 1e-6);
        assert!((dd.all_pct - 0.50).abs() < 1e-6);
        assert_eq!(dd.contributions.len(), 3);
        assert!(out.any_modified);
    }

    /// Unrelated mods must not touch an ability whose buckets don't list their tokens.
    #[test]
    fn unrelated_mods_do_not_apply() {
        let stats = CombatStats {
            damage: Some(20.0),
            attributes_that_mod_base_damage: vec!["MOD_SKILL_SWORD".into()],
            ..Default::default()
        };
        let out = compute(&stats, None, &[m("MOD_SKILL_UNARMED", 0.99)]);
        let dd = &out.direct_damages[0].value;
        assert!((dd.effective - 20.0).abs() < 1e-6);
        assert!(!dd.is_modified());
        assert!(!out.any_modified);
    }

    /// A heal SpecialValue: base 25, +5 flat heal → 30.
    #[test]
    fn special_value_heal_adds_flat() {
        let stats = CombatStats {
            special_values: vec![SpecialValue {
                label: Some("Restore".into()),
                suffix: Some("Health to You and Nearby Allies".into()),
                value: 25.0,
                attributes_that_delta: vec!["BOOST_MAJORHEAL_SENDER".into()],
                ..Default::default()
            }],
            ..Default::default()
        };
        let out = compute(&stats, None, &[m("BOOST_MAJORHEAL_SENDER", 5.0)]);
        let sv = &out.special_values[0];
        assert!((sv.value.base - 25.0).abs() < 1e-6);
        assert!((sv.value.effective - 30.0).abs() < 1e-6);
        assert_eq!(sv.label.as_deref(), Some("Restore"));
    }

    /// A DoT expressed as a SpecialValue (reflect-style, e.g. Tough Hoof's "X Trauma damage")
    /// gets generic indirect modifiers too, but not base-skill-damage.
    #[test]
    fn special_value_dot_gets_generic_indirect() {
        let stats = CombatStats {
            special_values: vec![SpecialValue {
                label: Some("For 8 seconds, each time target attacks you, they suffer".into()),
                suffix: Some("Trauma damage".into()),
                value: 50.0,
                attributes_that_delta: vec!["BOOST_ABILITYDOT_TOUGHHOOF".into()],
                ..Default::default()
            }],
            ..Default::default()
        };
        let mods = vec![
            m("BOOST_ABILITYDOT_TOUGHHOOF", 30.0), // ability-specific DoT flat
            m("MOD_TRAUMA_INDIRECT", 0.25),        // generic % indirect (Trauma)
            m("MOD_SKILL_UNARMED", 1.00),          // base-skill: must NOT apply
        ];
        let out = compute(&stats, None, &mods);
        let sv = &out.special_values[0];
        // (50 + 30) × (1 + 0.25) = 100
        assert!(
            (sv.value.effective - 100.0).abs() < 1e-6,
            "got {}",
            sv.value.effective
        );
    }

    /// A dormant SpecialValue (base 0, SkipIfZero) lights up when a mod feeds its token.
    #[test]
    fn dormant_special_value_activates() {
        let stats = CombatStats {
            special_values: vec![SpecialValue {
                label: Some("Restore".into()),
                value: 0.0,
                skip_if_zero: true,
                attributes_that_delta: vec!["BOOST_HEALINGMIST_TSYS_HEALTH_SENDER".into()],
                ..Default::default()
            }],
            ..Default::default()
        };
        let out = compute(
            &stats,
            None,
            &[m("BOOST_HEALINGMIST_TSYS_HEALTH_SENDER", 12.0)],
        );
        let sv = &out.special_values[0];
        assert!((sv.value.effective - 12.0).abs() < 1e-6);
        assert!(sv.value.dormant_activated);
        assert!(out.any_modified);
    }

    /// DoT: per-tick 10 over 4 ticks, +50% mod → per-tick 15, total 60.
    #[test]
    fn dot_applies_mod_to_each_tick() {
        let stats = CombatStats {
            dots: vec![DotEffect {
                damage_per_tick: 10.0,
                num_ticks: 4.0,
                duration: Some(8.0),
                damage_type: Some("Fire".into()),
                attributes_that_mod: vec!["MOD_ABILITYDOT_BARRAGE".into()],
                ..Default::default()
            }],
            ..Default::default()
        };
        let out = compute(&stats, None, &[m("MOD_ABILITYDOT_BARRAGE", 0.5)]);
        let dot = &out.dots[0];
        assert!((dot.per_tick.effective - 15.0).abs() < 1e-6);
        assert!((dot.total_effective - 60.0).abs() < 1e-6);
        assert!((dot.total_base - 40.0).abs() < 1e-6);
    }

    /// Mindworm: a hybrid hit (Psychic). Its up-front hit is `HealthSpecificDamage` (369),
    /// which the parser folds into `CombatStats.damage`, plus a Psychic DoT (84/tick × 4).
    /// The two parts use disjoint modifier sets: the direct hit moves with base-skill /
    /// direct-damage mods, while the DoT is *indirect* and moves only with indirect mods.
    #[test]
    fn mindworm_direct_hit_and_dot_modify_independently() {
        let stats = CombatStats {
            // Mindworm's up-front hit is HealthSpecificDamage 369 (no plain `Damage` field), which
            // the engine surfaces as a "health" direct-damage component.
            health_specific_damage: Some(369.0),
            attributes_that_delta_damage: vec!["BOOST_ABILITY_MINDWORM".into()],
            attributes_that_mod_base_damage: vec!["MOD_SKILL_MENTALISM".into()],
            attributes_that_mod_damage: vec!["MOD_ABILITY_MINDWORM".into()],
            dots: vec![DotEffect {
                damage_per_tick: 84.0,
                num_ticks: 4.0,
                duration: Some(8.0),
                damage_type: Some("Psychic".into()),
                attributes_that_delta: vec!["BOOST_ABILITYDOT_MINDWORM".into()],
                ..Default::default()
            }],
            ..Default::default()
        };

        // Direct + base-skill mods move the up-front hit but must NOT touch the DoT.
        // direct = 369×(1 + 1.05 + 1.30) + 202×(1 + 1.30) = 1236.15 + 464.6 = 1700.75.
        let direct_only = vec![
            m("MOD_SKILL_MENTALISM", 1.05),
            m("BOOST_ABILITY_MINDWORM", 202.0),
            m("MOD_ABILITY_MINDWORM", 1.30),
        ];
        let out = compute(&stats, Some("Psychic".into()), &direct_only);
        let dd = out
            .direct_damages
            .iter()
            .find(|d| d.kind == "health")
            .map(|d| &d.value)
            .expect("Mindworm has a health-specific direct hit (from HSD)");
        assert!((dd.base - 369.0).abs() < 1e-6);
        assert!(
            (dd.effective - 1700.75).abs() < 1e-6,
            "got {}",
            dd.effective
        );
        let dot = &out.dots[0];
        assert!(
            (dot.per_tick.effective - 84.0).abs() < 1e-6,
            "direct/base mods must not move the DoT, got {}",
            dot.per_tick.effective
        );
        assert!(out.any_modified);

        // Indirect mods move the DoT but leave the up-front hit at its base.
        // (84 + 84) × (1 + 0.20 + 0.10) = 168 × 1.30 = 218.4/tick → 873.6 total.
        let indirect = vec![
            m("BOOST_ABILITYDOT_MINDWORM", 84.0),
            m("MOD_PSYCHIC_INDIRECT", 0.20),
            m("MOD_UNIVERSAL_INDIRECT", 0.10),
        ];
        let out2 = compute(&stats, Some("Psychic".into()), &indirect);
        let dd2 = out2
            .direct_damages
            .iter()
            .find(|d| d.kind == "health")
            .map(|d| &d.value)
            .unwrap();
        assert!(
            (dd2.effective - 369.0).abs() < 1e-6,
            "indirect mods must not move the direct hit, got {}",
            dd2.effective
        );
        let dot2 = &out2.dots[0];
        assert!(
            (dot2.per_tick.effective - 218.4).abs() < 1e-6,
            "got {}",
            dot2.per_tick.effective
        );
        assert!(
            (dot2.total_effective - 873.6).abs() < 1e-6,
            "got {}",
            dot2.total_effective
        );
        assert!(out2.any_modified);
    }

    /// Acid Spew shape: a normal `Damage` hit plus an `ArmorSpecificDamage` hit. Both surface as
    /// separate direct-damage components and both move with the shared direct-damage mods.
    #[test]
    fn normal_and_armor_specific_are_separate_components() {
        let stats = CombatStats {
            damage: Some(10.0),
            armor_specific_damage: Some(20.0),
            attributes_that_mod_base_damage: vec!["MOD_SKILL_FIRE".into()],
            ..Default::default()
        };
        let out = compute(&stats, Some("Acid".into()), &[m("MOD_SKILL_FIRE", 0.50)]);
        assert_eq!(out.direct_damages.len(), 2);

        let normal = out
            .direct_damages
            .iter()
            .find(|d| d.kind == "normal")
            .unwrap();
        assert!((normal.value.base - 10.0).abs() < 1e-6);
        assert!(
            (normal.value.effective - 15.0).abs() < 1e-6,
            "got {}",
            normal.value.effective
        );

        let armor = out
            .direct_damages
            .iter()
            .find(|d| d.kind == "armor")
            .unwrap();
        assert!((armor.value.base - 20.0).abs() < 1e-6);
        assert!(
            (armor.value.effective - 30.0).abs() < 1e-6,
            "got {}",
            armor.value.effective
        );
        assert!(out.any_modified);
    }

    /// A flat-damage focus item ("Direct Psychic Damage +104") boosts a Psychic ability's direct
    /// hit even though the ability never lists BOOST_PSYCHIC_DIRECT in its own delta array — the
    /// engine adds the generic per-type/universal direct tokens, mirroring the DoT indirect rule.
    #[test]
    fn generic_direct_damage_tokens_apply_by_type() {
        let stats = CombatStats {
            damage: Some(100.0),
            attributes_that_mod_base_damage: vec!["MOD_SKILL_MENTALISM".into()],
            ..Default::default()
        };
        let mods = vec![
            m("BOOST_PSYCHIC_DIRECT", 104.0), // flat, typed
            m("BOOST_UNIVERSAL_DIRECT", 6.0), // flat, all-type
            m("MOD_PSYCHIC_DIRECT", 0.10),    // percent, typed
        ];
        let out = compute(&stats, Some("Psychic".into()), &mods);
        let dd = &out.direct_damages[0].value;
        // added = 104 + 6 = 110; all_pct = 0.10 → 100×1.10 + 110×1.10 = 110 + 121 = 231.
        assert!((dd.added - 110.0).abs() < 1e-6, "added {}", dd.added);
        assert!((dd.effective - 231.0).abs() < 1e-6, "got {}", dd.effective);
        assert!(out.any_modified);
    }

    /// Generic direct tokens of a *different* damage type must not touch the ability.
    #[test]
    fn generic_direct_tokens_respect_damage_type() {
        let stats = CombatStats {
            damage: Some(100.0),
            ..Default::default()
        };
        let out = compute(
            &stats,
            Some("Fire".into()),
            &[m("BOOST_PSYCHIC_DIRECT", 50.0)],
        );
        let dd = &out.direct_damages[0].value;
        assert!((dd.effective - 100.0).abs() < 1e-6, "got {}", dd.effective);
        assert!(!dd.is_modified());
        assert!(!out.any_modified);
    }

    /// The item's per-tick indirect bonus (BOOST_PSYCHIC_INDIRECT +21) feeds a Psychic DoT via
    /// the existing indirect-token rule — the matching half of the focus-item fix.
    #[test]
    fn generic_indirect_flat_token_feeds_dot() {
        let stats = CombatStats {
            dots: vec![DotEffect {
                damage_per_tick: 80.0,
                num_ticks: 4.0,
                damage_type: Some("Psychic".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let out = compute(
            &stats,
            Some("Psychic".into()),
            &[m("BOOST_PSYCHIC_INDIRECT", 21.0)],
        );
        let dot = &out.dots[0];
        assert!(
            (dot.per_tick.effective - 101.0).abs() < 1e-6,
            "got {}",
            dot.per_tick.effective
        );
    }

    /// Dual-scope "X Damage (Direct & Per-Tick)" tokens ({MOD_ELECTRICITY}, {BOOST_FIRE}, …)
    /// move BOTH the direct hit (by ability damage type) and each DoT tick (by DoT damage type).
    #[test]
    fn dual_scope_type_tokens_hit_direct_and_ticks() {
        let stats = CombatStats {
            damage: Some(100.0),
            dots: vec![DotEffect {
                damage_per_tick: 20.0,
                num_ticks: 4.0,
                damage_type: Some("Fire".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mods = vec![m("MOD_FIRE", 0.10), m("BOOST_FIRE", 2.0)];
        let out = compute(&stats, Some("Fire".into()), &mods);
        // direct: 100×1.10 + 2×1.10 = 112.2
        let dd = &out.direct_damages[0].value;
        assert!((dd.effective - 112.2).abs() < 1e-6, "got {}", dd.effective);
        // per tick: (20 + 2) × 1.10 = 24.2
        let dot = &out.dots[0];
        assert!(
            (dot.per_tick.effective - 24.2).abs() < 1e-6,
            "got {}",
            dot.per_tick.effective
        );
        assert!(out.any_modified);
    }

    /// Dual-scope tokens of a different type must not apply to either component.
    #[test]
    fn dual_scope_type_tokens_respect_type() {
        let stats = CombatStats {
            damage: Some(100.0),
            dots: vec![DotEffect {
                damage_per_tick: 20.0,
                num_ticks: 4.0,
                damage_type: Some("Poison".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let out = compute(&stats, Some("Crushing".into()), &[m("MOD_FIRE", 0.50)]);
        assert!((out.direct_damages[0].value.effective - 100.0).abs() < 1e-6);
        assert!((out.dots[0].per_tick.effective - 20.0).abs() < 1e-6);
        assert!(!out.any_modified);
    }

    /// Power cost reduction: base 16, -4 delta → 12.
    #[test]
    fn power_cost_delta_reduces() {
        let stats = CombatStats {
            power_cost: Some(16.0),
            attributes_that_delta_power_cost: vec!["DELTA_POWER_COST_X".into()],
            ..Default::default()
        };
        let out = compute(&stats, None, &[m("DELTA_POWER_COST_X", -4.0)]);
        let pc = out.power_cost.unwrap();
        assert!((pc.effective - 12.0).abs() < 1e-6);
    }
}
