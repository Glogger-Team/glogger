use crate::parsers::parse_timestamp;
/// Player log event parser — foundational module for parsing all ProcessXxx events
/// from Player.log into structured, identity-resolved events.
///
/// This is a core system that features subscribe to. It maintains:
/// - Instance registry: maps instance IDs to item names/type IDs
/// - Stack tracking: current stack size per instance
/// - Interaction context: current NPC interaction for contextualizing events
/// - Pending delete buffer: 1-line lookahead to disambiguate storage/vendor/consumed
use std::collections::HashMap;

// ============================================================
// Event Types
// ============================================================

#[derive(serde::Serialize, Clone, Debug)]
#[serde(tag = "kind")]
pub enum PlayerEvent {
    // === Item Events ===
    ItemAdded {
        timestamp: String,
        item_name: String,
        instance_id: u64,
        slot_index: i32,
        is_new: bool,
        /// Initial stack quantity. When a matching chat [Status] gain was in
        /// the buffer, this is the chat's authoritative quantity (e.g., 3 for
        /// "Fluorite x3 added to inventory"). Otherwise defaults to 1.
        /// Downstream consumers (GameStateManager, test harness) should use
        /// this as the transaction quantity instead of hardcoding 1.
        initial_quantity: u32,
        /// Source attribution derived from the parser's ActivityContext stack
        /// at the time of this event. See `ItemProvenance` docs.
        provenance: ItemProvenance,
    },
    ItemStackChanged {
        timestamp: String,
        instance_id: u64,
        item_name: Option<String>,
        item_type_id: u16,
        old_stack_size: u32,
        new_stack_size: u32,
        delta: i32,
        from_server: bool,
        provenance: ItemProvenance,
    },
    ItemDeleted {
        timestamp: String,
        instance_id: u64,
        item_name: Option<String>,
        context: DeleteContext,
    },

    // === Skill Events ===
    SkillsLoaded {
        timestamp: String,
        skills: Vec<SkillSnapshot>,
    },

    // === NPC Events ===
    InteractionStarted {
        timestamp: String,
        entity_id: u32,
        interaction_type: u32,
        npc_name: String,
    },
    /// Another player's stall UI opened (is_own_stall False) or the player's
    /// own stall management screen (True). Emitted only for others' stalls —
    /// own-stall opens are already covered by PlayerShopLog books.
    VendorScreenOpened {
        timestamp: String,
        npc_entity_id: u32,
        is_own_stall: bool,
    },
    /// Purchase-or-collect from a vendor screen: pairs with the ItemAdded
    /// that follows (item/quantity live there; this supplies the stall).
    VendorItemTaken {
        timestamp: String,
        npc_entity_id: u32,
        instance_id: u64,
        is_own_stall: bool,
    },
    FavorChanged {
        timestamp: String,
        npc_id: u32,
        npc_name: String,
        delta: f32,
        is_gift: bool,
    },
    // The game's authoritative weekly gift count for an NPC, parsed from the
    // "<Npc> will accept up to <b>N</b> gifts per calendar week and has received
    // <b>M</b> so far this week" note shown by gift-capped (Statehelm) NPCs.
    // Used to reconcile the gift log — bulk-gifting a stack emits a single
    // ProcessDeltaFavor line, so counting favor events alone undercounts.
    GiftCountObserved {
        timestamp: String,
        npc_name: String,
        received: u32,
        max: u32,
    },

    /// The `Player.log` login line's own declared UTC instant
    /// (`Logged in as character X. Time UTC=MM/DD/YYYY HH:MM:SS.`). Anchors the
    /// date for every `HH:MM:SS` timestamp that follows, so a catch-up that lags
    /// the log by more than a day still dates lines by the session they came from.
    LogSessionAnchored {
        utc: chrono::NaiveDateTime,
    },

    // === Vendor Events ===
    VendorSold {
        timestamp: String,
        price: u32,
        item_name: String,
        instance_id: u64,
        is_buyback: bool,
    },
    VendorStackUpdated {
        timestamp: String,
        instance_id: u64,
        item_type_id: u16,
        new_stack_size: u32,
        price: u32,
    },

    // === Storage Events ===
    StorageDeposit {
        timestamp: String,
        npc_id: u32,
        vault_key: Option<String>,
        slot: i32,
        item_name: String,
        instance_id: u64,
    },
    StorageWithdrawal {
        timestamp: String,
        npc_id: u32,
        vault_key: Option<String>,
        instance_id: u64,
        quantity: u32,
        provenance: ItemProvenance,
    },

    // === Action Events ===
    DelayLoopStarted {
        timestamp: String,
        duration: f32,
        action_type: String,
        label: String,
        entity_id: u32,
        abort_condition: String,
    },

    // === Screen/Book Events ===
    ScreenText {
        timestamp: String,
        category: String,
        message: String,
    },
    BookOpened {
        timestamp: String,
        title: String,
        content: String,
        book_type: String,
    },

    // === Interaction Events ===
    InteractionEnded {
        timestamp: String,
        entity_id: i32,
    },

    // === Skill Bar Events ===
    ActiveSkillsChanged {
        timestamp: String,
        skill1: String,
        skill2: String,
    },

    // === Mount Events ===
    MountStateChanged {
        timestamp: String,
        entity_id: u32,
        is_mounting: bool,
    },

    // === Weather Events ===
    WeatherChanged {
        timestamp: String,
        weather_name: String,
        is_active: bool,
    },

    // === Recipe Events ===
    RecipeUpdated {
        timestamp: String,
        recipe_id: u32,
        completion_count: u32,
    },

    // === Combat Events ===
    CombatStateChanged {
        timestamp: String,
        in_combat: bool,
    },

    // === Vendor Gold Events ===
    VendorGoldChanged {
        timestamp: String,
        current_gold: u32,
        server_id: u64,
        max_gold: u32,
    },

    // === Attribute Events ===
    AttributesChanged {
        timestamp: String,
        entity_id: u32,
        attributes: Vec<AttributeValue>,
    },

    // === Login Snapshot Events ===
    AbilitiesLoaded {
        timestamp: String,
        skill1: String,
        skill2: String,
    },
    RecipesLoaded {
        timestamp: String,
    },
    EquipmentChanged {
        timestamp: String,
        entity_id: u32,
        appearance: String,
        equipment: Vec<EquipmentSlot>,
    },

    // === Effect Events ===
    EffectsAdded {
        timestamp: String,
        entity_id: u32,
        source_entity_id: u32,
        effect_ids: Vec<u32>,
        is_login_batch: bool,
    },
    // === Moon Phase Events ===
    MoonPhaseChanged {
        timestamp: String,
        phase: String,
    },

    // === Guild Events ===
    GuildInfoLoaded {
        timestamp: String,
        guild_id: u32,
        guild_name: String,
        motd: String,
    },

    // === Directed Goals Events ===
    DirectedGoalsLoaded {
        timestamp: String,
        goal_ids: Vec<u32>,
    },

    // === Player String Events ===
    PlayerStringUpdated {
        timestamp: String,
        key: String,
        value: String,
    },

    /// Signal-only: ProcessRemoveEffects prints opaque System.Int32[] so we can't
    /// extract which IDs were removed. We still emit the event so consumers know
    /// *something* changed.
    EffectsRemoved {
        timestamp: String,
        entity_id: u32,
    },
    EffectNameUpdated {
        timestamp: String,
        entity_id: u32,
        effect_instance_id: u32,
        display_name: String,
    },
    /// ProcessUpdateDescription — entity state change (garden plants, crafting items with timers, etc.)
    EntityDescriptionUpdated {
        timestamp: String,
        entity_id: u32,
        name: String,
        description: String,
        action: String,
        action_type: String,
        appearance: String,
        flags: u32,
    },

    // === Player Message Events ===
    /// Pigeon or stall-note message (sent or received).
    PlayerMessage {
        timestamp: String,
        /// "pigeon" or "stall_note"
        message_type: String,
        /// "sent" or "received"
        direction: String,
        /// The other player involved (sender if received, recipient if sent)
        other_player: String,
        /// Message body text
        body: String,
        /// Attached item name (pigeons only)
        item_name: Option<String>,
    },

    /// A poem recited by another player, captured from a `Poem by X` NPC talk
    /// screen. Stored globally (not per-character) in the `poems` table.
    PoemRecorded {
        timestamp: String,
        /// The player who wrote/recited the poem (from the `Poem by X` title).
        author: String,
        /// The poem's bold name/title (may be empty if the game omits it).
        title: String,
        /// The poem stanzas, with the intro/outro review blurbs stripped and
        /// the log's `\n` escapes converted to real newlines.
        content: String,
    },

    // === Loot Events ===
    /// Fired when a corpse's loot window is opened (`Search Corpse of X`).
    /// This is the drop-rate *denominator*: it marks a lootable kill even if
    /// the corpse yielded nothing. Corpses the player has no permission to loot
    /// (`has_permission = false`) are reported so the consumer can exclude them.
    CorpseSearched {
        timestamp: String,
        /// Entity ID of the corpse — links to LootPickedUp.corpse_entity_id.
        corpse_entity_id: u32,
        /// Enemy/corpse name (e.g. "Ratkin Miner").
        corpse_name: String,
        /// False when the screen carried "(You do not have permission to loot
        /// this corpse.)" — not the player's kill, can't observe its drops.
        has_permission: bool,
    },
    /// Fired when an item is picked up from a corpse's loot window.
    /// Only actual drops produce this — skinning/butchering rewards do not.
    LootPickedUp {
        timestamp: String,
        instance_id: u64,
        /// Entity ID of the corpse being searched, if a CorpseSearch context is active
        corpse_entity_id: Option<u32>,
        /// Name of the corpse (e.g., "Bear Groupie"), if a CorpseSearch context is active
        corpse_name: Option<String>,
        /// Resolved item name (from instance_registry or preceding UpdateItemCode)
        item_name: Option<String>,
        /// Resolved item type ID (from preceding UpdateItemCode encoding)
        item_type_id: Option<u16>,
        /// Quantity picked up (from preceding AddItem initial_quantity or UpdateItemCode delta)
        quantity: u32,
    },
    /// Fired when an item is extracted from a corpse via the Butchering or
    /// Skinning skill. These grant skill XP and produce NO ProcessRemoveLoot,
    /// so they are tracked as a category separate from loot-table drops.
    CorpseExtract {
        timestamp: String,
        item_name: String,
        item_type_id: Option<u16>,
        quantity: u32,
        /// "Butchering" or "Skinning"
        skill: String,
        /// Name of the corpse, if a CorpseSearch context is active
        corpse_name: Option<String>,
        /// Player's level in `skill` (Butchering/Skinning) at harvest time.
        skill_level: Option<u32>,
        /// Equipment skill bonus from the corpse line
        /// "(with a +N skill bonus from equipment)".
        equipment_bonus: Option<u32>,
        /// Anatomy family for this monster type (e.g. "Canines"), from the
        /// co-occurring `Anatomy_<Family>` skill update.
        anatomy_family: Option<String>,
        /// Player's anatomy level for that family at harvest time.
        anatomy_level: Option<u32>,
    },
}

#[derive(serde::Serialize, Clone, Debug, PartialEq)]
pub enum DeleteContext {
    StorageTransfer,
    VendorSale,
    #[allow(dead_code)]
    Consumed,
    Unknown,
}

/// Where did this item come from? Attached to every inventory gain event
/// (ItemAdded, ItemStackChanged, StorageWithdrawal) so downstream features
/// can aggregate by source (mining yield stats, survey loot tracking, etc.).
///
/// Attribution is best-effort: when the parser sees a clean match to exactly
/// one active `ActivityContext` at the event's timestamp, it emits
/// `Attributed`. When multiple contexts are active and none can be
/// confidently preferred, it emits `Uncertain` with the candidate list so
/// downstream code can decide whether/how to classify. When no context is
/// active, `UnknownSource` is used — this bucket is expected to be sizeable
/// because Player.log does not mark every gain with a surrounding activity.
#[derive(serde::Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind")]
pub enum ItemProvenance {
    Attributed {
        source: ActivitySource,
        confidence: AttributionConfidence,
        /// A3 stitching link populated by feature aggregators (currently only
        /// the survey tracker). When set, this gain belongs to the named
        /// `survey_uses` row and downstream queries can group by it via
        /// `item_transactions.source_details->>'survey_use_id'`. The parser
        /// itself never sets this — it stays `None` until an aggregator that
        /// cares mutates it before the event reaches game-state persistence.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        survey_use_id: Option<i64>,
    },
    Uncertain {
        candidates: Vec<ActivitySource>,
    },
    UnknownSource,
    /// Internal "don't attribute" signal — used for events that represent
    /// state reconciliation rather than real inventory gains (e.g., the
    /// first UpdateItemCode on a session-loaded item, session-load AddItems).
    /// Serialized as `{kind: "NotApplicable"}` so consumers can filter these
    /// out of gain-based aggregates.
    NotApplicable,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq)]
pub enum AttributionConfidence {
    /// Exactly one active context, emitted immediately.
    Confident,
    /// Multiple contexts active but one was very recently opened relative to
    /// the gain, or the best candidate was preferred by a tie-breaker.
    #[allow(dead_code)] // populated by future heuristics; no tie-breaker rules yet
    Probable,
    /// Fallback when we pick a best guess without strong signal.
    #[allow(dead_code)] // reserved for future item-type-to-loot-table matching
    Weak,
}

/// Database-ready representation of an `ItemProvenance`: three values that
/// populate the `source_kind`, `source_details`, and `confidence` columns on
/// `item_transactions`. All three may be `None` depending on the provenance
/// variant (see column documentation in migration v25).
pub struct ProvenanceColumns {
    pub source_kind: Option<String>,
    pub source_details: Option<String>,
    pub confidence: Option<String>,
}

impl ItemProvenance {
    /// Project this provenance into the three DB columns.
    ///
    /// Taxonomy for `source_kind`:
    /// - `"mining"`, `"survey_map_use"`, `"survey_map_craft"`,
    ///   `"general_craft"`, `"corpse_search"`, `"vendor_browsing"`,
    ///   `"storage_browsing"` — from `Attributed` variants (source kind
    ///   determines the string).
    /// - `"uncertain"` — multiple candidates, preserved in `source_details`.
    /// - `"unknown"` — `UnknownSource`.
    /// - `"not_applicable"` — session-load reloads, consumption/losses. Gain
    ///   aggregates should filter rows where source_kind = 'not_applicable'.
    ///
    /// `source_details` is a JSON blob with source-kind-specific fields
    /// (node_name, npc_name, etc.) — present whenever the source carries
    /// additional identifying info worth preserving.
    pub fn to_columns(&self) -> ProvenanceColumns {
        match self {
            ItemProvenance::Attributed {
                source,
                confidence,
                survey_use_id,
            } => {
                let (kind, details) = activity_source_to_kind_and_details(source);
                // If a survey_use_id is attached, splice it into the existing
                // details JSON (or create a fresh one). This is the A3
                // stitching link — `item_transactions.source_details->>'survey_use_id'`
                // is the grouping key for "all loot from this survey use".
                let details = match (details, survey_use_id) {
                    (Some(json), Some(use_id)) => Some(inject_survey_use_id(&json, *use_id)),
                    (None, Some(use_id)) => Some(format!(r#"{{"survey_use_id":{use_id}}}"#)),
                    (other, None) => other,
                };
                ProvenanceColumns {
                    source_kind: Some(kind.to_string()),
                    source_details: details,
                    confidence: Some(confidence_to_string(confidence).to_string()),
                }
            }
            ItemProvenance::Uncertain { candidates } => {
                // Serialize the full candidate list as JSON so downstream
                // classification/reclassification logic can access it later.
                let details = serde_json::to_string(candidates).ok();
                ProvenanceColumns {
                    source_kind: Some("uncertain".to_string()),
                    source_details: details,
                    confidence: None,
                }
            }
            ItemProvenance::UnknownSource => ProvenanceColumns {
                source_kind: Some("unknown".to_string()),
                source_details: None,
                confidence: None,
            },
            ItemProvenance::NotApplicable => ProvenanceColumns {
                source_kind: Some("not_applicable".to_string()),
                source_details: None,
                confidence: None,
            },
        }
    }
}

fn confidence_to_string(c: &AttributionConfidence) -> &'static str {
    match c {
        AttributionConfidence::Confident => "confident",
        AttributionConfidence::Probable => "probable",
        AttributionConfidence::Weak => "weak",
    }
}

/// Splice a `survey_use_id` field into an existing source-details JSON object.
///
/// Falls back to a minimal object containing only the survey_use_id if the
/// input isn't valid JSON (defensive — shouldn't happen since we control the
/// upstream serializer, but keeps a malformed details blob from corrupting
/// the stitching link).
fn inject_survey_use_id(details_json: &str, survey_use_id: i64) -> String {
    match serde_json::from_str::<serde_json::Value>(details_json) {
        Ok(serde_json::Value::Object(mut map)) => {
            map.insert(
                "survey_use_id".to_string(),
                serde_json::Value::from(survey_use_id),
            );
            serde_json::Value::Object(map).to_string()
        }
        _ => format!(r#"{{"survey_use_id":{survey_use_id}}}"#),
    }
}

/// Project an ActivitySource into a stable string kind and optional JSON
/// details blob. The JSON is flat and only includes fields that carry
/// identifying info (node names, NPC names, etc.).
fn activity_source_to_kind_and_details(source: &ActivitySource) -> (&'static str, Option<String>) {
    match source {
        ActivitySource::Mining {
            node_entity_id,
            node_name,
        } => {
            let details = serde_json::json!({
                "node_entity_id": node_entity_id,
                "node_name": node_name,
            });
            ("mining", Some(details.to_string()))
        }
        ActivitySource::SurveyMapUse {
            survey_map_internal_name,
        } => {
            let details = serde_json::json!({
                "survey_map_internal_name": survey_map_internal_name,
            });
            ("survey_map_use", Some(details.to_string()))
        }
        ActivitySource::SurveyMapCraft => ("survey_map_craft", None),
        ActivitySource::GeneralCraft { action_type, label } => {
            let details = serde_json::json!({
                "action_type": action_type,
                "label": label,
            });
            ("general_craft", Some(details.to_string()))
        }
        ActivitySource::CorpseSearch {
            entity_id,
            corpse_name,
        } => {
            let details = serde_json::json!({
                "entity_id": entity_id,
                "corpse_name": corpse_name,
            });
            ("corpse_search", Some(details.to_string()))
        }
        ActivitySource::VendorBrowsing {
            npc_entity_id,
            npc_name,
        } => {
            let details = serde_json::json!({
                "npc_entity_id": npc_entity_id,
                "npc_name": npc_name,
            });
            ("vendor_browsing", Some(details.to_string()))
        }
        ActivitySource::StorageBrowsing {
            vault_owner_entity_id,
            vault_name,
        } => {
            let details = serde_json::json!({
                "vault_owner_entity_id": vault_owner_entity_id,
                "vault_name": vault_name,
            });
            ("storage_browsing", Some(details.to_string()))
        }
    }
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct SkillSnapshot {
    pub skill_type: String,
    pub raw: u32,
    pub bonus: u32,
    pub xp: u32,
    pub tnl: i32,
    pub max: u32,
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct AttributeValue {
    pub name: String,
    pub value: f32,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq)]
pub struct EquipmentSlot {
    pub slot: String,
    pub appearance_key: String,
}

// ============================================================
// Internal State Types
// ============================================================

#[derive(Clone, Debug)]
struct InstanceInfo {
    item_name: String,
    item_type_id: Option<u16>,
}

#[derive(Clone, Debug)]
struct PendingDelete {
    timestamp: String,
    instance_id: u64,
    item_name: Option<String>,
    /// Seconds-of-day when the DeleteItem fired (for the 5 s retention).
    seen_at_secs: u32,
    /// Non-matching lines observed while buffered (for the 5-line retention).
    lines_seen: u32,
    /// Wall-clock instant the DeleteItem fired. Backstop for live-tail
    /// silence: when polls arrive with no new lines, log-clock and
    /// line-count retention both freeze, so the idle drain evicts entries
    /// older than `DELETE_BUFFER_IDLE_TIMEOUT_SECS` as `Unknown`.
    seen_at_instant: std::time::Instant,
}

/// Delete-buffer retention: a `ProcessDeleteItem` waits up to 5 s / 5 lines
/// for its `ProcessAddToStorageVault` / vendor confirmation before it is
/// emitted as `Unknown`. Next-line flush misclassified vault deposits with
/// an unrelated line in between; the game emits the pair within a tick.
const DELETE_BUFFER_MAX_AGE_SECS: u32 = 5;
const DELETE_BUFFER_MAX_LINES: u32 = 5;
/// Wall-clock backstop for the delete buffer during live-tail silence. When
/// polls arrive with no new log lines, the 5 s / 5-line retention never
/// advances; a delete older than this is emitted as `Unknown` instead of
/// lingering forever. Generous vs the 500 ms poll cadence and the
/// game's within-a-tick Delete/confirmation pairing.
const DELETE_BUFFER_IDLE_TIMEOUT_SECS: u64 = 30;

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct InteractionContext {
    entity_id: u32,
    npc_name: String,
    interaction_type: u32,
}

// ============================================================
// Activity Context — source attribution for item gains
// ============================================================
//
// Reconstructs what the player is currently *doing* from the log line stream.
// Each active context describes a potential source of inventory gains (mining
// a node, using a survey map, searching a corpse, browsing a vendor, etc.).
//
// Contexts are opened by reliable signals — primarily `ProcessDoDelayLoop`
// action_type + label, or screen-specific events like `ProcessTalkScreen`
// ("Search Corpse of X", …, Corpse). Contexts close on an explicit end signal
// (`ProcessEndInteraction`) or a timeout derived from the delay-loop duration.
//
// Signals that are *unreliable* across game versions and player environments —
// such as `ProcessStartInteraction` names (frequently empty for motherlode
// nodes) and `ProcessFirstEverInteraction` (fires inconsistently) — are only
// used as enrichment when present. Attribution must work even when they are
// absent.

/// Discriminated source of an inventory gain, reconstructed from player.log context.
#[derive(serde::Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind")]
pub enum ActivitySource {
    /// Mining at a world node (motherlode or plain). node_name is present only
    /// when the game emitted it; many empty `StartInteraction` names occur in
    /// real logs and we must not rely on this being populated.
    Mining {
        node_entity_id: Option<u32>,
        node_name: Option<String>,
    },
    /// Using a survey map (primary source for survey loot).
    SurveyMapUse {
        survey_map_internal_name: Option<String>,
    },
    /// Crafting a new survey map via the Surveying skill.
    SurveyMapCraft,
    /// Other crafting/use actions detected via DoDelayLoop (cooking, brewing,
    /// distilling, using food, etc.).
    GeneralCraft { action_type: String, label: String },
    /// Actively searching a corpse (opened via TalkScreen "Search Corpse of X").
    CorpseSearch { entity_id: u32, corpse_name: String },
    /// Vendor trade window open.
    VendorBrowsing {
        npc_entity_id: u32,
        npc_name: Option<String>,
    },
    /// Storage vault UI open (saddlebag, housing, NPC account storage, etc.).
    StorageBrowsing {
        vault_owner_entity_id: u32,
        vault_name: String,
    },
}

#[derive(Clone, Debug)]
#[allow(dead_code)] // started_at*/close_deadline_secs are read by tests and future Phase 2 attribution
pub struct ActivityContext {
    pub source: ActivitySource,
    /// Timestamp the context was opened (HH:MM:SS from player.log).
    pub started_at: String,
    /// Seconds since start-of-day for the started_at timestamp — cached so we
    /// don't re-parse on every expire check.
    pub started_at_secs: u32,
    /// Seconds after `started_at_secs` at which this context is considered
    /// stale and closed on the next line. Computed from the DoDelayLoop
    /// duration (+ slack) when available; otherwise a type-specific default.
    pub close_deadline_secs: u32,
    /// entity_id used to match an explicit `EndInteraction` close signal,
    /// when applicable. Not all sources are interaction-backed.
    pub entity_id: Option<u32>,
}

/// Slack added to a delay-loop duration before timing out a context.
/// Gives the server a moment to emit the resulting item events after the loop
/// completes, without letting the context drift indefinitely.
const DELAY_LOOP_SLACK_SECS: u32 = 2;

/// Default lifetime for contexts that don't carry an explicit duration
/// (corpse search, vendor/storage browsing). Conservative upper bound; real
/// activity typically closes via an explicit signal sooner.
const DEFAULT_CONTEXT_LIFETIME_SECS: u32 = 30;

/// Max seconds between a corpse's `Search Corpse of X` loot window and a
/// `ProcessRemoveLoot` for that pickup to still be filed under the corpse.
///
/// PG re-emits the corpse's `Search Corpse of X` TalkScreen in the *same* log
/// tick as every item taken off it, so genuine corpse loot fires its RemoveLoot
/// 0s after the search (auto-loot) or up to ~2s later (a slow manual first-grab)
/// — verified across 59 real corpse-loot events, max observed gap 2s.
///
/// Gathered/ground loot (Xogrite Chunks and other node pickups) also fires
/// `ProcessRemoveLoot` but has **no** corpse window of its own. Without this
/// gate it borrowed whatever corpse-search context was still inside the 30s
/// `DEFAULT_CONTEXT_LIFETIME_SECS`, mis-filing e.g. Xogrite Chunks under a
/// monster killed seconds earlier. Requiring the corpse search to be *fresh*
/// keeps ground loot out while comfortably covering real corpse loot.
const CORPSE_LOOT_MAX_SEARCH_AGE_SECS: u32 = 3;

/// Parse a Project: Gorgon item instance ID.
///
/// The game logs item instance IDs as **signed 32-bit** integers, so they are
/// frequently negative (e.g. `ProcessDeleteItem(-1796085135)`). We store them
/// as `u64` via a bit-preserving `as` cast so the same source string always
/// maps to the same registry key across Add/Delete/UpdateItemCode lines.
///
/// Parsing as `u64` directly (the historical behavior) failed on every
/// negative ID, causing the parser to silently drop the entire line — which
/// lost roughly half of all item add/delete/loot events (those whose hashed
/// instance ID happened to have the sign bit set). That in turn made survey
/// tracking, which keys off these events, fire only intermittently.
fn parse_instance_id(s: &str) -> Option<u64> {
    s.trim().parse::<i64>().ok().map(|v| v as u64)
}

/// Convert "HH:MM:SS" to seconds-of-day. Returns 0 on malformed input — safe
/// because mis-parsed contexts will expire immediately.
fn timestamp_to_secs(ts: &str) -> u32 {
    let mut parts = ts.split(':');
    let h: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let m: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let s: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    h * 3600 + m * 60 + s
}

/// Pending interaction metadata captured from `ProcessStartInteraction` so it
/// can enrich a subsequent screen-specific event (VendorScreen, ShowStorageVault,
/// TalkScreen corpse) before being consumed.
#[derive(Clone, Debug)]
struct PendingInteractionMeta {
    entity_id: u32,
    /// NPC name from StartInteraction, if non-empty. Often empty for
    /// non-NPC interactions (mining nodes, portals, containers).
    name: Option<String>,
    captured_at_secs: u32,
}

/// A recent chat `[Status] X added to inventory` gain, held briefly so the
/// parser can seed `ProcessAddItem`'s stack from the chat quantity instead of
/// the fallback 1.
///
/// Matching is by resolved internal item name (display-name-to-internal
/// resolution happens in the coordinator before the event is pushed into the
/// parser, since the parser has no CDN access). Entries are consumed on a
/// first-match basis to handle the multi-stack case (a single pickup split
/// across inventory stacks fires two separate chat lines and two separate
/// AddItems).
#[derive(Clone, Debug)]
struct PendingChatGain {
    item_internal_name: String,
    quantity: u32,
    captured_at_secs: u32,
}

/// Tracks a pigeon send in progress. The confirmation TalkScreen contains
/// the recipient, item, and message before the final "The pigeon is sent!"
/// confirmation.
#[derive(Clone, Debug)]
struct PendingPigeonSend {
    recipient: String,
    message: String,
    item_name: Option<String>,
}

/// Tracks a stall note send in progress. The ProcessInputBox title contains
/// "Send Note to <player>". The actual note text is entered by the user but
/// not logged, so we just track the recipient.
#[derive(Clone, Debug)]
struct PendingStallNoteSend {
    recipient: String,
}

/// Window (seconds) either side of a `ProcessAddItem` timestamp within which a
/// chat gain entry is considered a match. Chat and player.log flush
/// independently, so the chat message can arrive slightly before or after the
/// corresponding player.log line.
const CHAT_GAIN_MATCH_WINDOW_SECS: u32 = 2;

/// Maximum lifetime of an unmatched chat gain before it's discarded.
/// Longer than the match window so a chat gain that arrives slightly early
/// still has its full window available to find a match.
const CHAT_GAIN_BUFFER_LIFETIME_SECS: u32 = 10;

/// Consecutive `is_new=false` AddItem burst length that marks a login
/// full-state dump (mirrors effects' `is_login_batch`). Live play never
/// produces 20 back-to-back `is_new=false` AddItems with no intervening
/// UpdateItemCode; a login dump always does. One-shot per burst: the
/// `login_batch_fired` latch is set when the flag fires and only cleared
/// where the run resets, so the flag can't re-arm mid-burst.
const LOGIN_BATCH_MIN_ITEMS: u32 = 20;

// ============================================================
// Parser
// ============================================================

pub struct PlayerEventParser {
    instance_registry: HashMap<u64, InstanceInfo>,
    stack_sizes: HashMap<u64, u32>,
    current_interaction: Option<InteractionContext>,
    pending_deletes: Vec<PendingDelete>,
    /// Stack of currently-active activity contexts. Most-recently-pushed is
    /// last; attribution prefers the most-specific/most-recent match.
    activity_contexts: Vec<ActivityContext>,
    /// Last-seen `ProcessStartInteraction` metadata, used to enrich the next
    /// screen-specific event. Cleared when consumed or after a short window.
    pending_interaction: Option<PendingInteractionMeta>,
    /// Buffer of chat-log `[Status] ItemGained` events awaiting correlation
    /// with an upcoming `ProcessAddItem`. Entries are consumed on first match
    /// and age out after `CHAT_GAIN_BUFFER_LIFETIME_SECS`.
    pending_chat_gains: Vec<PendingChatGain>,

    /// Inventory-accuracy instrumentation: how many `ProcessAddItem`
    /// `(is_new=true, slot<0)` seeds came from a matched chat gain vs the
    /// `unwrap_or(1)` fallback. Read via `chat_gain_counts_for_test`;
    /// surfaced through the existing debug/dump path only (no new command).
    chat_gain_matched: u64,
    chat_gain_fallback: u64,

    /// Consecutive `ProcessAddItem(is_new=false)` count since the last
    /// `CharacterLogin` or intervening `ProcessUpdateItemCode` — the
    /// login-batch full-state dump signal, mirroring effects'
    /// `is_login_batch`. Fires the one-shot `inventory_login_batch` flag at
    /// `LOGIN_BATCH_MIN_ITEMS`; reset on UpdateItemCode / login line /
    /// genuine (`is_new=true`) gain.
    login_batch_run: u32,
    /// One-shot: the current burst crossed the login-batch threshold and the
    /// inventory slice needs one clear before the batch persists. Consumed by
    /// `take_inventory_login_batch`.
    inventory_login_batch: bool,
    /// Per-burst latch: set when the one-shot fires, cleared only where the
    /// run resets. Stops the flag re-arming mid-burst — without it, the line
    /// after a take would re-set the flag and the coordinator would clear
    /// the slice a second time, wiping the already-persisted first half.
    login_batch_fired: bool,
    /// Seconds-of-day of the most recent timestamped line seen. Feeds the
    /// poll-boundary drain (`flush_expired_deletes`): the watcher asks for
    /// the poll's last clock after feeding all lines, so cross-poll pairs
    /// keep their 5 s window instead of flushing every 500 ms poll.
    last_line_secs: u32,
    /// State for tracking an in-progress pigeon send (multi-step NPC flow).
    /// Set when we see the confirmation TalkScreen with item/message details,
    /// consumed when "The pigeon is sent!" confirmation appears.
    pending_pigeon_send: Option<PendingPigeonSend>,

    /// State for tracking an in-progress stall note send.
    /// Set when we see ProcessInputBox with "Send Note to <player>",
    /// consumed when "Note Sent" confirmation appears.
    pending_stall_note_send: Option<PendingStallNoteSend>,

    /// Current vendor screen: (npc_entity_id, is_own_stall). Set by
    /// ProcessPlayerVendorScreen (arg layout: npcId, helpText, listType,
    /// slotCount, isManager, ...); consumed by ProcessPlayerVendorScreenRemove;
    /// cleared by ProcessEndInteraction. None when no screen is open.
    vendor_screen: Option<(u32, bool)>,

    /// Last item event emitted in this process_line call. Used by parse_remove_loot
    /// to identify the item for stacking cases where RemoveLoot has an orphaned instance_id.
    last_item_event: Option<LastItemEvent>,

    /// Running skill table (skill_type → (raw level, equipment/buff bonus)),
    /// maintained from ProcessLoadSkills + ProcessUpdateSkill. Lets a corpse
    /// extract report the player's Skinning/Butchering level at harvest time.
    skill_levels: HashMap<String, (u32, u32)>,

    /// Most recent `Anatomy_<Family>` skill update — butchering/skinning a corpse
    /// grants that family's anatomy XP, so this is the anatomy skill (and level)
    /// for the monster type just processed.
    last_anatomy: Option<AnatomyObservation>,

    /// Equipment skill bonus parsed from the most recent corpse-search body
    /// (`skinned/butchered the corpse (with a +N skill bonus from equipment)`),
    /// attached to the extract that follows.
    pending_extract_equip_bonus: Option<u32>,

    /// The most recent `Search Corpse of X` loot window and when it opened.
    /// A `ProcessRemoveLoot` is attributed to this corpse only if the search is
    /// still fresh (`CORPSE_LOOT_MAX_SEARCH_AGE_SECS`) — this is what keeps
    /// gathered/ground loot (e.g. Xogrite Chunks) from being mis-filed under a
    /// monster killed moments before. Cleared when the corpse interaction ends.
    last_corpse_search: Option<CorpseSearchMark>,
}

/// The most recent corpse loot window, used to attribute `ProcessRemoveLoot`
/// pickups. PG re-emits the corpse's `Search Corpse of X` TalkScreen in the same
/// tick as each item taken off it, so a fresh mark identifies the corpse a
/// RemoveLoot belongs to.
#[derive(Clone, Debug)]
struct CorpseSearchMark {
    entity_id: u32,
    corpse_name: String,
    /// Seconds-of-day the search TalkScreen fired (from the log timestamp).
    at_secs: u32,
}

/// A captured `Anatomy_<Family>` skill reading (e.g. family "Canines", raw 44).
#[derive(Clone, Debug)]
struct AnatomyObservation {
    family: String,
    raw: u32,
}

/// Tracks the most recent item gain event within a single process_line call.
/// This allows ProcessRemoveLoot to resolve the item it refers to.
#[derive(Clone)]
struct LastItemEvent {
    instance_id: u64,
    item_name: String,
    item_type_id: Option<u16>,
    quantity: u32,
}

impl PlayerEventParser {
    pub fn new() -> Self {
        Self {
            instance_registry: HashMap::new(),
            stack_sizes: HashMap::new(),
            current_interaction: None,
            pending_deletes: Vec::new(),
            activity_contexts: Vec::new(),
            pending_interaction: None,
            pending_chat_gains: Vec::new(),
            chat_gain_matched: 0,
            chat_gain_fallback: 0,
            login_batch_run: 0,
            inventory_login_batch: false,
            login_batch_fired: false,
            last_line_secs: 0,
            pending_pigeon_send: None,
            pending_stall_note_send: None,
            vendor_screen: None,
            last_item_event: None,
            skill_levels: HashMap::new(),
            last_anatomy: None,
            pending_extract_equip_bonus: None,
            last_corpse_search: None,
        }
    }

    /// Inventory-accuracy instrumentation: `(matched, fallback)` chat-gain
    /// seed counts. Invariant on fixture replay: `matched + fallback` ==
    /// new-item `(is_new=true, slot<0)` AddItem count.
    #[cfg(test)]
    pub(crate) fn chat_gain_counts_for_test(&self) -> (u64, u64) {
        (self.chat_gain_matched, self.chat_gain_fallback)
    }

    /// One-shot login-batch signal: true once per `is_new=false` burst that
    /// crossed `LOGIN_BATCH_MIN_ITEMS`; false after taken, and not re-armed
    /// until the burst ends (the `login_batch_fired` latch only clears where
    /// the run resets). The game-state layer clears the inventory slice once,
    /// before the batch persists.
    pub fn take_inventory_login_batch(&mut self) -> bool {
        std::mem::replace(&mut self.inventory_login_batch, false)
    }

    /// Check whether an instance_id is known in the instance registry.
    /// Used by replay/debug tools to determine how a LootPickedUp was resolved.
    pub fn has_instance(&self, instance_id: u64) -> bool {
        self.instance_registry.contains_key(&instance_id)
    }

    /// Feed a chat `[Status] X added to inventory` event into the parser's
    /// correlation buffer.
    ///
    /// The coordinator resolves the chat's display name to the CDN internal
    /// name before calling this so matching can be exact (chat uses "Rubywall
    /// Crystal" but player.log uses "RedCrystal" — different strings for the
    /// same item). The `timestamp` must be a Player.log-style "HH:MM:SS" in
    /// UTC so it aligns with player.log timestamps for correlation.
    ///
    /// The buffer is consulted during `ProcessAddItem` handling to seed new
    /// stacks from the chat's authoritative quantity instead of the fallback 1.
    pub fn feed_chat_gain(
        &mut self,
        item_internal_name: String,
        quantity: u32,
        timestamp_hms: &str,
    ) {
        let captured_at_secs = timestamp_to_secs(timestamp_hms);
        // Age out any chat gains older than the lifetime before pushing new ones.
        // Also age out any whose capture is way in the future (clock skew safety).
        self.pending_chat_gains.retain(|g| {
            let age = captured_at_secs.saturating_sub(g.captured_at_secs);
            age <= CHAT_GAIN_BUFFER_LIFETIME_SECS
        });
        self.pending_chat_gains.push(PendingChatGain {
            item_internal_name,
            quantity,
            captured_at_secs,
        });
    }

    /// Look up and consume a matching chat gain for a ProcessAddItem. Returns
    /// the quantity to seed the stack with, or `None` if no match was found.
    /// Matching is by exact internal name and within `CHAT_GAIN_MATCH_WINDOW_SECS`
    /// of the AddItem's timestamp (both directions).
    fn consume_chat_gain_for_add_item(
        &mut self,
        item_internal_name: &str,
        add_item_at_secs: u32,
    ) -> Option<u32> {
        // Find the closest-in-time matching entry. Prefer closest to avoid
        // stealing a chat gain that belongs to a different, later AddItem.
        let mut best_idx: Option<usize> = None;
        let mut best_dist: u32 = u32::MAX;
        for (i, g) in self.pending_chat_gains.iter().enumerate() {
            if g.item_internal_name != item_internal_name {
                continue;
            }
            let dist = g.captured_at_secs.abs_diff(add_item_at_secs);
            if dist <= CHAT_GAIN_MATCH_WINDOW_SECS && dist < best_dist {
                best_idx = Some(i);
                best_dist = dist;
            }
        }
        best_idx.map(|i| self.pending_chat_gains.swap_remove(i).quantity)
    }

    // ============================================================
    // Activity Context helpers
    // ============================================================

    /// Snapshot of currently-active contexts, most-recent last. Primarily for
    /// tests and future attribution logic.
    #[allow(dead_code)] // Phase 2 consumers will call this
    pub fn active_activities(&self) -> &[ActivityContext] {
        &self.activity_contexts
    }

    /// Push a new activity context. Contexts are treated as a stack; multiple
    /// can be active simultaneously (e.g., a corpse search started while a
    /// mining context hasn't yet timed out).
    fn push_activity(&mut self, ctx: ActivityContext) {
        self.activity_contexts.push(ctx);
    }

    /// Close all contexts matching the given entity_id. Used for explicit
    /// `ProcessEndInteraction` signals.
    fn close_activities_for_entity(&mut self, entity_id: u32) {
        self.activity_contexts
            .retain(|c| c.entity_id != Some(entity_id));
    }

    /// Expire any contexts whose close_deadline is at or before `now_secs`.
    /// Called at the start of each line so attribution reflects only active
    /// contexts at the line's timestamp.
    fn expire_stale_activities(&mut self, now_secs: u32) {
        self.activity_contexts
            .retain(|c| now_secs < c.close_deadline_secs);
        // Pending interaction metadata also expires after ~5 seconds so it
        // doesn't cross-pollinate unrelated later events.
        if let Some(pi) = &self.pending_interaction {
            if now_secs.saturating_sub(pi.captured_at_secs) > 5 {
                self.pending_interaction = None;
            }
        }
        // Aged-out chat gains are discarded to prevent them drifting into
        // attribution of unrelated later AddItems.
        self.pending_chat_gains.retain(|g| {
            let age = now_secs.saturating_sub(g.captured_at_secs);
            age <= CHAT_GAIN_BUFFER_LIFETIME_SECS
        });
    }

    /// Compute provenance for a gain event based on the current activity
    /// stack. Returns `UnknownSource` when nothing is active, `Attributed`
    /// when exactly one context matches, and `Uncertain` when multiple are
    /// plausible and no confident tie-breaker applies.
    ///
    /// Phase 2 keeps the rules deliberately simple: single-context
    /// attribution is `Confident`, multi-context is `Uncertain`. Richer
    /// heuristics (item-type-to-loot-table matching, recency bias) live in
    /// Phase 2.5+ once we have real-world attribution data to tune against.
    fn compute_provenance(&self) -> ItemProvenance {
        match self.activity_contexts.len() {
            0 => ItemProvenance::UnknownSource,
            1 => ItemProvenance::Attributed {
                source: self.activity_contexts[0].source.clone(),
                confidence: AttributionConfidence::Confident,
                survey_use_id: None,
            },
            _ => {
                // Tie-breaker for multi-context cases: when Mining is active
                // alongside a more passive context (CorpseSearch, VendorBrowsing,
                // StorageBrowsing), the gain almost always belongs to Mining.
                // Justification — mining is an explicit active task (delay loop
                // swinging a pick); the others are screens that happen to be open.
                // Real-world data (50x-povus dataset): players constantly fight
                // mobs between mining swings, so CorpseSearch + Mining overlap is
                // the dominant ambiguity case, and ~99% of the time the loot is
                // mining loot.
                //
                // Confidence is Probable rather than Confident to mark this as a
                // heuristic outcome — downstream code can choose to filter on
                // confidence level if it cares.
                let mining_idx = self
                    .activity_contexts
                    .iter()
                    .position(|c| matches!(c.source, ActivitySource::Mining { .. }));
                if let Some(idx) = mining_idx {
                    let others_are_passive = self
                        .activity_contexts
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| *i != idx)
                        .all(|(_, c)| {
                            matches!(
                                c.source,
                                ActivitySource::CorpseSearch { .. }
                                    | ActivitySource::VendorBrowsing { .. }
                                    | ActivitySource::StorageBrowsing { .. }
                            )
                        });
                    if others_are_passive {
                        return ItemProvenance::Attributed {
                            source: self.activity_contexts[idx].source.clone(),
                            confidence: AttributionConfidence::Probable,
                            survey_use_id: None,
                        };
                    }
                }
                ItemProvenance::Uncertain {
                    candidates: self
                        .activity_contexts
                        .iter()
                        .map(|c| c.source.clone())
                        .collect(),
                }
            }
        }
    }

    /// Feed one log line; returns zero or more events.
    pub fn process_line(&mut self, line: &str) -> Vec<PlayerEvent> {
        let mut events = Vec::new();

        // Expire any activity contexts whose close_deadline has passed,
        // using this line's timestamp as "now". This makes provenance
        // attribution reflect only contexts that were still active when
        // the event occurred.
        if let Some(ts) = parse_timestamp(line) {
            let now_secs = timestamp_to_secs(&ts);
            self.last_line_secs = now_secs;
            self.expire_stale_activities(now_secs);
        }

        // Try to resolve pending deletes against this line. Storage/vendor
        // lines consume their matching deletes; every other line only ages
        // the buffer (retained up to 5 s / 5 lines for a late confirmation,
        // then flushed as `Unknown` inside `resolve_pending_deletes`).
        let resolved = self.resolve_pending_deletes(line, &mut events);

        // If we resolved pending deletes via storage/vendor, the line was consumed
        if resolved {
            return events;
        }

        // The Player.log login line declares the session's own UTC instant.
        // It is not a `LocalPlayer: Process` line, so it must be handled before
        // the fast path below; nothing else can consume it.
        if line.contains("Logged in as character ") && line.contains("Time UTC=") {
            // New session: the next `is_new=false` burst is a fresh dump.
            // Reset the run and the fired latch (the stale one-shot's clear
            // belongs to the burst that set it, handled before this line's
            // batch).
            self.login_batch_run = 0;
            self.login_batch_fired = false;
            // A delete still buffered here must be emitted *before* the
            // anchor: it belongs to an earlier line, and the date resolver
            // is stateful — an earlier time of day after the anchor reads
            // as a UTC-midnight crossing and dates every following event a
            // day ahead.
            self.flush_pending_deletes(&mut events);
            if let Some((_character, utc)) = crate::parsers::parse_player_log_login_line(line) {
                events.push(PlayerEvent::LogSessionAnchored { utc });
            }
            return events;
        }

        // Non-LocalPlayer engine lines handled before the fast path below.
        // `AreaConfig Setting weather:` is the live weather source since the
        // game stopped emitting ProcessSetWeather.
        if line.contains("AreaConfig Setting weather:") {
            if let Some(ev) = self.parse_area_config_weather(line) {
                events.push(ev);
            }
            return events;
        }

        // Fast path: skip lines that aren't LocalPlayer Process events
        // (but still flush pending deletes above)
        if !line.contains("LocalPlayer: Process") {
            // Flush any remaining pending deletes as Unknown
            return events;
        }

        // Dispatch by event type
        if line.contains("ProcessAddItem(") {
            // Flush pending deletes before processing new events
            if let Some(ev) = self.parse_add_item(line) {
                // Track for RemoveLoot correlation
                if let PlayerEvent::ItemAdded {
                    instance_id,
                    item_name,
                    initial_quantity,
                    ..
                } = &ev
                {
                    self.last_item_event = Some(LastItemEvent {
                        instance_id: *instance_id,
                        item_name: item_name.clone(),
                        item_type_id: None,
                        quantity: *initial_quantity,
                    });
                }
                events.push(ev);
            }
        } else if line.contains("ProcessUpdateItemCode(") {
            if let Some(ev) = self.parse_update_item_code(line) {
                // Track for RemoveLoot correlation (positive deltas only)
                if let PlayerEvent::ItemStackChanged {
                    instance_id,
                    item_name,
                    item_type_id,
                    delta,
                    ..
                } = &ev
                {
                    if *delta > 0 {
                        self.last_item_event = Some(LastItemEvent {
                            instance_id: *instance_id,
                            item_name: item_name.clone().unwrap_or_default(),
                            item_type_id: Some(*item_type_id),
                            quantity: *delta as u32,
                        });
                    }
                }
                events.push(ev);
            }
        } else if line.contains("ProcessDeleteItem(") {
            // Don't flush yet — buffer this delete
            self.parse_delete_item(line);
        } else if line.contains("ProcessLoadSkills(") {
            if let Some(ev) = self.parse_load_skills(line) {
                // Seed the running skill table from the full snapshot.
                if let PlayerEvent::SkillsLoaded { skills, .. } = &ev {
                    for s in skills {
                        self.skill_levels
                            .insert(s.skill_type.clone(), (s.raw, s.bonus));
                    }
                }
                events.push(ev);
            }
        } else if line.contains("ProcessStartInteraction(") {
            if let Some(ev) = self.parse_start_interaction(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessDeltaFavor(") {
            if let Some(ev) = self.parse_delta_favor(line) {
                events.push(ev);
            }
        } else if line.contains("gifts per calendar week and has received") {
            // Gift-cap note embedded in a ProcessTalkScreen / ProcessPromptForItem
            // line — carries the game's authoritative weekly gift count.
            if let Some(ev) = parse_gift_count_line(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessVendorAddItem(") {
            // This should have been handled in resolve_pending_deletes,
            // but handle standalone case too
            if let Some(ev) = self.parse_vendor_add_item(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessVendorUpdateItem(") {
            if let Some(ev) = self.parse_vendor_update_item(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessAddToStorageVault(") {
            // This should have been handled in resolve_pending_deletes,
            // but handle standalone case too
            if let Some(ev) = self.parse_add_to_storage(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessRemoveFromStorageVault(") {
            if let Some(ev) = self.parse_remove_from_storage(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessRemoveLoot(") {
            if let Some(ev) = self.parse_remove_loot(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessUpdateSkill(") {
            // Keep the running skill table current and note any anatomy update
            // (anatomy XP is granted when butchering/skinning that monster type).
            self.track_skill_update(line);
            // Butchering/Skinning XP correlates to a just-added corpse extract
            // that never produces a RemoveLoot. Detect those here; all other
            // skill updates are handled by the separate skill-update parser.
            if let Some(ev) = self.parse_corpse_extract(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessDoDelayLoop(") {
            if let Some(ev) = self.parse_delay_loop(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessScreenText(") {
            if let Some(ev) = self.parse_screen_text(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessBook(") {
            events.extend(self.parse_book(line));
        } else if line.contains("ProcessEndInteraction(") {
            if let Some(ev) = self.parse_end_interaction(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessTalkScreen(") {
            // Context-only for corpse search. Also emits PlayerMessage events
            // for pigeon sends and stall note sends.
            self.handle_talk_screen(line, &mut events);
        } else if line.contains("ProcessPlayerVendorScreen(") {
            // Player-stall screens: (npcId, helpText, listType, slotCount,
            // isManager, ...). Tracked as a session for price-capture;
            // ShopLog books (own stall) are handled by parse_book separately.
            self.handle_player_vendor_screen(line, &mut events);
        } else if line.contains("ProcessPlayerVendorScreenRemove(") {
            // Item taken off a player-stall screen (purchase from another
            // player, or collecting your own listing back).
            self.handle_player_vendor_screen_remove(line, &mut events);
        } else if line.contains("ProcessVendorScreen(") {
            // Context-only: vendor window open.
            self.handle_vendor_screen(line);
        } else if line.contains("ProcessShowStorageVault(") {
            // Context-only: storage UI open.
            self.handle_show_storage_vault(line);
        } else if line.contains("ProcessSetActiveSkills(") {
            if let Some(ev) = self.parse_set_active_skills(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessPlayerMount(") {
            if let Some(ev) = self.parse_player_mount(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessSetWeather(") {
            if let Some(ev) = self.parse_set_weather(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessUpdateRecipe(") {
            if let Some(ev) = self.parse_update_recipe(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessCombatModeStatus(") {
            if let Some(ev) = self.parse_combat_mode_status(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessVendorUpdateAvailableGold(") {
            if let Some(ev) = self.parse_vendor_update_gold(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessSetAttributes(") {
            if let Some(ev) = self.parse_set_attributes(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessLoadAbilities(") {
            if let Some(ev) = self.parse_load_abilities(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessLoadRecipes(") {
            if let Some(ev) = self.parse_load_recipes(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessSetEquippedItems(") {
            if let Some(ev) = self.parse_set_equipped_items(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessAddEffects(") {
            if let Some(ev) = self.parse_add_effects(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessRemoveEffects(") {
            if let Some(ev) = self.parse_remove_effects(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessUpdateEffectName(") {
            if let Some(ev) = self.parse_update_effect_name(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessUpdateDescription(") {
            if let Some(ev) = self.parse_update_description(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessSetCelestialInfo(") {
            if let Some(ev) = self.parse_celestial_info(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessGuildGeneralInfo(") {
            if let Some(ev) = self.parse_guild_general_info(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessCompleteDirectedGoals(") {
            if let Some(ev) = self.parse_complete_directed_goals(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessSetString(") {
            if let Some(ev) = self.parse_set_string(line) {
                events.push(ev);
            }
        } else if line.contains("ProcessInputBox(") {
            // Track stall note sends: title contains "Send Note to <player>"
            self.handle_input_box(line);
        } else {
            // Unrecognized Process line — nothing to emit (pending deletes were
            // already flushed at the top of `process_line`).
        }

        events
    }

    /// Flush any buffered pending deletes as events.
    /// Call this at end-of-poll to ensure nothing is left hanging.
    pub fn flush_all_pending(&mut self) -> Vec<PlayerEvent> {
        let mut events = Vec::new();
        self.flush_pending_deletes(&mut events);
        events
    }

    // ============================================================
    // Pending Delete Resolution
    // ============================================================

    /// Try to match pending deletes against a storage/vendor line.
    /// Returns true if the line was consumed (was a storage/vendor event).
    /// Non-matching lines age the buffer: entries older than
    /// `DELETE_BUFFER_MAX_AGE_SECS` or past `DELETE_BUFFER_MAX_LINES`
    /// lines flush as `Unknown`; the rest stay buffered.
    fn resolve_pending_deletes(&mut self, line: &str, events: &mut Vec<PlayerEvent>) -> bool {
        if self.pending_deletes.is_empty() {
            return false;
        }
        // Current line's clock for the age check. Midnights roll the day
        // counter, not the seconds value — `saturating_sub` keeps a borrow
        // failure mode of "treat cross-midnight as age 0, keep buffering"
        // instead of flushing a delete that is a second old.
        let now_secs = parse_timestamp(line)
            .map(|ts| timestamp_to_secs(&ts))
            .unwrap_or(0);

        // Check for ProcessAddToStorageVault with matching instance ID
        if line.contains("ProcessAddToStorageVault(") {
            if let Some(storage_ev) = self.parse_add_to_storage(line) {
                let storage_instance_id = match &storage_ev {
                    PlayerEvent::StorageDeposit { instance_id, .. } => *instance_id,
                    _ => 0,
                };

                // Emit matched deletes as StorageTransfer, unmatched as Unknown
                let pending = std::mem::take(&mut self.pending_deletes);
                for pd in pending {
                    let context = if pd.instance_id == storage_instance_id {
                        DeleteContext::StorageTransfer
                    } else {
                        DeleteContext::Unknown
                    };
                    events.push(PlayerEvent::ItemDeleted {
                        timestamp: pd.timestamp,
                        instance_id: pd.instance_id,
                        item_name: pd.item_name,
                        context,
                    });
                }
                events.push(storage_ev);
                return true;
            }
        }

        // Check for ProcessVendorAddItem with matching instance ID
        if line.contains("ProcessVendorAddItem(") {
            if let Some(vendor_ev) = self.parse_vendor_add_item(line) {
                let vendor_instance_id = match &vendor_ev {
                    PlayerEvent::VendorSold { instance_id, .. } => *instance_id,
                    _ => 0,
                };

                let pending = std::mem::take(&mut self.pending_deletes);
                for pd in pending {
                    let context = if pd.instance_id == vendor_instance_id {
                        DeleteContext::VendorSale
                    } else {
                        DeleteContext::Unknown
                    };
                    events.push(PlayerEvent::ItemDeleted {
                        timestamp: pd.timestamp,
                        instance_id: pd.instance_id,
                        item_name: pd.item_name,
                        context,
                    });
                }
                events.push(vendor_ev);
                return true;
            }
        }

        // Check for ProcessVendorUpdateItem (selling stackable to vendor that already has it)
        if line.contains("ProcessVendorUpdateItem(") {
            if let Some(vendor_ev) = self.parse_vendor_update_item(line) {
                let pending = std::mem::take(&mut self.pending_deletes);
                for pd in pending {
                    events.push(PlayerEvent::ItemDeleted {
                        timestamp: pd.timestamp,
                        instance_id: pd.instance_id,
                        item_name: pd.item_name,
                        context: DeleteContext::VendorSale,
                    });
                }
                events.push(vendor_ev);
                return true;
            }
        }

        // No storage/vendor confirmation on this line: age the buffer and
        // flush only the entries that outlived the retention window.
        let mut expired = Vec::new();
        self.pending_deletes.retain(|pd| {
            let age = now_secs.saturating_sub(pd.seen_at_secs);
            if age > DELETE_BUFFER_MAX_AGE_SECS || pd.lines_seen >= DELETE_BUFFER_MAX_LINES {
                expired.push(pd.clone());
                false
            } else {
                true
            }
        });
        for pd in &mut self.pending_deletes {
            pd.lines_seen += 1;
        }
        for pd in expired {
            events.push(PlayerEvent::ItemDeleted {
                timestamp: pd.timestamp,
                instance_id: pd.instance_id,
                item_name: pd.item_name,
                context: DeleteContext::Unknown,
            });
        }
        false
    }

    /// Flush all pending deletes as ItemDeleted with Unknown context.
    fn flush_pending_deletes(&mut self, events: &mut Vec<PlayerEvent>) {
        let pending = std::mem::take(&mut self.pending_deletes);
        for pd in pending {
            events.push(PlayerEvent::ItemDeleted {
                timestamp: pd.timestamp,
                instance_id: pd.instance_id,
                item_name: pd.item_name,
                context: DeleteContext::Unknown,
            });
        }
    }

    /// Retention-aware drain for the live poll boundary: emit only entries
    /// that already outlived the 5 s / 5-line window as `Unknown`, keep
    /// young ones for the next poll's confirmation. The unconditional
    /// `flush_all_pending` would cap the buffer at one 500 ms poll and
    /// finalize cross-poll Delete/confirmation pairs prematurely.
    /// Full drains stay on the login line (anchor ordering) and end-of-stream
    /// callers (offline import, replay, manual flush).
    ///
    /// The wall-clock backstop (`DELETE_BUFFER_IDLE_TIMEOUT_SECS`) also
    /// applies: when polls arrive with no new lines, log-clock and line-count
    /// retention both freeze, so a delete older than the timeout is emitted
    /// as `Unknown` instead of lingering forever.
    pub fn flush_expired_deletes(&mut self, now_secs: u32, events: &mut Vec<PlayerEvent>) {
        self.flush_expired_deletes_at(now_secs, std::time::Instant::now(), events);
    }

    fn flush_expired_deletes_at(
        &mut self,
        now_secs: u32,
        now_instant: std::time::Instant,
        events: &mut Vec<PlayerEvent>,
    ) {
        if self.pending_deletes.is_empty() {
            return;
        }
        let mut expired = Vec::new();
        self.pending_deletes.retain(|pd| {
            let age = now_secs.saturating_sub(pd.seen_at_secs);
            let idle_secs = now_instant
                .saturating_duration_since(pd.seen_at_instant)
                .as_secs();
            if (now_secs != 0 && age > DELETE_BUFFER_MAX_AGE_SECS)
                || pd.lines_seen >= DELETE_BUFFER_MAX_LINES
                || idle_secs >= DELETE_BUFFER_IDLE_TIMEOUT_SECS
            {
                expired.push(pd.clone());
                false
            } else {
                true
            }
        });
        for pd in expired {
            events.push(PlayerEvent::ItemDeleted {
                timestamp: pd.timestamp,
                instance_id: pd.instance_id,
                item_name: pd.item_name,
                context: DeleteContext::Unknown,
            });
        }
    }

    /// Last-seen line clock for the poll-boundary drain: seconds-of-day of
    /// the most recent timestamped line, or 0 when no line carried one
    /// (caller then keeps everything — age unknown, count still applies).
    pub fn last_line_secs(&self) -> u32 {
        self.last_line_secs
    }

    // ============================================================
    // Individual Parse Functions
    // ============================================================

    /// ProcessAddItem(InternalName(instanceId), slotIndex, isNew)
    fn parse_add_item(&mut self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessAddItem(")? + "ProcessAddItem(".len();
        let args_section = &line[args_start..];

        // Extract InternalName(instanceId) — name is before '(', id is inside parens
        let inner_paren = args_section.find('(')?;
        let item_name = args_section[..inner_paren].to_string();

        let id_start = inner_paren + 1;
        let id_end = args_section[id_start..].find(')')? + id_start;
        let instance_id = parse_instance_id(&args_section[id_start..id_end])?;

        // After the closing paren of InternalName(id), we have ", slotIndex, isNew)"
        let after_name = &args_section[id_end + 1..];
        let parts: Vec<&str> = after_name.split(',').collect();
        // parts[0] = ")", parts[1] = " slotIndex", parts[2] = " isNew)"  (or similar)
        let slot_index: i32 = parts.get(1)?.trim().parse().ok()?;
        let is_new_str = parts.get(2)?.trim().trim_end_matches(')');
        let is_new = is_new_str == "True";

        // Register in instance registry
        self.instance_registry.insert(
            instance_id,
            InstanceInfo {
                item_name: item_name.clone(),
                item_type_id: None,
            },
        );

        // Track the login-batch full-state dump: consecutive `is_new=false`
        // AddItems with no intervening UpdateItemCode since the last login.
        // At `LOGIN_BATCH_MIN_ITEMS` the burst is a login dump, not live
        // play — set the one-shot clear flag (mirrors effects'
        // `is_login_batch`). Any genuine gain breaks the run.
        if !is_new {
            self.login_batch_run = self.login_batch_run.saturating_add(1);
            if self.login_batch_run >= LOGIN_BATCH_MIN_ITEMS && !self.login_batch_fired {
                self.inventory_login_batch = true;
                self.login_batch_fired = true;
            }
        } else {
            self.login_batch_run = 0;
            self.login_batch_fired = false;
        }

        // Seed stack tracking for genuinely new items.
        // - slot_index >= 0 with is_new=true means a storage vault withdrawal;
        //   do NOT seed here — ProcessRemoveFromStorageVault will seed with the
        //   correct quantity so we don't report a false delta.
        // - slot_index == -1 with is_new=true means a real new acquisition (loot,
        //   craft, vendor purchase). If a matching chat [Status] gain event is
        //   buffered within a short window, seed from the chat's authoritative
        //   quantity; otherwise fall back to 1 (which was the prior behavior and
        //   remains correct for the common single-item-pickup case).
        // - is_new=false is a session-start inventory load; do NOT seed — the first
        //   ProcessUpdateItemCode will establish the baseline.
        let initial_quantity = if is_new && slot_index < 0 {
            let add_item_secs = timestamp_to_secs(&ts);
            match self.consume_chat_gain_for_add_item(&item_name, add_item_secs) {
                Some(seed_qty) => {
                    self.chat_gain_matched += 1;
                    self.stack_sizes.insert(instance_id, seed_qty);
                    seed_qty
                }
                None => {
                    self.chat_gain_fallback += 1;
                    self.stack_sizes.insert(instance_id, 1);
                    1
                }
            }
        } else {
            1
        };

        // Provenance reflects the currently-active activity stack. Session-
        // load AddItems (is_new=false) aren't gains — mark them NotApplicable
        // so downstream aggregates can filter them out without caring about
        // the surrounding context (which could be anything at login time).
        //
        // Exception: a new item arriving while another player's stall screen
        // session is open is a stall purchase. The activity-context variant of
        // VendorBrowsing expires after 30s (players browse long before buying),
        // so the authoritative purchase signal is the vendor_screen session —
        // set on ProcessPlayerVendorScreen and cleared on EndInteraction.
        // The paired ProcessPlayerVendorScreenRemove that follows confirms the
        // purchase (same instance_id); without it the provisional stamp is
        // harmless: provenance is informational, and the coordinator records
        // the purchase row from the Remove event, not from this stamp.
        let provenance = if !is_new {
            ItemProvenance::NotApplicable
        } else if let Some((npc_id, false)) = self.vendor_screen {
            ItemProvenance::Attributed {
                source: ActivitySource::VendorBrowsing {
                    npc_entity_id: npc_id,
                    npc_name: None,
                },
                confidence: AttributionConfidence::Confident,
                survey_use_id: None,
            }
        } else {
            self.compute_provenance()
        };

        Some(PlayerEvent::ItemAdded {
            timestamp: ts,
            item_name,
            instance_id,
            slot_index,
            is_new,
            initial_quantity,
            provenance,
        })
    }

    /// ProcessUpdateItemCode(instanceId, encodedValue, fromServer)
    fn parse_update_item_code(&mut self, line: &str) -> Option<PlayerEvent> {
        // A baseline arriving mid-burst means live play, not a login dump.
        self.login_batch_run = 0;
        self.login_batch_fired = false;
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessUpdateItemCode(")? + "ProcessUpdateItemCode(".len();
        let args_end = line[args_start..].find(')')? + args_start;
        let args = &line[args_start..args_end];

        let parts: Vec<&str> = args.split(',').collect();
        let instance_id = parse_instance_id(parts.first()?)?;
        let encoded_value: u32 = parts.get(1)?.trim().parse().ok()?;
        let from_server = parts.get(2)?.trim() == "True";

        // Encoding is 0-based: actual stack size = (encoded >> 16) + 1.
        // Verified against JSON inventory exports where StackSize fields are 1-based.
        let new_stack_size = (encoded_value >> 16) + 1;
        let item_type_id = (encoded_value & 0xFFFF) as u16;

        let had_prior = self.stack_sizes.contains_key(&instance_id);
        let old_stack_size = self.stack_sizes.get(&instance_id).copied().unwrap_or(0);
        let delta = new_stack_size as i32 - old_stack_size as i32;

        // Update tracking state — always record the new stack size
        self.stack_sizes.insert(instance_id, new_stack_size);

        // Update type ID in registry if we have an entry
        if let Some(info) = self.instance_registry.get_mut(&instance_id) {
            info.item_type_id = Some(item_type_id);
        }

        // If we had no prior stack observation, this is establishing a baseline
        // (e.g., session-start inventory load after the AddItem burst).
        //
        // If a matching chat [Status] gain exists, this is actually a real gain
        // on top of a login-loaded stack — emit with the chat quantity as delta.
        //
        // Otherwise, emit with delta=0 so the DB gets the correct absolute
        // stack size (the prior AddItem only wrote stack_size=1). The zero delta
        // ensures no transaction is recorded — this is purely a DB correction.
        if !had_prior {
            let item_name = self
                .instance_registry
                .get(&instance_id)
                .map(|info| info.item_name.clone());
            let update_secs = timestamp_to_secs(&ts);
            let (effective_delta, effective_provenance) = if let Some(chat_qty) = item_name
                .as_ref()
                .and_then(|name| self.consume_chat_gain_for_add_item(name, update_secs))
            {
                (chat_qty as i32, self.compute_provenance())
            } else {
                (0, ItemProvenance::NotApplicable)
            };
            return Some(PlayerEvent::ItemStackChanged {
                timestamp: ts,
                instance_id,
                item_name,
                item_type_id,
                old_stack_size: new_stack_size.saturating_sub(effective_delta.max(0) as u32),
                new_stack_size,
                delta: effective_delta,
                from_server,
                provenance: effective_provenance,
            });
        }

        let item_name = self
            .instance_registry
            .get(&instance_id)
            .map(|info| info.item_name.clone());

        // Provenance only meaningfully applies to positive deltas (gains).
        // For zero or negative deltas (stack unchanged, consumed, split),
        // mark NotApplicable so consumers aggregating gains by source aren't
        // polluted by loss/consumption events.
        let provenance = if delta > 0 {
            self.compute_provenance()
        } else {
            ItemProvenance::NotApplicable
        };

        Some(PlayerEvent::ItemStackChanged {
            timestamp: ts,
            instance_id,
            item_name,
            item_type_id,
            old_stack_size,
            new_stack_size,
            delta,
            from_server,
            provenance,
        })
    }

    /// ProcessDeleteItem(instanceId) — buffers for context resolution
    fn parse_delete_item(&mut self, line: &str) {
        let ts = parse_timestamp(line).unwrap_or_default();
        let args_start = match line.find("ProcessDeleteItem(") {
            Some(i) => i + "ProcessDeleteItem(".len(),
            None => return,
        };
        let args_end = match line[args_start..].find(')') {
            Some(i) => args_start + i,
            None => return,
        };
        let instance_id = match parse_instance_id(&line[args_start..args_end]) {
            Some(id) => id,
            None => return,
        };

        let item_name = self
            .instance_registry
            .get(&instance_id)
            .map(|info| info.item_name.clone());

        // Clean up tracking state
        self.instance_registry.remove(&instance_id);
        self.stack_sizes.remove(&instance_id);

        self.pending_deletes.push(PendingDelete {
            seen_at_secs: timestamp_to_secs(&ts),
            seen_at_instant: std::time::Instant::now(),
            lines_seen: 0,
            timestamp: ts,
            instance_id,
            item_name,
        });
    }

    /// ProcessLoadSkills({type=X,raw=R,bonus=B,xp=X,tnl=T,max=M}, ...)
    fn parse_load_skills(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessLoadSkills(")? + "ProcessLoadSkills(".len();

        let mut skills = Vec::new();
        let rest = &line[args_start..];

        // Iterate through {..} blocks
        let mut pos = 0;
        while pos < rest.len() {
            let block_start = match rest[pos..].find('{') {
                Some(i) => pos + i,
                None => break,
            };
            let block_end = match rest[block_start..].find('}') {
                Some(i) => block_start + i + 1,
                None => break,
            };
            let block = &rest[block_start..block_end];

            if let Some(skill) = parse_skill_block(block) {
                skills.push(skill);
            }

            pos = block_end;
        }

        if skills.is_empty() {
            return None;
        }

        Some(PlayerEvent::SkillsLoaded {
            timestamp: ts,
            skills,
        })
    }

    /// ProcessStartInteraction(entityId, interactionType, distance, canInteract, "NPC_Name")
    fn parse_start_interaction(&mut self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessStartInteraction(")? + "ProcessStartInteraction(".len();
        let args_end = line.rfind(')')?;
        let args = &line[args_start..args_end];

        let parts: Vec<&str> = args.splitn(5, ',').collect();
        let entity_id: u32 = parts.first()?.trim().parse().ok()?;
        let interaction_type: u32 = parts.get(1)?.trim().parse().ok()?;
        // parts[2] = distance, parts[3] = canInteract, parts[4] = "NPC_Name"
        let npc_name_raw = parts.get(4)?.trim();
        let npc_name = npc_name_raw.trim_matches('"').to_string();

        self.current_interaction = Some(InteractionContext {
            entity_id,
            npc_name: npc_name.clone(),
            interaction_type,
        });

        // Capture enrichment metadata for the next screen-specific event.
        // Name may be empty (common for mining nodes, containers); we keep it
        // as None in that case rather than storing an empty string.
        self.pending_interaction = Some(PendingInteractionMeta {
            entity_id,
            name: if npc_name.is_empty() {
                None
            } else {
                Some(npc_name.clone())
            },
            captured_at_secs: timestamp_to_secs(&ts),
        });

        Some(PlayerEvent::InteractionStarted {
            timestamp: ts,
            entity_id,
            interaction_type,
            npc_name,
        })
    }

    /// ProcessDeltaFavor(npcId, "NPC_Name", delta, isGift)
    fn parse_delta_favor(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessDeltaFavor(")? + "ProcessDeltaFavor(".len();
        let args_end = line.rfind(')')?;
        let args = &line[args_start..args_end];

        // Split carefully — NPC_Name is quoted
        let first_comma = args.find(',')?;
        let npc_id: u32 = args[..first_comma].trim().parse().ok()?;

        let rest = &args[first_comma + 1..];
        // Find quoted NPC name
        let q_start = rest.find('"')? + 1;
        let q_end = rest[q_start..].find('"')? + q_start;
        let npc_name = rest[q_start..q_end].to_string();

        let after_name = &rest[q_end + 1..];
        let parts: Vec<&str> = after_name.split(',').collect();
        let delta: f32 = parts.get(1)?.trim().parse().ok()?;
        let is_gift = parts.get(2)?.trim().trim_end_matches(')') == "True";

        Some(PlayerEvent::FavorChanged {
            timestamp: ts,
            npc_id,
            npc_name,
            delta,
            is_gift,
        })
    }

    /// ProcessVendorAddItem(price, InternalName(instanceId), isFromBuyback)
    fn parse_vendor_add_item(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessVendorAddItem(")? + "ProcessVendorAddItem(".len();
        let args = &line[args_start..];

        // First arg: price
        let first_comma = args.find(',')?;
        let price: u32 = args[..first_comma].trim().parse().ok()?;

        // Second arg: InternalName(instanceId)
        let rest = &args[first_comma + 1..];
        let inner_paren = rest.find('(')?;
        let item_name = rest[..inner_paren].trim().to_string();

        let id_start = inner_paren + 1;
        let id_end = rest[id_start..].find(')')? + id_start;
        let instance_id = parse_instance_id(&rest[id_start..id_end])?;

        // Third arg: isFromBuyback
        let after_id = &rest[id_end + 1..];
        let last_comma = after_id.find(',')?;
        let buyback_str = after_id[last_comma + 1..].trim().trim_end_matches(')');
        let is_buyback = buyback_str == "True";

        Some(PlayerEvent::VendorSold {
            timestamp: ts,
            price,
            item_name,
            instance_id,
            is_buyback,
        })
    }

    /// ProcessVendorUpdateItem(instanceId, encodedValue, price)
    fn parse_vendor_update_item(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessVendorUpdateItem(")? + "ProcessVendorUpdateItem(".len();
        let args_end = line[args_start..].find(')')? + args_start;
        let args = &line[args_start..args_end];

        let parts: Vec<&str> = args.split(',').collect();
        let instance_id = parse_instance_id(parts.first()?)?;
        let encoded_value: u32 = parts.get(1)?.trim().parse().ok()?;
        let price: u32 = parts.get(2)?.trim().parse().ok()?;

        let new_stack_size = (encoded_value >> 16) + 1;
        let item_type_id = (encoded_value & 0xFFFF) as u16;

        Some(PlayerEvent::VendorStackUpdated {
            timestamp: ts,
            instance_id,
            item_type_id,
            new_stack_size,
            price,
        })
    }

    /// ProcessAddToStorageVault(npcId, -1, slot, InternalName(instanceId))
    fn parse_add_to_storage(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start =
            line.find("ProcessAddToStorageVault(")? + "ProcessAddToStorageVault(".len();
        let args = &line[args_start..];

        let parts: Vec<&str> = args.splitn(4, ',').collect();
        let npc_id: u32 = parts.first()?.trim().parse().ok()?;
        // parts[1] = -1 (skip)
        let slot: i32 = parts.get(2)?.trim().parse().ok()?;

        // parts[3] = " InternalName(instanceId))"
        let name_part = parts.get(3)?.trim();
        let inner_paren = name_part.find('(')?;
        let item_name = name_part[..inner_paren].to_string();

        let id_start = inner_paren + 1;
        let id_end = name_part[id_start..].find(')')? + id_start;
        let instance_id = parse_instance_id(&name_part[id_start..id_end])?;

        let vault_key = self
            .current_interaction
            .as_ref()
            .map(|ctx| ctx.npc_name.clone());

        Some(PlayerEvent::StorageDeposit {
            timestamp: ts,
            npc_id,
            vault_key,
            slot,
            item_name,
            instance_id,
        })
    }

    /// ProcessRemoveFromStorageVault(npcId, -1, instanceId, quantity)
    fn parse_remove_from_storage(&mut self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start =
            line.find("ProcessRemoveFromStorageVault(")? + "ProcessRemoveFromStorageVault(".len();
        let args_end = line[args_start..].find(')')? + args_start;
        let args = &line[args_start..args_end];

        let parts: Vec<&str> = args.split(',').collect();
        let npc_id: u32 = parts.first()?.trim().parse().ok()?;
        // parts[1] = -1 (skip)
        let instance_id = parse_instance_id(parts.get(2)?)?;
        let quantity: u32 = parts.get(3)?.trim().parse().ok()?;

        // Seed stack size from the vault's known quantity.
        // This covers storage withdrawals where AddItem(slot>=0, True)
        // intentionally skipped seeding so we could use this authoritative value.
        // If the vault item merges into an existing inventory stack (different
        // instance_id), this seeds the vault instance which will never get an
        // UpdateItemCode — harmless.
        self.stack_sizes.entry(instance_id).or_insert(quantity);

        let vault_key = self
            .current_interaction
            .as_ref()
            .map(|ctx| ctx.npc_name.clone());

        // Storage withdrawals are definitionally attributable to the
        // currently-active StorageBrowsing context. Prefer the one whose
        // vault_owner matches this npc_id; fall back to any StorageBrowsing
        // if the id doesn't match; only then fall back to generic
        // provenance logic.
        let provenance = self
            .activity_contexts
            .iter()
            .find(|c| matches!(&c.source, ActivitySource::StorageBrowsing { vault_owner_entity_id, .. } if *vault_owner_entity_id == npc_id))
            .or_else(|| {
                self.activity_contexts
                    .iter()
                    .find(|c| matches!(&c.source, ActivitySource::StorageBrowsing { .. }))
            })
            .map(|c| ItemProvenance::Attributed {
                source: c.source.clone(),
                confidence: AttributionConfidence::Confident,
                survey_use_id: None,
            })
            .unwrap_or_else(|| self.compute_provenance());

        Some(PlayerEvent::StorageWithdrawal {
            timestamp: ts,
            npc_id,
            vault_key,
            instance_id,
            quantity,
            provenance,
        })
    }

    /// ProcessRemoveLoot(instanceId)
    /// Fires when the player picks up an item from a corpse's loot window.
    /// Skinning/butchering rewards do NOT produce this event.
    fn parse_remove_loot(&mut self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessRemoveLoot(")? + "ProcessRemoveLoot(".len();
        let args_end = line[args_start..].find(')')? + args_start;
        let instance_id = parse_instance_id(&line[args_start..args_end])?;

        // Attribute this pickup to the corpse whose loot window is open — but
        // ONLY if that `Search Corpse of X` is still fresh. PG re-fires the
        // corpse's search TalkScreen in the same tick as each item taken off it,
        // so genuine corpse loot lands within `CORPSE_LOOT_MAX_SEARCH_AGE_SECS`
        // of the mark. Gathered/ground loot (e.g. Xogrite Chunks) fires its own
        // RemoveLoot with no corpse window; a stale mark from a monster killed
        // seconds earlier must NOT capture it (that was the mis-filing bug).
        let remove_at_secs = timestamp_to_secs(&ts);
        let (corpse_entity_id, corpse_name) = match &self.last_corpse_search {
            Some(mark)
                if remove_at_secs.saturating_sub(mark.at_secs)
                    <= CORPSE_LOOT_MAX_SEARCH_AGE_SECS =>
            {
                (Some(mark.entity_id), Some(mark.corpse_name.clone()))
            }
            _ => (None, None),
        };

        // Resolve item identity:
        // 1. Direct match: instance_id matches a known item from instance_registry
        //    (set by a preceding ProcessAddItem with the same id)
        // 2. Fallback: use last_item_event (the most recent AddItem/UpdateItemCode)
        //    This handles the stacking case where RemoveLoot has an orphaned id
        let (item_name, item_type_id, quantity) =
            if let Some(info) = self.instance_registry.get(&instance_id) {
                // Direct match — this instance was seen in a ProcessAddItem
                let qty = self
                    .last_item_event
                    .as_ref()
                    .filter(|e| e.instance_id == instance_id)
                    .map(|e| e.quantity)
                    .unwrap_or(1);
                (Some(info.item_name.clone()), info.item_type_id, qty)
            } else if let Some(last) = self.last_item_event.take() {
                // Orphaned instance_id (stacking case) — use the immediately
                // preceding UpdateItemCode which is the item that grew.
                // Take (consume) it so a second RemoveLoot in the same tick
                // without its own UpdateItemCode won't reuse this match.
                (
                    Some(last.item_name.clone()),
                    last.item_type_id,
                    last.quantity,
                )
            } else {
                (None, None, 1)
            };

        // Consume the correlation so a following Butchering/Skinning XP line
        // (which has no AddItem of its own) can't mistake this loot item for a
        // corpse extract.
        self.last_item_event = None;

        Some(PlayerEvent::LootPickedUp {
            timestamp: ts,
            instance_id,
            corpse_entity_id,
            corpse_name,
            item_name,
            item_type_id,
            quantity,
        })
    }

    /// Keep the running skill table current from a `ProcessUpdateSkill` line and
    /// record the latest `Anatomy_<Family>` reading (the anatomy skill for the
    /// monster type just butchered/skinned).
    fn track_skill_update(&mut self, line: &str) {
        let block_start = match line.find('{') {
            Some(i) => i,
            None => return,
        };
        let block_end = match line[block_start..].find('}') {
            Some(i) => block_start + i + 1,
            None => return,
        };
        if let Some(s) = parse_skill_block(&line[block_start..block_end]) {
            self.skill_levels
                .insert(s.skill_type.clone(), (s.raw, s.bonus));
            if let Some(family) = s.skill_type.strip_prefix("Anatomy_") {
                self.last_anatomy = Some(AnatomyObservation {
                    family: family.to_string(),
                    raw: s.raw,
                });
            }
        }
    }

    /// Detect a skinning/butchering extract from a `ProcessUpdateSkill` line.
    /// Butchering/Skinning grant XP for an item that was just added via
    /// `ProcessAddItem` but produces no `ProcessRemoveLoot`. We correlate the
    /// XP line to the immediately-preceding AddItem (last_item_event) and
    /// consume it, so a stray XP line with no fresh AddItem can't reuse it.
    fn parse_corpse_extract(&mut self, line: &str) -> Option<PlayerEvent> {
        // Parse the skill type out of "{type=Butchering,raw=...}"
        let type_idx = line.find("type=")? + "type=".len();
        let rest = &line[type_idx..];
        let type_end = rest.find([',', '}'])?;
        let skill = rest[..type_end].trim();
        if skill != "Butchering" && skill != "Skinning" {
            return None;
        }

        let ts = parse_timestamp(line)?;
        // The extracted item is the most recent AddItem not yet consumed by a
        // RemoveLoot or a prior extract. If absent, this XP granted no item.
        let last = self.last_item_event.take()?;

        // Best-effort corpse attribution from the active CorpseSearch context.
        // The first extract on a fresh corpse may precede its search screen, so
        // this can legitimately be None.
        let corpse_name = self.activity_contexts.iter().rev().find_map(|c| {
            if let ActivitySource::CorpseSearch { corpse_name, .. } = &c.source {
                Some(corpse_name.clone())
            } else {
                None
            }
        });

        // Skill level (Butchering/Skinning) from the line's own raw, falling
        // back to the running table if absent.
        let skill_level = extract_block_field(line, "raw=")
            .and_then(|v| v.parse::<u32>().ok())
            .or_else(|| self.skill_levels.get(skill).map(|(raw, _)| *raw));

        // Anatomy family + level for this monster type, from the most recent
        // Anatomy_<Family> update. Cleared after attribution so it can't bleed
        // onto an unrelated later extract.
        let (anatomy_family, anatomy_level) = match self.last_anatomy.take() {
            Some(a) => (Some(a.family), Some(a.raw)),
            None => (None, None),
        };

        Some(PlayerEvent::CorpseExtract {
            timestamp: ts,
            item_name: last.item_name,
            item_type_id: last.item_type_id,
            quantity: last.quantity,
            skill: skill.to_string(),
            corpse_name,
            skill_level,
            equipment_bonus: self.pending_extract_equip_bonus,
            anatomy_family,
            anatomy_level,
        })
    }

    /// ProcessDoDelayLoop(duration, actionType, "label", entityId, abortCondition)
    fn parse_delay_loop(&mut self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessDoDelayLoop(")? + "ProcessDoDelayLoop(".len();
        let args = &line[args_start..];

        // Split into parts: duration, actionType, "label", entityId, abortCondition
        let first_comma = args.find(',')?;
        let duration: f32 = args[..first_comma].trim().parse().ok()?;

        let rest = &args[first_comma + 1..];
        let second_comma = rest.find(',')?;
        let action_type = rest[..second_comma].trim().to_string();

        // Extract quoted label
        let q_start = rest.find('"')? + 1;
        let q_end = rest[q_start..].find('"')? + q_start;
        let label = rest[q_start..q_end].to_string();

        // After closing quote: ", entityId, abortCondition)"
        let after_label = &rest[q_end + 1..];
        let parts: Vec<&str> = after_label.split(',').collect();
        let entity_id: u32 = parts.get(1)?.trim().parse().ok()?;
        let abort_condition = parts.get(2)?.trim().trim_end_matches(')').to_string();

        // Derive activity source from action_type + label. This is the
        // primary (reliable) signal — label shape is stable across game
        // versions where the decorative StartInteraction name is not.
        let started_at_secs = timestamp_to_secs(&ts);
        let duration_secs = duration.ceil().max(0.0) as u32;
        let close_deadline_secs = started_at_secs + duration_secs + DELAY_LOOP_SLACK_SECS;

        if let Some(source) = classify_delay_loop(&action_type, &label, &self.pending_interaction) {
            // If the delay loop references an entity_id that matches a recent
            // StartInteraction, adopt that for explicit-end matching. The
            // DoDelayLoop's own entity_id field is the loop target (usually
            // the player), not the interaction entity, so we prefer the
            // pending interaction's entity when available.
            let ctx_entity_id = match &source {
                ActivitySource::Mining { node_entity_id, .. } => *node_entity_id,
                _ => self.pending_interaction.as_ref().map(|p| p.entity_id),
            };

            // Prevent duplicate stacking when a delay loop repeats mid-cycle
            // (some actions re-emit DoDelayLoop on each tick). If the top of
            // the stack is already the same source kind and entity, just
            // extend its deadline instead of stacking.
            if let Some(top) = self.activity_contexts.last_mut() {
                if activity_source_matches(&top.source, &source) && top.entity_id == ctx_entity_id {
                    top.close_deadline_secs = close_deadline_secs;
                    // leave started_at alone — first-seen is canonical
                } else {
                    self.push_activity(ActivityContext {
                        source,
                        started_at: ts.clone(),
                        started_at_secs,
                        close_deadline_secs,
                        entity_id: ctx_entity_id,
                    });
                }
            } else {
                self.push_activity(ActivityContext {
                    source,
                    started_at: ts.clone(),
                    started_at_secs,
                    close_deadline_secs,
                    entity_id: ctx_entity_id,
                });
            }
        }

        Some(PlayerEvent::DelayLoopStarted {
            timestamp: ts,
            duration,
            action_type,
            label,
            entity_id,
            abort_condition,
        })
    }

    /// ProcessScreenText(category, "message")
    fn parse_screen_text(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessScreenText(")? + "ProcessScreenText(".len();
        let args = &line[args_start..];

        // Category is before the first comma
        let first_comma = args.find(',')?;
        let category = args[..first_comma].trim().to_string();

        // Message is the quoted string
        let rest = &args[first_comma + 1..];
        let q_start = rest.find('"')? + 1;
        let q_end = rest.rfind('"')?;
        if q_start >= q_end {
            return None;
        }
        let message = rest[q_start..q_end].to_string();

        Some(PlayerEvent::ScreenText {
            timestamp: ts,
            category,
            message,
        })
    }

    /// ProcessBook("title", "content", "bookType", ...)
    fn parse_book(&self, line: &str) -> Vec<PlayerEvent> {
        let ts = match parse_timestamp(line) {
            Some(t) => t,
            None => return vec![],
        };
        let args_start = match line.find("ProcessBook(") {
            Some(i) => i + "ProcessBook(".len(),
            None => return vec![],
        };
        let args = &line[args_start..];

        // Extract first three quoted strings
        // Reworked to respect \" escaped quotes
        let (Some(title), Some(content), Some(book_type)) = (
            extract_quoted_escaped(args, 0),
            extract_quoted_escaped(args, 1),
            extract_quoted_escaped(args, 2),
        ) else {
            return vec![];
        };
        // Un-escape quotes only, other symbols are expected to remain escaped
        let title = unescape_log_quotes(&title);
        let content = unescape_log_quotes(&content);
        let book_type = unescape_log_quotes(&book_type);

        let mut events = vec![];

        // Detect pigeon receive: title = "Pigeon From <player>"
        if let Some(sender) = title.strip_prefix("Pigeon From ") {
            // Body is text before the optional "\n\n<i>You received:" section
            let body = if let Some(idx) = content.find("\\n\\n<i>You received:") {
                content[..idx].to_string()
            } else {
                content.clone()
            };

            // Item is after "You received:\n" and before "</i>"
            let item = content
                .find("You received:\\n")
                .map(|i| &content[i + "You received:\\n".len()..])
                .and_then(|s| {
                    let end = s.find("</i>").unwrap_or(s.len());
                    let item_name = s[..end].trim();
                    if item_name.is_empty() {
                        None
                    } else {
                        Some(item_name.to_string())
                    }
                });

            events.push(PlayerEvent::PlayerMessage {
                timestamp: ts.clone(),
                message_type: "pigeon".to_string(),
                direction: "received".to_string(),
                other_player: sender.to_string(),
                body,
                item_name: item,
            });
        }

        // Detect stall note receive: book_type = "PlayerShopNote", title = "Note From <player>"
        if book_type == "PlayerShopNote" {
            if let Some(sender) = title.strip_prefix("Note From ") {
                // Content format: "From: <sender>\nTime: <timestamp>\n\n<message>"
                // The actual newlines in the escaped content are \\n
                let body = content
                    .find("\\n\\n")
                    .map(|i| content[i + 4..].to_string())
                    .unwrap_or_default();

                events.push(PlayerEvent::PlayerMessage {
                    timestamp: ts.clone(),
                    message_type: "stall_note".to_string(),
                    direction: "received".to_string(),
                    other_player: sender.to_string(),
                    body,
                    item_name: None,
                });
            }
        }

        // Always emit the original BookOpened event too (coordinator uses it)
        events.push(PlayerEvent::BookOpened {
            timestamp: ts,
            title,
            content,
            book_type,
        });

        events
    }

    /// ProcessEndInteraction(entityId)
    /// ProcessTalkScreen(entityId, "title", "body", "footer", ints[], strs[], N, Category)
    ///
    /// Opens a CorpseSearch context when title starts with "Search Corpse of ".
    /// Also detects pigeon-send confirmation screens and stall-note-send confirmations.
    fn handle_talk_screen(&mut self, line: &str, events: &mut Vec<PlayerEvent>) {
        let ts = match parse_timestamp(line) {
            Some(t) => t,
            None => return,
        };
        let args_start = match line.find("ProcessTalkScreen(") {
            Some(i) => i + "ProcessTalkScreen(".len(),
            None => return,
        };
        let args = &line[args_start..];

        // entity_id is first before the comma
        let first_comma = match args.find(',') {
            Some(i) => i,
            None => return,
        };
        // Parsed as i64 because some talk screens (notably "Poem by X" poem
        // recitals) carry a negative entity id like -51. The corpse-search
        // path below re-narrows to u32 where a real entity id is required.
        let entity_id: i64 = match args[..first_comma].trim().parse() {
            Ok(id) => id,
            Err(_) => return,
        };

        // Title is the first quoted string after entity_id
        let rest = &args[first_comma + 1..];
        let q_start = match rest.find('"') {
            Some(i) => i + 1,
            None => return,
        };
        let q_end = match rest[q_start..].find('"') {
            Some(i) => i + q_start,
            None => return,
        };
        let title = &rest[q_start..q_end];

        // Body is the second quoted string
        let after_title = &rest[q_end + 1..];
        let body_text = extract_quoted_string(after_title, 0).unwrap_or_default();

        // --- Poem detection ---
        // Poems are recited via a "Poem by <author>" talk screen. Delegated to
        // the shared `parse_poem_line` so the live parser and the one-time
        // Player.log backfill produce identical results.
        if title.starts_with("Poem by ") {
            if let Some(poem) = parse_poem_line(line) {
                events.push(PlayerEvent::PoemRecorded {
                    timestamp: poem.timestamp,
                    author: poem.author,
                    title: poem.title,
                    content: poem.content,
                });
            }
            // A poem screen is never a pigeon/stall/corpse interaction.
            return;
        }

        // --- Pigeon send detection ---
        // Confirmation screen: "I am sending <recipient> the following..."
        // Contains item name in <i>...</i> tags and message after "Message:\\n<indent=3em>"
        if body_text.contains("I am sending") && body_text.contains("Message:") {
            // Extract recipient: "I am sending <name> the following"
            if let Some(after_sending) = body_text.strip_prefix("Excellent. I am sending ") {
                if let Some(name_end) = after_sending.find(" the following") {
                    let recipient = after_sending[..name_end].to_string();

                    // Extract item from <i>...</i>
                    let item = after_sending
                        .find("<i>")
                        .and_then(|start| {
                            let s = &after_sending[start + 3..];
                            s.find("</i>").map(|end| s[..end].trim().to_string())
                        })
                        .filter(|s| !s.is_empty());

                    // Extract message from "Message:\\n<indent=3em>...<"
                    let message = after_sending
                        .find("Message:\\n<indent=3em>")
                        .map(|i| {
                            let s = &after_sending[i + "Message:\\n<indent=3em>".len()..];
                            let end = s
                                .find("</indent>")
                                .or_else(|| s.find('<'))
                                .unwrap_or(s.len());
                            s[..end].to_string()
                        })
                        .unwrap_or_default();

                    self.pending_pigeon_send = Some(PendingPigeonSend {
                        recipient,
                        message,
                        item_name: item,
                    });
                }
            }
        }

        // "The pigeon is sent!" — emit the pending pigeon send
        if body_text.contains("The pigeon is sent!") {
            if let Some(pending) = self.pending_pigeon_send.take() {
                events.push(PlayerEvent::PlayerMessage {
                    timestamp: ts.clone(),
                    message_type: "pigeon".to_string(),
                    direction: "sent".to_string(),
                    other_player: pending.recipient,
                    body: pending.message,
                    item_name: pending.item_name,
                });
            }
        }

        // Stall note send confirmation: body contains "Note Sent"
        if body_text.contains("Note Sent") {
            if let Some(pending) = self.pending_stall_note_send.take() {
                events.push(PlayerEvent::PlayerMessage {
                    timestamp: ts.clone(),
                    message_type: "stall_note".to_string(),
                    direction: "sent".to_string(),
                    other_player: pending.recipient,
                    body: String::new(), // Note text isn't in the log
                    item_name: None,
                });
            }
        }

        if let Some(corpse_name) = title.strip_prefix("Search Corpse of ") {
            // Corpse entity ids are always positive; narrow back to u32.
            let entity_id = entity_id as u32;
            // Capture the equipment skill bonus for the extract that follows,
            // e.g. "...skinned the corpse (with a +12 skill bonus from
            // equipment)...". Attached to the next CorpseExtract.
            if let Some(bonus) = parse_equipment_bonus(&body_text) {
                self.pending_extract_equip_bonus = Some(bonus);
            }

            // Denominator signal for drop-rate tracking: a loot window opened
            // for this corpse. No-permission corpses are reported but flagged.
            let has_permission = !line.contains("You do not have permission to loot this corpse");
            events.push(PlayerEvent::CorpseSearched {
                timestamp: ts.clone(),
                corpse_entity_id: entity_id,
                corpse_name: corpse_name.trim().to_string(),
                has_permission,
            });

            let started_at_secs = timestamp_to_secs(&ts);

            // Mark this as the corpse whose loot window is currently open, so a
            // following `ProcessRemoveLoot` (fired in the same tick per item) is
            // filed under it — but only while the mark is fresh (see
            // `parse_remove_loot` / `CORPSE_LOOT_MAX_SEARCH_AGE_SECS`).
            self.last_corpse_search = Some(CorpseSearchMark {
                entity_id,
                corpse_name: corpse_name.trim().to_string(),
                at_secs: started_at_secs,
            });
            let source = ActivitySource::CorpseSearch {
                entity_id,
                corpse_name: corpse_name.trim().to_string(),
            };
            // Deduplicate: if we already have a CorpseSearch for this entity,
            // just refresh its deadline.
            let already = self.activity_contexts.iter_mut().find(|c| {
                c.entity_id == Some(entity_id)
                    && matches!(&c.source, ActivitySource::CorpseSearch { .. })
            });
            if let Some(existing) = already {
                existing.close_deadline_secs = started_at_secs + DEFAULT_CONTEXT_LIFETIME_SECS;
            } else {
                self.push_activity(ActivityContext {
                    source,
                    started_at: ts,
                    started_at_secs,
                    close_deadline_secs: started_at_secs + DEFAULT_CONTEXT_LIFETIME_SECS,
                    entity_id: Some(entity_id),
                });
            }
        }
    }

    /// ProcessInputBox — used to detect stall note sends.
    /// Title "Send Note to <player>" sets up pending stall note state.
    fn handle_input_box(&mut self, line: &str) {
        // Look for the title string in the ProcessInputBox args
        // Format: ProcessInputBox(entityId, Type, "title", "body", "label", ...)
        let args_start = match line.find("ProcessInputBox(") {
            Some(i) => i + "ProcessInputBox(".len(),
            None => return,
        };
        let args = &line[args_start..];

        // Title is the third quoted string (after entityId, Type)
        if let Some(title) = extract_quoted_string(args, 0) {
            if let Some(recipient) = title.strip_prefix("Send Note to ") {
                self.pending_stall_note_send = Some(PendingStallNoteSend {
                    recipient: recipient.to_string(),
                });
            }
        }
    }

    /// ProcessPlayerVendorScreen(npcId, helpText, listType, slotCount,
    /// isManager, ...) — another player's stall UI (isManager False) or the
    /// player's own stall management screen (True). Sets the vendor_screen
    /// session used by ProcessPlayerVendorScreenRemove; emits
    /// VendorScreenOpened only for OTHERS' stalls (own-stall activity is
    /// already covered by PlayerShopLog books via parse_book).
    fn handle_player_vendor_screen(&mut self, line: &str, events: &mut Vec<PlayerEvent>) {
        let ts = match parse_timestamp(line) {
            Some(t) => t,
            None => return,
        };
        let marker = "ProcessPlayerVendorScreen(";
        let args_start = match line.find(marker) {
            Some(i) => i + marker.len(),
            None => return,
        };
        let args = &line[args_start..];

        let first_comma = match args.find(',') {
            Some(i) => i,
            None => return,
        };
        let npc_id: u32 = match args[..first_comma].trim().parse() {
            Ok(id) => id,
            Err(_) => return,
        };

        // The isManager flag is the 5th positional arg. Args are comma-space
        // separated, but the helpText arg can contain commas inside its
        // quoted string, so split on ", " boundaries at paren-depth 0 —
        // quote-aware scan instead of a naive split.
        // Arg layout changed or unparseable: treat as someone else's
        // stall (the common case for captures; purchases are gated
        // on Remove anyway).
        let is_own_stall = extract_positional_bool(args, 4).unwrap_or(false);

        self.vendor_screen = Some((npc_id, is_own_stall));

        if !is_own_stall {
            events.push(PlayerEvent::VendorScreenOpened {
                timestamp: ts,
                npc_entity_id: npc_id,
                is_own_stall: false,
            });
        }
    }

    /// ProcessPlayerVendorScreenRemove(npcId, instanceId, ...) — an item was
    /// taken off the currently-open player stall screen: a purchase from
    /// another player's stall, or collecting your own listing. Emits
    /// VendorItemTaken only when the open screen is someone ELSE's stall.
    fn handle_player_vendor_screen_remove(&mut self, line: &str, events: &mut Vec<PlayerEvent>) {
        let ts = match parse_timestamp(line) {
            Some(t) => t,
            None => return,
        };
        let marker = "ProcessPlayerVendorScreenRemove(";
        let args_start = match line.find(marker) {
            Some(i) => i + marker.len(),
            None => return,
        };
        let args = &line[args_start..];

        let first_comma = match args.find(',') {
            Some(i) => i,
            None => return,
        };
        let npc_id: u32 = match args[..first_comma].trim().parse() {
            Ok(id) => id,
            Err(_) => return,
        };
        let rest = &args[first_comma + 1..];
        let instance_id: u64 = match rest.find(',') {
            Some(i) => match rest[..i].trim().parse() {
                Ok(v) => v,
                Err(_) => return,
            },
            None => {
                // Last arg (instanceId) may terminate the arg list.
                let end = rest.find(')').unwrap_or(rest.len());
                match rest[..end].trim().parse() {
                    Ok(v) => v,
                    Err(_) => return,
                }
            }
        };

        if let Some((screen_npc, is_own)) = self.vendor_screen {
            if screen_npc == npc_id && !is_own {
                events.push(PlayerEvent::VendorItemTaken {
                    timestamp: ts,
                    npc_entity_id: npc_id,
                    instance_id,
                    is_own_stall: false,
                });
            }
        }
    }

    /// ProcessVendorScreen(npcId, ...) — opens VendorBrowsing context.
    /// The NPC name is enriched from a recent ProcessStartInteraction when
    /// available; it is not derivable from the VendorScreen args themselves.
    fn handle_vendor_screen(&mut self, line: &str) {
        let ts = match parse_timestamp(line) {
            Some(t) => t,
            None => return,
        };
        let args_start = match line.find("ProcessVendorScreen(") {
            Some(i) => i + "ProcessVendorScreen(".len(),
            None => return,
        };
        let first_comma = match line[args_start..].find(',') {
            Some(i) => args_start + i,
            None => return,
        };
        let npc_id: u32 = match line[args_start..first_comma].trim().parse() {
            Ok(id) => id,
            Err(_) => return,
        };

        let npc_name = self
            .pending_interaction
            .as_ref()
            .filter(|pi| pi.entity_id == npc_id)
            .and_then(|pi| pi.name.clone());

        let started_at_secs = timestamp_to_secs(&ts);
        self.push_activity(ActivityContext {
            source: ActivitySource::VendorBrowsing {
                npc_entity_id: npc_id,
                npc_name,
            },
            started_at: ts,
            started_at_secs,
            close_deadline_secs: started_at_secs + DEFAULT_CONTEXT_LIFETIME_SECS,
            entity_id: Some(npc_id),
        });
    }

    /// ProcessShowStorageVault(ownerId, _, "vault_name", ...) — opens
    /// StorageBrowsing context.
    fn handle_show_storage_vault(&mut self, line: &str) {
        let ts = match parse_timestamp(line) {
            Some(t) => t,
            None => return,
        };
        let args_start = match line.find("ProcessShowStorageVault(") {
            Some(i) => i + "ProcessShowStorageVault(".len(),
            None => return,
        };
        let args = &line[args_start..];

        let first_comma = match args.find(',') {
            Some(i) => i,
            None => return,
        };
        let owner_id: u32 = match args[..first_comma].trim().parse() {
            Ok(id) => id,
            Err(_) => return,
        };

        // Vault display name is the first quoted string after owner_id.
        let rest = &args[first_comma + 1..];
        let vault_name = match rest.find('"') {
            Some(q_start) => {
                let q_start = q_start + 1;
                match rest[q_start..].find('"') {
                    Some(q_end) => rest[q_start..(q_start + q_end)].to_string(),
                    None => String::new(),
                }
            }
            None => String::new(),
        };

        let started_at_secs = timestamp_to_secs(&ts);
        self.push_activity(ActivityContext {
            source: ActivitySource::StorageBrowsing {
                vault_owner_entity_id: owner_id,
                vault_name,
            },
            started_at: ts,
            started_at_secs,
            close_deadline_secs: started_at_secs + DEFAULT_CONTEXT_LIFETIME_SECS,
            entity_id: Some(owner_id),
        });
    }

    fn parse_end_interaction(&mut self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessEndInteraction(")? + "ProcessEndInteraction(".len();
        let args_end = line[args_start..].find(')')? + args_start;
        let entity_id: i32 = line[args_start..args_end].trim().parse().ok()?;

        // Clear interaction context
        self.current_interaction = None;
        // The stall-price capture session ends with the interaction too.
        self.vendor_screen = None;

        // Close any activity contexts keyed to this entity_id. `entity_id` is
        // signed (occasional negative values for special targets like doors);
        // cast to u32 only when non-negative since activity contexts store u32.
        if entity_id >= 0 {
            self.close_activities_for_entity(entity_id as u32);
            // The corpse loot window closed — drop the mark so a subsequent
            // ground-loot pickup can't borrow it even inside the fresh window.
            if self.last_corpse_search.as_ref().map(|m| m.entity_id) == Some(entity_id as u32) {
                self.last_corpse_search = None;
            }
        }

        Some(PlayerEvent::InteractionEnded {
            timestamp: ts,
            entity_id,
        })
    }

    /// ProcessSetActiveSkills(Skill1, Skill2)
    fn parse_set_active_skills(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessSetActiveSkills(")? + "ProcessSetActiveSkills(".len();
        let args_end = line[args_start..].find(')')? + args_start;
        let args = &line[args_start..args_end];

        let parts: Vec<&str> = args.split(',').collect();
        let skill1 = parts.first()?.trim().to_string();
        let skill2 = parts.get(1)?.trim().to_string();

        Some(PlayerEvent::ActiveSkillsChanged {
            timestamp: ts,
            skill1,
            skill2,
        })
    }

    /// ProcessPlayerMount(entityId, isMounting)
    fn parse_player_mount(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessPlayerMount(")? + "ProcessPlayerMount(".len();
        let args_end = line[args_start..].find(')')? + args_start;
        let args = &line[args_start..args_end];

        let parts: Vec<&str> = args.split(',').collect();
        let entity_id: u32 = parts.first()?.trim().parse().ok()?;
        let is_mounting = parts.get(1)?.trim() == "True";

        Some(PlayerEvent::MountStateChanged {
            timestamp: ts,
            entity_id,
            is_mounting,
        })
    }

    /// ProcessSetWeather("WeatherName", boolFlag)
    fn parse_set_weather(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessSetWeather(")? + "ProcessSetWeather(".len();
        let args = &line[args_start..];

        // Extract quoted weather name
        let weather_name = extract_quoted_string(args, 0)?;

        // Find bool after the closing quote
        let after_quote = args.rfind('"')? + 1;
        let rest = &args[after_quote..];
        let last_comma = rest.find(',')?;
        let bool_str = rest[last_comma + 1..].trim().trim_end_matches(')');
        let is_active = bool_str == "True";

        Some(PlayerEvent::WeatherChanged {
            timestamp: ts,
            weather_name,
            is_active,
        })
    }

    /// AreaConfig Setting weather: <Name>
    /// The game stopped emitting ProcessSetWeather (last seen in archived
    /// fixtures) and now logs the active weather through AreaConfig on every
    /// zone-in and weather transition (verified 2026-10-30 live logs). No
    /// active flag on this line: it always states the current weather, so
    /// is_active is true.
    fn parse_area_config_weather(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let marker = "AreaConfig Setting weather:";
        let start = line.find(marker)? + marker.len();
        let weather_name = line[start..].trim().to_string();
        if weather_name.is_empty() {
            return None;
        }

        Some(PlayerEvent::WeatherChanged {
            timestamp: ts,
            weather_name,
            is_active: true,
        })
    }

    fn parse_update_recipe(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessUpdateRecipe(")? + "ProcessUpdateRecipe(".len();
        let args_end = line[args_start..].find(')')? + args_start;
        let args = &line[args_start..args_end];

        let parts: Vec<&str> = args.split(',').collect();
        let recipe_id: u32 = parts.first()?.trim().parse().ok()?;
        let completion_count: u32 = parts.get(1)?.trim().parse().ok()?;

        Some(PlayerEvent::RecipeUpdated {
            timestamp: ts,
            recipe_id,
            completion_count,
        })
    }

    /// ProcessCombatModeStatus(status, System.Int32[])
    fn parse_combat_mode_status(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessCombatModeStatus(")? + "ProcessCombatModeStatus(".len();
        let args = &line[args_start..];

        // First arg before the comma is the status enum
        let first_comma = args.find(',')?;
        let status = args[..first_comma].trim();

        let in_combat = match status {
            "InCombat" => true,
            "NotInCombat" => false,
            _ => return None,
        };

        Some(PlayerEvent::CombatStateChanged {
            timestamp: ts,
            in_combat,
        })
    }

    /// ProcessVendorUpdateAvailableGold(currentGold, serverId, maxGold)
    fn parse_vendor_update_gold(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessVendorUpdateAvailableGold(")?
            + "ProcessVendorUpdateAvailableGold(".len();
        let args_end = line[args_start..].find(')')? + args_start;
        let args = &line[args_start..args_end];

        let parts: Vec<&str> = args.split(',').collect();
        let current_gold: u32 = parts.first()?.trim().parse().ok()?;
        let server_id: u64 = parts.get(1)?.trim().parse().ok()?;
        let max_gold: u32 = parts.get(2)?.trim().parse().ok()?;

        Some(PlayerEvent::VendorGoldChanged {
            timestamp: ts,
            current_gold,
            server_id,
            max_gold,
        })
    }

    /// ProcessSetAttributes(entityId, "[KEY1, KEY2, ...], [val1, val2, ...]")
    fn parse_set_attributes(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessSetAttributes(")? + "ProcessSetAttributes(".len();
        let args = &line[args_start..];

        // Extract entityId (everything before the first comma that precedes the first '[')
        let first_bracket = args.find('[')?;
        let entity_part = &args[..first_bracket];
        // entityId is the first number, separated by ", " from the rest
        let first_comma = entity_part.find(',')?;
        let entity_id: u32 = args[..first_comma].trim().parse().ok()?;

        // Find the keys array: first '[' to first ']'
        let keys_start = first_bracket + 1;
        let keys_end = args[keys_start..].find(']')? + keys_start;
        let keys_str = &args[keys_start..keys_end];

        let keys: Vec<String> = keys_str
            .split(',')
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
            .collect();

        // Find the values array: second '[' to second ']'
        let after_first_array = &args[keys_end + 1..];
        let vals_bracket = after_first_array.find('[')?;
        let vals_start = vals_bracket + 1;
        let vals_end = after_first_array[vals_start..].find(']')? + vals_start;
        let vals_str = &after_first_array[vals_start..vals_end];

        let values: Vec<f32> = vals_str
            .split(',')
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
            .map(|v| v.parse::<f32>())
            .collect::<Result<Vec<_>, _>>()
            .ok()?;

        // Arrays must match in length
        if keys.len() != values.len() {
            return None;
        }

        let attributes: Vec<AttributeValue> = std::iter::zip(keys, values)
            .map(|(name, value)| AttributeValue { name, value })
            .collect();

        Some(PlayerEvent::AttributesChanged {
            timestamp: ts,
            entity_id,
            attributes,
        })
    }
    /// ProcessLoadAbilities(System.Int32[], Skill1, Skill2, AbilityBarContents[])
    /// The Int32[] and AbilityBarContents[] are opaque C# serialized types.
    /// We extract only the two skill names.
    fn parse_load_abilities(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessLoadAbilities(")? + "ProcessLoadAbilities(".len();
        let args_end = line.rfind(')')?;
        let args = &line[args_start..args_end];

        // Format: "System.Int32[], Skill1, Skill2, AbilityBarContents[]"
        let parts: Vec<&str> = args.split(',').map(|s| s.trim()).collect();
        // parts[0] = "System.Int32[]", parts[1] = Skill1, parts[2] = Skill2, parts[3] = "AbilityBarContents[]"
        if parts.len() < 4 {
            return None;
        }

        let skill1 = parts[1].to_string();
        let skill2 = parts[2].to_string();

        Some(PlayerEvent::AbilitiesLoaded {
            timestamp: ts,
            skill1,
            skill2,
        })
    }

    /// ProcessLoadRecipes(System.Int32[], System.Int32[])
    /// Both arrays are opaque C# serialized types — no data extractable.
    /// We emit a signal event with just the timestamp.
    fn parse_load_recipes(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        // Verify the line actually contains the event
        line.find("ProcessLoadRecipes(")?;

        Some(PlayerEvent::RecipesLoaded { timestamp: ts })
    }

    /// ProcessSetEquippedItems(System.Int32[], System.Int32[], System.Int32[], "appearanceString", entityId)
    /// The Int32[] arrays are opaque. We extract the appearance string and entity ID,
    /// then parse equipment slot assignments from the appearance string.
    fn parse_set_equipped_items(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessSetEquippedItems(")? + "ProcessSetEquippedItems(".len();

        // The appearance string is quoted — find it
        let appearance = extract_quoted_string(&line[args_start..], 0)?;

        // Entity ID is after the closing quote + ", "
        let quote_end = line[args_start..].rfind('"')? + args_start;
        let after_quote = &line[quote_end + 1..];
        // after_quote looks like: ", 11921435)"
        let entity_str = after_quote
            .trim()
            .trim_start_matches(',')
            .trim()
            .trim_end_matches(')');
        let entity_id: u32 = entity_str.parse().ok()?;

        // Parse equipment slots from appearance string
        let equipment = parse_equipment_slots(&appearance);

        Some(PlayerEvent::EquipmentChanged {
            timestamp: ts,
            entity_id,
            appearance,
            equipment,
        })
    }

    /// ProcessAddEffects(entityId, sourceEntityId, "[effectId1, effectId2, ...]", boolFlag)
    fn parse_add_effects(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessAddEffects(")? + "ProcessAddEffects(".len();
        let args = &line[args_start..];

        let first_comma = args.find(',')?;
        let entity_id: u32 = args[..first_comma].trim().parse().ok()?;

        let rest = &args[first_comma + 1..];
        let second_comma = rest.find(',')?;
        let source_entity_id: u32 = rest[..second_comma].trim().parse().ok()?;

        // Effect IDs are in "[...]" — may contain quoted brackets like "[302, 303, ...]"
        let bracket_start = rest.find('[')?;
        let bracket_end = rest.find(']')?;
        let ids_str = &rest[bracket_start + 1..bracket_end];

        let effect_ids: Vec<u32> = ids_str
            .split(',')
            .filter_map(|s| s.trim().parse::<u32>().ok())
            .collect();

        // boolFlag is after the "]" bracket — e.g. `", False)` or `], False)`
        let rest_after = &rest[bracket_end + 1..];
        // Strip quotes, commas, parens to isolate the bool token
        let bool_str = rest_after
            .trim()
            .trim_start_matches('"')
            .trim_start_matches(',')
            .trim()
            .trim_end_matches(')')
            .trim_end_matches('"')
            .trim();
        let is_login_batch = bool_str == "False";

        Some(PlayerEvent::EffectsAdded {
            timestamp: ts,
            entity_id,
            source_entity_id,
            effect_ids,
            is_login_batch,
        })
    }

    /// ProcessRemoveEffects(entityId, System.Int32[])
    /// The int array is C#'s opaque ToString() — we can't extract individual IDs.
    fn parse_remove_effects(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessRemoveEffects(")? + "ProcessRemoveEffects(".len();
        let args = &line[args_start..];

        let first_comma = args.find(',')?;
        let entity_id: u32 = args[..first_comma].trim().parse().ok()?;

        Some(PlayerEvent::EffectsRemoved {
            timestamp: ts,
            entity_id,
        })
    }

    /// ProcessUpdateEffectName(entityId, effectInstanceId, "Effect Name, Level N")
    fn parse_update_effect_name(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessUpdateEffectName(")? + "ProcessUpdateEffectName(".len();
        let args = &line[args_start..];

        let first_comma = args.find(',')?;
        let entity_id: u32 = args[..first_comma].trim().parse().ok()?;

        let rest = &args[first_comma + 1..];
        let second_comma = rest.find(',')?;
        let effect_instance_id: u32 = rest[..second_comma].trim().parse().ok()?;

        // Display name is in quotes
        let display_name = extract_quoted_string(rest, 0)?;

        Some(PlayerEvent::EffectNameUpdated {
            timestamp: ts,
            entity_id,
            effect_instance_id,
            display_name,
        })
    }

    /// ProcessUpdateDescription(entityId, "name", "description", "action", actionType, "appearance", flags)
    fn parse_update_description(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start =
            line.find("ProcessUpdateDescription(")? + "ProcessUpdateDescription(".len();
        let args = &line[args_start..];

        // entityId is the first token before the first comma
        let first_comma = args.find(',')?;
        let entity_id: u32 = args[..first_comma].trim().parse().ok()?;

        // Quoted strings: name(0), description(1), action(2), appearance(3)
        let name = extract_quoted_string(args, 0)?;
        let description = extract_quoted_string(args, 1)?;
        let action = extract_quoted_string(args, 2)?;
        let appearance = extract_quoted_string(args, 3)?;

        // actionType is the unquoted token between the 3rd closing quote and 4th opening quote
        // Find the end of the 3rd quoted string (action), then parse the token before the 4th quote
        let action_end = {
            let mut pos = 0;
            for _ in 0..6 {
                // skip 6 quote characters (3 pairs of open+close)
                pos = args[pos..].find('"')? + pos + 1;
            }
            pos
        };
        let before_appearance = &args[action_end..];
        let action_type = before_appearance
            .split('"')
            .next()?
            .trim()
            .trim_matches(',')
            .trim()
            .to_string();

        // flags is the numeric value after the last closing quote
        let last_quote = args.rfind('"')?;
        let after_last = &args[last_quote + 1..];
        let flags_str = after_last
            .trim_start_matches(',')
            .trim()
            .trim_end_matches(')')
            .trim();
        let flags: u32 = flags_str.parse().unwrap_or(0);

        Some(PlayerEvent::EntityDescriptionUpdated {
            timestamp: ts,
            entity_id,
            name,
            description,
            action,
            action_type,
            appearance,
            flags,
        })
    }

    // ── New game state parsers ──────────────────────────────────────

    /// ProcessSetCelestialInfo(WaxingCrescentMoon)
    fn parse_celestial_info(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let start = line.find("ProcessSetCelestialInfo(")? + "ProcessSetCelestialInfo(".len();
        let end = line[start..].find(')')? + start;
        let phase = line[start..end].trim().to_string();
        if phase.is_empty() {
            return None;
        }
        Some(PlayerEvent::MoonPhaseChanged {
            timestamp: ts,
            phase,
        })
    }

    /// ProcessGuildGeneralInfo(guildId, "GuildName", "motd")
    fn parse_guild_general_info(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessGuildGeneralInfo(")? + "ProcessGuildGeneralInfo(".len();
        let args = &line[args_start..];

        let first_comma = args.find(',')?;
        let guild_id: u32 = args[..first_comma].trim().parse().ok()?;

        let guild_name = extract_quoted_string(args, 0)?;
        let motd = extract_quoted_string(args, 1).unwrap_or_default();

        Some(PlayerEvent::GuildInfoLoaded {
            timestamp: ts,
            guild_id,
            guild_name,
            motd,
        })
    }

    /// ProcessCompleteDirectedGoals([3200,8000,1,70,...])
    fn parse_complete_directed_goals(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start =
            line.find("ProcessCompleteDirectedGoals(")? + "ProcessCompleteDirectedGoals(".len();
        let args = &line[args_start..];

        let bracket_start = args.find('[')? + 1;
        let bracket_end = args.find(']')?;
        let ids_str = &args[bracket_start..bracket_end];

        let goal_ids: Vec<u32> = ids_str
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();

        Some(PlayerEvent::DirectedGoalsLoaded {
            timestamp: ts,
            goal_ids,
        })
    }

    /// ProcessSetString(KEY, "value")
    /// Only emits events for known useful keys.
    fn parse_set_string(&self, line: &str) -> Option<PlayerEvent> {
        let ts = parse_timestamp(line)?;
        let args_start = line.find("ProcessSetString(")? + "ProcessSetString(".len();
        let args = &line[args_start..];

        let first_comma = args.find(',')?;
        let key = args[..first_comma].trim().to_string();

        // Only persist known useful keys
        match key.as_str() {
            "NOTEPAD"
            | "NOTEPAD_TAB_1"
            | "NOTEPAD_TAB_2"
            | "NOTEPAD_TAB_3"
            | "NOTEPAD_TAB_4"
            | "NOTEPAD_TAB_NAMES"
            | "FRIEND_STATUS"
            | "PUBLIC_STATUS"
            | "HUNTING_GROUP_TITLE" => {}
            _ => return None,
        }

        let value = extract_quoted_string(args, 0)?;

        Some(PlayerEvent::PlayerStringUpdated {
            timestamp: ts,
            key,
            value,
        })
    }
}

/// Classify a DoDelayLoop into an ActivitySource, or None if the loop isn't
/// a known item-source activity.
///
/// The action_type + label combination is the reliable signal here. Labels
/// have appeared in real logs with minor formatting variations (`"Mining..."`
/// vs `"Mining ..."`) so comparisons must be lenient.
fn classify_delay_loop(
    action_type: &str,
    label: &str,
    pending_interaction: &Option<PendingInteractionMeta>,
) -> Option<ActivitySource> {
    let label_trimmed = label.trim();
    let label_trimmed_lower = label_trimmed.to_lowercase();

    // Mining: "Mining..." (possibly with a trailing space before the dots in
    // older logs). ChopLumber action_type is a reasonable additional filter
    // but we don't require it strictly since label is already diagnostic.
    // Strip spaces and trailing dots to unify variants.
    let label_core: String = label_trimmed_lower
        .trim_end_matches(|c: char| c == '.' || c.is_whitespace())
        .to_string();
    if label_core == "mining" {
        // Adopt node_entity_id / node_name from a recent StartInteraction if
        // any (interaction fires just before the delay loop when present).
        let (node_entity_id, node_name) = match pending_interaction {
            Some(pi) => (Some(pi.entity_id), pi.name.clone()),
            None => (None, None),
        };
        return Some(ActivitySource::Mining {
            node_entity_id,
            node_name,
        });
    }

    // Survey map crafting: label "Surveying" (with the Surveying skill action).
    if label_core == "surveying" {
        return Some(ActivitySource::SurveyMapCraft);
    }

    // Survey map use: label "Using <X> Survey" (or "Using <X> Motherlode Map").
    // Extract the map internal name best-effort from the label.
    if let Some(rest) = label_trimmed.strip_prefix("Using ") {
        let map_name = rest.trim().to_string();
        // Survey or motherlode map-specific suffixes
        if map_name.ends_with(" Survey") || map_name.ends_with(" Motherlode Map") {
            return Some(ActivitySource::SurveyMapUse {
                survey_map_internal_name: Some(map_name),
            });
        }
        // Other "Using X" labels (food, consumables) are generic craft/use
        // events — still worth tracking so gains during consumption are
        // attributable, even if rarely produce items.
        return Some(ActivitySource::GeneralCraft {
            action_type: action_type.to_string(),
            label: label_trimmed.to_string(),
        });
    }

    // Explicit generic-craft action types. These labels are recipe names.
    let craft_actions = [
        "ChopLumber",
        "Cook",
        "Brew",
        "Distill",
        "Refine",
        "Smelt",
        "Weave",
        "Assemble",
        "Forge",
    ];
    if craft_actions.contains(&action_type) && label_core != "mining" {
        return Some(ActivitySource::GeneralCraft {
            action_type: action_type.to_string(),
            label: label_trimmed.to_string(),
        });
    }

    None
}

/// Compare two ActivitySource values for "same kind and same target" — used to
/// collapse repeated DoDelayLoop emissions on the same interaction rather than
/// stacking duplicates.
fn activity_source_matches(a: &ActivitySource, b: &ActivitySource) -> bool {
    match (a, b) {
        (
            ActivitySource::Mining {
                node_entity_id: ea, ..
            },
            ActivitySource::Mining {
                node_entity_id: eb, ..
            },
        ) => ea == eb,
        (ActivitySource::SurveyMapCraft, ActivitySource::SurveyMapCraft) => true,
        (
            ActivitySource::SurveyMapUse {
                survey_map_internal_name: a,
            },
            ActivitySource::SurveyMapUse {
                survey_map_internal_name: b,
            },
        ) => a == b,
        (
            ActivitySource::GeneralCraft {
                action_type: at_a,
                label: l_a,
            },
            ActivitySource::GeneralCraft {
                action_type: at_b,
                label: l_b,
            },
        ) => at_a == at_b && l_a == l_b,
        (
            ActivitySource::CorpseSearch { entity_id: a, .. },
            ActivitySource::CorpseSearch { entity_id: b, .. },
        ) => a == b,
        (
            ActivitySource::VendorBrowsing {
                npc_entity_id: a, ..
            },
            ActivitySource::VendorBrowsing {
                npc_entity_id: b, ..
            },
        ) => a == b,
        (
            ActivitySource::StorageBrowsing {
                vault_owner_entity_id: a,
                ..
            },
            ActivitySource::StorageBrowsing {
                vault_owner_entity_id: b,
                ..
            },
        ) => a == b,
        _ => false,
    }
}

/// Parse equipment slot assignments from an appearance string.
/// Looks for patterns like `@Chest=...;`, `@MainHand=...;`, `MainHandEquip=...;`
fn parse_equipment_slots(appearance: &str) -> Vec<EquipmentSlot> {
    let slot_prefixes = [
        "@Chest=",
        "@Head=",
        "@Legs=",
        "@Feet=",
        "@Hands=",
        "@MainHand=",
        "@OffHandShield=",
        "@Racial=",
    ];
    let equip_type_prefixes = ["MainHandEquip=", "OffHandEquip="];

    let mut slots = Vec::new();

    for prefix in &slot_prefixes {
        if let Some(start) = appearance.find(prefix) {
            let value_start = start + prefix.len();
            // Value runs until the next ';' or ')' at the same nesting depth
            let value = extract_slot_value(&appearance[value_start..]);
            if !value.is_empty() {
                slots.push(EquipmentSlot {
                    slot: prefix
                        .trim_start_matches('@')
                        .trim_end_matches('=')
                        .to_string(),
                    appearance_key: value,
                });
            }
        }
    }

    for prefix in &equip_type_prefixes {
        if let Some(start) = appearance.find(prefix) {
            let value_start = start + prefix.len();
            let value = extract_slot_value(&appearance[value_start..]);
            if !value.is_empty() {
                slots.push(EquipmentSlot {
                    slot: prefix.trim_end_matches('=').to_string(),
                    appearance_key: value,
                });
            }
        }
    }

    slots
}

/// Extract a value from an appearance string starting at a position.
/// Handles nested parentheses: `@eq-f2-chest-steel-02(^Armor=...;Color1=...)`
/// Returns everything up to the `;` or end that closes the value at depth 0.
fn extract_slot_value(s: &str) -> String {
    let mut depth = 0i32;
    let mut end = s.len();

    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    end = i;
                    break;
                }
                depth -= 1;
            }
            ';' if depth == 0 => {
                end = i;
                break;
            }
            _ => {}
        }
    }

    s[..end].to_string()
}

// ============================================================
// Helper Functions
// ============================================================

/// Parse a single {type=X,raw=R,bonus=B,xp=X,tnl=T,max=M} block
fn parse_skill_block(block: &str) -> Option<SkillSnapshot> {
    let skill_type = extract_block_field(block, "type=")?;
    let raw: u32 = extract_block_field(block, "raw=")?.parse().ok()?;
    let bonus: u32 = extract_block_field(block, "bonus=")?.parse().ok()?;
    let xp: u32 = extract_block_field(block, "xp=")?.parse().ok()?;
    let tnl: i32 = extract_block_field(block, "tnl=")?.parse().ok()?;
    let max: u32 = extract_block_field(block, "max=")?.parse().ok()?;

    Some(SkillSnapshot {
        skill_type,
        raw,
        bonus,
        xp,
        tnl,
        max,
    })
}

/// Extract a field value from within a {key=value,...} block
fn extract_block_field(block: &str, key: &str) -> Option<String> {
    let start = block.find(key)? + key.len();
    let rest = &block[start..];
    let end = rest.find([',', '}']).unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

/// Parse the equipment skill bonus from a corpse body, e.g.
/// "...the corpse (with a +12 skill bonus from equipment)..." → 12.
fn parse_equipment_bonus(body: &str) -> Option<u32> {
    let marker = "skill bonus from equipment";
    let marker_pos = body.find(marker)?;
    // Walk back from the marker to the "+" that precedes the number.
    let before = &body[..marker_pos];
    let plus = before.rfind('+')?;
    let digits: String = before[plus + 1..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// Extract the nth positional arg (0-indexed) from a comma-separated log arg
/// list, ignoring commas inside double-quoted strings. Returns the raw
/// untrimmed text of that arg, or None when the list ends first.
fn extract_positional_arg(text: &str, n: usize) -> Option<&str> {
    let mut depth = 0usize; // paren nesting inside args, e.g. List`1[...](...)
    let mut in_quotes = false;
    let mut arg_start = 0usize;
    let mut index = 0usize;
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if in_quotes {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'"' {
                in_quotes = false;
            }
        } else {
            match c {
                b'"' => in_quotes = true,
                b'(' | b'[' => depth += 1,
                b')' | b']' => {
                    depth = depth.saturating_sub(1);
                }
                b',' if depth == 0 => {
                    if index == n {
                        return Some(&text[arg_start..i]);
                    }
                    index += 1;
                    arg_start = i + 1;
                }
                _ => {}
            }
        }
        i += 1;
    }
    if index == n {
        // Final arg runs to end of the scanned text (caller bounds it with ')').
        return Some(&text[arg_start..]);
    }
    None
}

/// Parse the nth positional arg as a Rust-style bool (`True`/`False`).
fn extract_positional_bool(text: &str, n: usize) -> Option<bool> {
    match extract_positional_arg(text, n)?.trim() {
        "True" => Some(true),
        "False" => Some(false),
        _ => None,
    }
}

/// Extract the nth quoted string from text (0-indexed)
fn extract_quoted_string(text: &str, n: usize) -> Option<String> {
    let mut count = 0;
    let mut pos = 0;
    while pos < text.len() {
        if let Some(q_start) = text[pos..].find('"') {
            let abs_start = pos + q_start + 1;
            if let Some(q_end) = text[abs_start..].find('"') {
                if count == n {
                    return Some(text[abs_start..abs_start + q_end].to_string());
                }
                count += 1;
                pos = abs_start + q_end + 1;
            } else {
                break;
            }
        } else {
            break;
        }
    }
    None
}

/// Extract the nth double-quoted string from `text`, honoring backslash escapes
/// (`\"`, `\\`) so an escaped quote inside the string is not mistaken for its
/// terminator. Returns the raw inner content with escapes left intact (use
/// [`unescape_log_string`] to resolve them). Unlike [`extract_quoted_string`],
/// this is safe for free-form text such as poems that may contain quotes.
fn extract_quoted_escaped(text: &str, n: usize) -> Option<String> {
    let bytes = text.as_bytes();
    let mut count = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() {
                match bytes[j] {
                    b'\\' => j += 2, // skip the escaped character
                    b'"' => break,
                    _ => j += 1,
                }
            }
            if j >= bytes.len() {
                return None; // unterminated string
            }
            if count == n {
                return Some(text[start..j].to_string());
            }
            count += 1;
            i = j + 1;
        } else {
            i += 1;
        }
    }
    None
}

/// Resolves \" into real quotes
/// Other escapes are untouched, as other consumers are expecting them
fn unescape_log_quotes(s: &str) -> String {
    if !s.contains("\\\"") {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"') => out.push('"'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Resolve the log's backslash escapes (`\n`, `\t`, `\r`, `\"`, `\\`) into their
/// real characters. Unknown escapes are passed through verbatim.
fn unescape_log_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Parse the gift-cap note that gift-capped (Statehelm) NPCs show:
/// `<Npc> will accept up to <b>5</b> gifts per calendar week and has received
/// <b>2</b> so far this week.` The note is embedded in a ProcessTalkScreen
/// (post-gift) or ProcessPromptForItem (pre-gift) line; either gives the
/// game's authoritative current count, which the gift log is reconciled to.
///
/// Free function (uses no parser state) so the live parser dispatch and the
/// Player.log gift backfill agree line-for-line.
pub fn parse_gift_count_line(line: &str) -> Option<PlayerEvent> {
    let ts = parse_timestamp(line)?;

    let marker = " will accept up to <b>";
    let marker_pos = line.find(marker)?;

    // NPC display name: the word(s) just before " will accept up to". The
    // preceding sentence ends with ". ", so take everything after the last
    // ". " before the marker.
    let before = &line[..marker_pos];
    let name_start = before.rfind(". ").map(|i| i + 2).unwrap_or(0);
    let npc_name = before[name_start..].trim().to_string();
    if npc_name.is_empty() {
        return None;
    }

    // max: the number inside the first <b>…</b> after the marker.
    let after_marker = &line[marker_pos + marker.len()..];
    let max_end = after_marker.find("</b>")?;
    let max: u32 = after_marker[..max_end].trim().parse().ok()?;

    // received: "has received <b>N</b> so far this week"
    let recv_marker = "has received <b>";
    let recv_pos = line.find(recv_marker)?;
    let recv_rest = &line[recv_pos + recv_marker.len()..];
    let recv_end = recv_rest.find("</b>")?;
    let received: u32 = recv_rest[..recv_end].trim().parse().ok()?;

    Some(PlayerEvent::GiftCountObserved {
        timestamp: ts,
        npc_name,
        received,
        max,
    })
}

/// A poem parsed from a `Poem by X` talk-screen line.
pub struct ParsedPoem {
    /// Raw `HH:MM:SS` timestamp from the log line.
    pub timestamp: String,
    pub author: String,
    pub title: String,
    pub content: String,
}

/// Parse a single `Poem by X` ProcessTalkScreen log line into a [`ParsedPoem`],
/// or `None` if the line is not a poem recital.
///
/// Shared by the live parser ([`PlayerEventParser::handle_talk_screen`]) and the
/// one-time Player.log poem backfill, so both stay byte-for-byte consistent.
pub fn parse_poem_line(line: &str) -> Option<ParsedPoem> {
    if !line.contains("ProcessTalkScreen(") {
        return None;
    }
    let timestamp = parse_timestamp(line)?;

    let args_start = line.find("ProcessTalkScreen(")? + "ProcessTalkScreen(".len();
    let args = &line[args_start..];
    let first_comma = args.find(',')?;
    let rest = &args[first_comma + 1..];

    // Title is the first quoted string. Poem titles are "Poem by <name>" and
    // never contain escaped quotes, so the naive scan is safe here.
    let q_start = rest.find('"')? + 1;
    let q_end = rest[q_start..].find('"')? + q_start;
    let title = &rest[q_start..q_end];
    let author = title.strip_prefix("Poem by ")?.trim().to_string();

    // Body is the next quoted string; it may contain `\"`-escaped quotes.
    let after_title = &rest[q_end + 1..];
    let raw_body = extract_quoted_escaped(after_title, 0)?;
    let (poem_title, content) = parse_poem_body(&raw_body)?;

    Some(ParsedPoem {
        timestamp,
        author,
        title: poem_title,
        content,
    })
}

/// Parse the raw body of a `Poem by X` talk screen into `(title, content)`.
///
/// The body has the shape (all on one log line, with `\n` escapes):
/// ```text
/// <i>The following poem was spoken by NAME…</i>\n\n---\n\n<b>TITLE</b>\n\n<poem>\n\n\n---\n\n<i>What did you think…</i>
/// ```
/// The bold `<b>TITLE</b>` is pulled out as the poem's name, the intro/outro
/// review blurbs (`<i>…</i>`) and the `---` fences are dropped, and the
/// remaining stanzas are returned with real newlines. Returns `None` only when
/// both the title and content come out empty.
fn parse_poem_body(raw_body: &str) -> Option<(String, String)> {
    // The bold tag holds the poem's name.
    let title = raw_body
        .find("<b>")
        .and_then(|s| {
            let inner = &raw_body[s + 3..];
            inner.find("</b>").map(|e| inner[..e].trim().to_string())
        })
        .unwrap_or_default();

    // Drop the leading intro blurb (everything up to and including its `</i>`)
    // and the trailing outro blurb (from its opening `<i>` to the end). Player
    // text is HTML-entity escaped by the game, so raw `<i>`/`</i>` only ever
    // come from the game-generated blurbs.
    let mut core = raw_body;
    if let Some(p) = core.find("</i>") {
        core = &core[p + "</i>".len()..];
    }
    if let Some(p) = core.rfind("<i>") {
        core = &core[..p];
    }

    // Remove the bold title line — it's stored separately.
    let core = if title.is_empty() {
        core.to_string()
    } else {
        core.replacen(&format!("<b>{title}</b>"), "", 1)
    };

    // Resolve `\n` (and friends) to real characters, then drop the blank and
    // `---` fence lines that bookend the poem, keeping interior lines intact.
    let unescaped = unescape_log_string(&core);
    let is_boundary = |line: &str| line.trim().is_empty() || line.trim().chars().all(|c| c == '-');
    let lines: Vec<&str> = unescaped.lines().collect();
    let mut start = 0;
    let mut end = lines.len();
    while start < end && is_boundary(lines[start]) {
        start += 1;
    }
    while end > start && is_boundary(lines[end - 1]) {
        end -= 1;
    }
    let content = lines[start..end].join("\n");

    if title.is_empty() && content.is_empty() {
        return None;
    }
    Some((title, content))
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_corpse_extract_captures_skill_equip_anatomy() {
        let mut parser = PlayerEventParser::new();
        // Establish a baseline skill table (Skinning level + others).
        parser.process_line(
            r#"[18:00:00] LocalPlayer: ProcessLoadSkills({type=Skinning,raw=61,bonus=4,xp=10,tnl=100,max=70}, {type=Anatomy_Canines,raw=40,bonus=0,xp=10,tnl=100,max=50})"#,
        );
        // Corpse-search body carries the equipment bonus + skin item.
        parser.process_line(
            r#"[18:00:01] LocalPlayer: ProcessTalkScreen(500, "Search Corpse of Wild Worg", "\nTestPlayer skinned the corpse (with a +12 skill bonus from equipment) and obtained Great Animal Skin", "", System.Int32[], System.String[], 1, Corpse)"#,
        );
        // The skin item is added (no RemoveLoot follows for an extract).
        parser.process_line(
            r#"[18:00:01] LocalPlayer: ProcessAddItem(GreatAnimalSkin(111), -1, True)"#,
        );
        // Anatomy XP for the monster family, then the Skinning XP that triggers
        // the extract emit.
        parser.process_line(
            r#"[18:00:01] LocalPlayer: ProcessUpdateSkill({type=Anatomy_Canines,raw=44,bonus=0,xp=20,tnl=100,max=50}, True, 50, 0, 0)"#,
        );
        let events = parser.process_line(
            r#"[18:00:01] LocalPlayer: ProcessUpdateSkill({type=Skinning,raw=62,bonus=4,xp=30,tnl=100,max=70}, True, 70, 0, 0)"#,
        );

        let extract = events
            .iter()
            .find_map(|e| match e {
                PlayerEvent::CorpseExtract {
                    item_name,
                    skill,
                    skill_level,
                    equipment_bonus,
                    anatomy_family,
                    anatomy_level,
                    ..
                } => Some((
                    item_name.clone(),
                    skill.clone(),
                    *skill_level,
                    *equipment_bonus,
                    anatomy_family.clone(),
                    *anatomy_level,
                )),
                _ => None,
            })
            .expect("a CorpseExtract event");

        assert_eq!(extract.0, "GreatAnimalSkin");
        assert_eq!(extract.1, "Skinning");
        assert_eq!(extract.2, Some(62)); // skill level from the triggering line
        assert_eq!(extract.3, Some(12)); // +12 equipment bonus from corpse body
        assert_eq!(extract.4.as_deref(), Some("Canines"));
        assert_eq!(extract.5, Some(44)); // current anatomy level
    }

    #[test]
    fn test_parse_add_item() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[16:17:48] LocalPlayer: ProcessAddItem(Malachite(115244857), -1, True)"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::ItemAdded {
                item_name,
                instance_id,
                slot_index,
                is_new,
                ..
            } => {
                assert_eq!(item_name, "Malachite");
                assert_eq!(*instance_id, 115244857);
                assert_eq!(*slot_index, -1);
                assert!(*is_new);
            }
            _ => panic!("Expected ItemAdded"),
        }
    }

    #[test]
    fn test_parse_add_item_login_load() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[16:00:00] LocalPlayer: ProcessAddItem(MetalSlab2(136937342), 5, False)"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::ItemAdded {
                item_name,
                is_new,
                slot_index,
                ..
            } => {
                assert_eq!(item_name, "MetalSlab2");
                assert!(!is_new);
                assert_eq!(*slot_index, 5);
            }
            _ => panic!("Expected ItemAdded"),
        }
    }

    #[test]
    fn test_parse_update_item_code_with_delta() {
        let mut parser = PlayerEventParser::new();

        // First register the item
        parser.process_line(
            r#"[16:00:00] LocalPlayer: ProcessAddItem(MetalSlab3(136937342), 5, False)"#,
        );
        // Set initial stack size
        parser.stack_sizes.insert(136937342, 20);

        let events = parser.process_line(
            r#"[16:17:48] LocalPlayer: ProcessUpdateItemCode(136937342, 1642723, True)"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::ItemStackChanged {
                instance_id,
                item_name,
                item_type_id,
                old_stack_size,
                new_stack_size,
                delta,
                from_server,
                ..
            } => {
                assert_eq!(*instance_id, 136937342);
                assert_eq!(item_name.as_deref(), Some("MetalSlab3"));
                assert_eq!(*item_type_id, 4323); // 1642723 & 0xFFFF
                assert_eq!(*old_stack_size, 20);
                assert_eq!(*new_stack_size, 26); // (1642723 >> 16) + 1; encoding is 0-based
                assert_eq!(*delta, 6);
                assert!(*from_server);
            }
            _ => panic!("Expected ItemStackChanged"),
        }
    }

    #[test]
    fn test_encoded_value_decoding() {
        // Encoding is 0-based: actual_stack = (encoded >> 16) + 1
        let encoded: u32 = 1642723;
        assert_eq!((encoded >> 16) + 1, 26); // actual stack size
        assert_eq!(encoded & 0xFFFF, 4323); // item type ID
    }

    #[test]
    fn test_delete_then_storage_transfer() {
        let mut parser = PlayerEventParser::new();

        // Register item first
        parser.process_line(
            r#"[13:27:07] LocalPlayer: ProcessAddItem(MapleWood(136093889), 10, False)"#,
        );

        // Delete item — should be buffered
        let events = parser.process_line(r#"[13:27:07] LocalPlayer: ProcessDeleteItem(136093889)"#);
        assert!(events.is_empty(), "Delete should be buffered");

        // Storage deposit — should resolve the pending delete
        let events = parser.process_line(
            r#"[13:27:07] LocalPlayer: ProcessAddToStorageVault(14804, -1, 40, MapleWood(136093889))"#
        );

        // Should have: ItemDeleted(StorageTransfer) + StorageDeposit
        assert_eq!(events.len(), 2);
        match &events[0] {
            PlayerEvent::ItemDeleted {
                instance_id,
                context,
                item_name,
                ..
            } => {
                assert_eq!(*instance_id, 136093889);
                assert_eq!(*context, DeleteContext::StorageTransfer);
                assert_eq!(item_name.as_deref(), Some("MapleWood"));
            }
            _ => panic!("Expected ItemDeleted with StorageTransfer"),
        }
        match &events[1] {
            PlayerEvent::StorageDeposit {
                item_name,
                instance_id,
                ..
            } => {
                assert_eq!(item_name, "MapleWood");
                assert_eq!(*instance_id, 136093889);
            }
            _ => panic!("Expected StorageDeposit"),
        }
    }

    #[test]
    fn test_delete_then_vendor_sale() {
        let mut parser = PlayerEventParser::new();

        parser.process_line(
            r#"[16:32:25] LocalPlayer: ProcessAddItem(AmuletOfCrushingMitigation5(115259296), 3, False)"#
        );

        let events = parser.process_line(r#"[16:32:25] LocalPlayer: ProcessDeleteItem(115259296)"#);
        assert!(events.is_empty());

        let events = parser.process_line(
            r#"[16:32:25] LocalPlayer: ProcessVendorAddItem(120, AmuletOfCrushingMitigation5(115259296), False)"#
        );
        assert_eq!(events.len(), 2);
        match &events[0] {
            PlayerEvent::ItemDeleted { context, .. } => {
                assert_eq!(*context, DeleteContext::VendorSale);
            }
            _ => panic!("Expected ItemDeleted with VendorSale"),
        }
        match &events[1] {
            PlayerEvent::VendorSold {
                price,
                item_name,
                is_buyback,
                ..
            } => {
                assert_eq!(*price, 120);
                assert_eq!(item_name, "AmuletOfCrushingMitigation5");
                assert!(!is_buyback);
            }
            _ => panic!("Expected VendorSold"),
        }
    }

    #[test]
    fn test_delete_standalone_flushed_as_unknown() {
        let mut parser = PlayerEventParser::new();

        parser.process_line(
            r#"[16:33:03] LocalPlayer: ProcessAddItem(SomeItem(114961794), 1, False)"#,
        );

        // Delete
        let events = parser.process_line(r#"[16:33:03] LocalPlayer: ProcessDeleteItem(114961794)"#);
        assert!(events.is_empty());

        // One unrelated line does NOT flush: the buffer retains up to 5 s /
        // 5 lines for a storage/vendor confirmation.
        let events = parser
            .process_line(r#"[16:33:04] entity_159956: OnAttackHitMe(Fiery Bite). Evaded = False"#);
        assert!(events.is_empty(), "delete stays buffered within retention");

        // Six unrelated lines past the delete exhaust the 5-line retention.
        let mut flushed = Vec::new();
        for i in 1..=6u32 {
            let line = format!(
                "[16:33:{:02}] entity_159956: OnAttackHitMe(Fiery Bite). Evaded = False",
                4 + i
            );
            flushed.extend(parser.process_line(&line));
        }
        assert_eq!(flushed.len(), 1);
        match &flushed[0] {
            PlayerEvent::ItemDeleted {
                instance_id,
                context,
                item_name,
                ..
            } => {
                assert_eq!(*instance_id, 114961794);
                assert_eq!(*context, DeleteContext::Unknown);
                assert_eq!(item_name.as_deref(), Some("SomeItem"));
            }
            _ => panic!("Expected ItemDeleted with Unknown"),
        }
    }

    #[test]
    fn test_delete_survives_unrelated_line_for_storage_match() {
        // The 1-line lookahead misclassified vault deposits with an unrelated
        // line in between. The buffer retains up to 5 s / 5 lines, so
        // Delete, unrelated, AddToStorageVault still resolves StorageTransfer.
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[13:27:07] LocalPlayer: ProcessAddItem(MapleWood(136093889), -1, True)"#,
        );
        assert!(parser
            .process_line(r#"[13:27:07] LocalPlayer: ProcessDeleteItem(136093889)"#)
            .is_empty());
        // Unrelated line in between — buffered, not flushed.
        assert!(parser
            .process_line(r#"[13:27:08] entity_159956: OnAttackHitMe(Fiery Bite). Evaded = False"#)
            .is_empty());
        let events = parser.process_line(
            r#"[13:27:08] LocalPlayer: ProcessAddToStorageVault(14804, -1, 40, MapleWood(136093889))"#,
        );
        assert_eq!(events.len(), 2);
        match &events[0] {
            PlayerEvent::ItemDeleted { context, .. } => {
                assert_eq!(*context, DeleteContext::StorageTransfer);
            }
            other => panic!("Expected ItemDeleted, got {other:?}"),
        }
        assert!(matches!(events[1], PlayerEvent::StorageDeposit { .. }));
    }

    #[test]
    fn test_login_batch_run_of_25_arms_one_shot() {
        // 25 consecutive `is_new=false` AddItems with no UpdateItemCode is a
        // login full-state dump: the one-shot fires exactly once.
        let mut parser = PlayerEventParser::new();
        for i in 0..25u64 {
            let line = format!(
                "[16:00:{:02}] LocalPlayer: ProcessAddItem(DumpItem{}(1000{}), -1, False)",
                i % 60,
                i,
                i
            );
            for e in parser.process_line(&line) {
                assert!(
                    !matches!(e, PlayerEvent::ItemDeleted { .. }),
                    "no deletes in this burst"
                );
            }
        }
        assert!(
            parser.take_inventory_login_batch(),
            "25-burst must arm the one-shot"
        );
        assert!(
            !parser.take_inventory_login_batch(),
            "one-shot consumes on take"
        );
    }

    #[test]
    fn test_login_batch_does_not_rearm_mid_burst() {
        // Finding A regression: after the coordinator takes the one-shot,
        // the next `is_new=false` line in the SAME burst must not re-arm —
        // a second take would clear the slice after the first half persisted.
        let mut parser = PlayerEventParser::new();
        for i in 0..25u64 {
            parser.process_line(&format!(
                "[16:00:{:02}] LocalPlayer: ProcessAddItem(DumpItem{}(1000{}), -1, False)",
                i % 60,
                i,
                i
            ));
        }
        assert!(
            parser.take_inventory_login_batch(),
            "25-burst must arm the one-shot"
        );
        // The burst continues (e.g. a dump split across polls): lines 25..30
        // must not re-arm the flag.
        for i in 25..30u64 {
            parser.process_line(&format!(
                "[16:01:{:02}] LocalPlayer: ProcessAddItem(DumpItem{}(1000{}), -1, False)",
                i % 60,
                i,
                i
            ));
        }
        assert!(
            !parser.take_inventory_login_batch(),
            "mid-burst continuation must not re-arm after take"
        );
        // A genuine gain ends the burst; a fresh 20-run after it arms again.
        parser.process_line(
            r#"[16:02:00] LocalPlayer: ProcessAddItem(FreshItem(9), -1, True)"#,
        );
        for i in 0..20u64 {
            parser.process_line(&format!(
                "[16:02:{:02}] LocalPlayer: ProcessAddItem(DumpItemB{}(3000{}), -1, False)",
                i % 60,
                i,
                i
            ));
        }
        assert!(
            parser.take_inventory_login_batch(),
            "new burst after a genuine gain must arm again"
        );
    }

    #[test]
    fn test_idle_delete_flushes_after_wall_clock_timeout() {
        // Finding B regression: a delete buffered at the end of a poll must
        // not linger forever when no new lines arrive (log-clock and
        // line-count retention both frozen). The wall-clock backstop evicts
        // it as `Unknown`.
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[13:27:07] LocalPlayer: ProcessAddItem(MapleWood(136093889), -1, True)"#,
        );
        assert!(parser
            .process_line(r#"[13:27:07] LocalPlayer: ProcessDeleteItem(136093889)"#)
            .is_empty());
        let mut drained = Vec::new();
        parser.flush_expired_deletes_at(
            parser.last_line_secs(),
            std::time::Instant::now() + std::time::Duration::from_secs(31),
            &mut drained,
        );
        assert_eq!(drained.len(), 1, "idle delete must flush as Unknown");
        match &drained[0] {
            PlayerEvent::ItemDeleted { context, .. } => {
                assert_eq!(*context, DeleteContext::Unknown);
            }
            other => panic!("Expected ItemDeleted, got {other:?}"),
        }
    }

    #[test]
    fn test_short_burst_does_not_arm_login_batch() {
        // 3 `is_new=false` AddItems is live play, not a dump.
        let mut parser = PlayerEventParser::new();
        for i in 0..3u64 {
            parser.process_line(&format!(
                "[16:00:0{}] LocalPlayer: ProcessAddItem(LiveItem{}(2000{}), -1, False)",
                i, i, i
            ));
        }
        assert!(
            !parser.take_inventory_login_batch(),
            "3-burst must not arm the one-shot"
        );
        // An UpdateItemCode resets the run first; 19 more after it still
        // fall short of the 20 threshold.
        parser.process_line(r#"[16:00:22] LocalPlayer: ProcessUpdateItemCode(20003, 0, True)"#);
        for i in 0..19u64 {
            parser.process_line(&format!(
                "[16:01:{:02}] LocalPlayer: ProcessAddItem(LiveItem{}(2000{}), -1, False)",
                i, i, i
            ));
        }
        assert!(
            !parser.take_inventory_login_batch(),
            "UpdateItemCode must reset the run"
        );
    }

    #[test]
    fn test_chat_gain_seed_counts_balance() {
        // Metric invariant: matched + fallback == new-item count.
        let mut parser = PlayerEventParser::new();
        parser.feed_chat_gain("Fluorite".to_string(), 3, "16:00:01");
        parser.process_line(r#"[16:00:01] LocalPlayer: ProcessAddItem(Fluorite(9001), -1, True)"#);
        parser.process_line(r#"[16:00:05] LocalPlayer: ProcessAddItem(Pebbles(9012), -1, True)"#);
        let (matched, fallback) = parser.chat_gain_counts_for_test();
        assert_eq!((matched, fallback), (1, 1));
    }

    #[test]
    fn test_poll_drain_keeps_young_deletes_for_next_poll() {
        // P2: the live poll-end drain must be retention-aware. A delete
        // buffered at the end of poll N must survive into poll N+1 for its
        // confirmation; only expired entries flush as Unknown.
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[13:27:07] LocalPlayer: ProcessAddItem(MapleWood(136093889), -1, True)"#,
        );
        assert!(parser
            .process_line(r#"[13:27:07] LocalPlayer: ProcessDeleteItem(136093889)"#)
            .is_empty());
        // Poll N ends: nothing expired (age 0, 0 lines seen beyond the
        // delete itself) — the young delete survives.
        let mut drained = Vec::new();
        parser.flush_expired_deletes(parser.last_line_secs(), &mut drained);
        assert!(drained.is_empty(), "young delete survives the poll boundary");
        // Poll N+1 opens with the vault confirmation — still resolves.
        let events = parser.process_line(
            r#"[13:27:08] LocalPlayer: ProcessAddToStorageVault(14804, -1, 40, MapleWood(136093889))"#,
        );
        assert_eq!(events.len(), 2);
        match &events[0] {
            PlayerEvent::ItemDeleted { context, .. } => {
                assert_eq!(*context, DeleteContext::StorageTransfer);
            }
            other => panic!("Expected ItemDeleted, got {other:?}"),
        }
    }

    /// A `ProcessDeleteItem` still buffered when the login line arrives must be
    /// emitted *before* the anchor. `LogDateResolver` is stateful and the batch
    /// is replayed in order, so a delete emitted after the anchor resolves its
    /// earlier time of day as a UTC-midnight crossing and dates every following
    /// event a day ahead.
    #[test]
    fn test_delete_buffered_at_the_login_line_flushes_ahead_of_the_anchor() {
        let mut parser = PlayerEventParser::new();

        parser.process_line(
            r#"[22:00:00] LocalPlayer: ProcessAddItem(SomeItem(114961794), 1, False)"#,
        );
        assert!(
            parser
                .process_line(r#"[22:00:01] LocalPlayer: ProcessDeleteItem(114961794)"#)
                .is_empty(),
            "the delete is buffered for context resolution"
        );

        let events = parser.process_line(
            "[22:43:41] Logged in as character TwinkleofToes. Time UTC=10/14/2026 22:43:41. Timezone Offset -05:00:00",
        );
        assert_eq!(
            events.len(),
            2,
            "the buffered delete, then the session anchor"
        );
        assert!(
            matches!(events[0], PlayerEvent::ItemDeleted { .. }),
            "the delete precedes the anchor in log order"
        );
        assert!(matches!(events[1], PlayerEvent::LogSessionAnchored { .. }));
    }

    #[test]
    fn test_parse_load_skills() {
        let mut parser = PlayerEventParser::new();
        let line = r#"[16:00:53] LocalPlayer: ProcessLoadSkills({type=Hammer,raw=70,bonus=5,xp=0,tnl=1153715,max=70},{type=Mentalism,raw=76,bonus=0,xp=2353127,tnl=2502977,max=80},{type=Gourmand,raw=49,bonus=0,xp=835,tnl=2500,max=100})"#;
        let events = parser.process_line(line);

        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::SkillsLoaded { skills, .. } => {
                assert_eq!(skills.len(), 3);
                assert_eq!(skills[0].skill_type, "Hammer");
                assert_eq!(skills[0].raw, 70);
                assert_eq!(skills[0].bonus, 5);
                assert_eq!(skills[0].tnl, 1153715);
                assert_eq!(skills[1].skill_type, "Mentalism");
                assert_eq!(skills[1].xp, 2353127);
                assert_eq!(skills[2].skill_type, "Gourmand");
                assert_eq!(skills[2].max, 100);
            }
            _ => panic!("Expected SkillsLoaded"),
        }
    }

    #[test]
    fn test_parse_load_skills_negative_tnl() {
        let mut parser = PlayerEventParser::new();
        let line = r#"[16:00:53] LocalPlayer: ProcessLoadSkills({type=Compassion,raw=50,bonus=0,xp=0,tnl=-1,max=50})"#;
        let events = parser.process_line(line);

        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::SkillsLoaded { skills, .. } => {
                assert_eq!(skills.len(), 1);
                assert_eq!(skills[0].tnl, -1);
            }
            _ => panic!("Expected SkillsLoaded"),
        }
    }

    #[test]
    fn test_parse_start_interaction() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[13:26:55] LocalPlayer: ProcessStartInteraction(14804, 7, 1200, True, "NPC_Qatik")"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::InteractionStarted {
                entity_id,
                interaction_type,
                npc_name,
                ..
            } => {
                assert_eq!(*entity_id, 14804);
                assert_eq!(*interaction_type, 7);
                assert_eq!(npc_name, "NPC_Qatik");
            }
            _ => panic!("Expected InteractionStarted"),
        }
    }

    #[test]
    fn test_parse_delta_favor() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[16:33:03] LocalPlayer: ProcessDeltaFavor(9618, "NPC_Kalaba", 2.8476, True)"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::FavorChanged {
                npc_id,
                npc_name,
                delta,
                is_gift,
                ..
            } => {
                assert_eq!(*npc_id, 9618);
                assert_eq!(npc_name, "NPC_Kalaba");
                assert!((delta - 2.8476).abs() < 0.001);
                assert!(*is_gift);
            }
            _ => panic!("Expected FavorChanged"),
        }
    }

    #[test]
    fn test_parse_login_line_anchors_the_session_date() {
        let mut parser = PlayerEventParser::new();
        // The real line: bracket time and `Time UTC=` are the same instant.
        let events = parser.process_line(
            r#"[22:43:41] Logged in as character TwinkleofToes. Time UTC=10/14/2026 22:43:41. Timezone Offset -05:00:00"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::LogSessionAnchored { utc } => {
                assert_eq!(utc.to_string(), "2026-10-14 22:43:41");
            }
            other => panic!("Expected LogSessionAnchored, got {other:?}"),
        }

        // A line without the declared UTC time anchors nothing.
        assert!(parser
            .process_line("[12:00:00] Logged in as character [Zenith]")
            .is_empty());
    }

    #[test]
    fn test_parse_gift_count_from_talk_screen() {
        let mut parser = PlayerEventParser::new();
        // Post-gift confirmation: bulk-gifting a stack jumps the count straight to 5.
        let line = r#"[04:31:04] LocalPlayer: ProcessTalkScreen(25493, "", "Oh, thanks!\n\n<i>[Note: Statehelm townsfolk are more sophisticated than villagers elsewhere. It would be uncouth to receive too many gifts per week -- Statehelm villagers value quality over quantity. Corinth will accept up to <b>5</b> gifts per calendar week and has received <b>5</b> so far this week. (The week resets on Monday EDT.)]</i>", "", [-1201,], System.String[], -1301, Generic)"#;
        let events = parser.process_line(line);
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::GiftCountObserved {
                npc_name,
                received,
                max,
                ..
            } => {
                assert_eq!(npc_name, "Corinth");
                assert_eq!(*received, 5);
                assert_eq!(*max, 5);
            }
            _ => panic!("Expected GiftCountObserved"),
        }
    }

    #[test]
    fn test_parse_gift_count_multiword_name_pre_gift() {
        let mut parser = PlayerEventParser::new();
        // Pre-gift prompt with a multi-word NPC name and a partial count.
        let line = r#"[04:25:20] LocalPlayer: ProcessPromptForItem(31486, "Give Gift", "A gift? For me?\n\n<i>[Note: Statehelm villagers value quality over quantity. Sir Brooker will accept up to <b>5</b> gifts per calendar week and has received <b>2</b> so far this week. (The week resets on Monday EDT.)]</i>", "Choose gift", null, [-1201,], System.String[], -1301, "", Error, 0, ForNpc, "NPC_SirBrooker")"#;
        let events = parser.process_line(line);
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::GiftCountObserved {
                npc_name,
                received,
                max,
                ..
            } => {
                assert_eq!(npc_name, "Sir Brooker");
                assert_eq!(*received, 2);
                assert_eq!(*max, 5);
            }
            _ => panic!("Expected GiftCountObserved"),
        }
    }

    #[test]
    fn test_parse_screen_text() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[17:37:01] LocalPlayer: ProcessScreenText(ImportantInfo, "The treasure is 342 meters from here.")"#
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::ScreenText {
                category, message, ..
            } => {
                assert_eq!(category, "ImportantInfo");
                assert_eq!(message, "The treasure is 342 meters from here.");
            }
            _ => panic!("Expected ScreenText"),
        }
    }

    #[test]
    fn test_parse_remove_from_storage() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[13:28:48] LocalPlayer: ProcessRemoveFromStorageVault(14804, -1, 132702881, 11)"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::StorageWithdrawal {
                npc_id,
                instance_id,
                quantity,
                ..
            } => {
                assert_eq!(*npc_id, 14804);
                assert_eq!(*instance_id, 132702881);
                assert_eq!(*quantity, 11);
            }
            _ => panic!("Expected StorageWithdrawal"),
        }
    }

    #[test]
    fn test_parse_vendor_update_item() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[16:32:27] LocalPlayer: ProcessVendorUpdateItem(115249145, 200909, 7)"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::VendorStackUpdated {
                instance_id,
                item_type_id,
                new_stack_size,
                price,
                ..
            } => {
                assert_eq!(*instance_id, 115249145);
                // (200909 >> 16) + 1 = 4, 200909 & 0xFFFF = 4301
                assert_eq!(*new_stack_size, 4);
                assert_eq!(*item_type_id, 4301);
                assert_eq!(*price, 7);
            }
            _ => panic!("Expected VendorStackUpdated"),
        }
    }

    #[test]
    fn test_parse_book() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[13:26:04] LocalPlayer: ProcessBook("Yesterday's Shop Logs", "Toncom bought Guava x5", "PlayerShopLog", "", "", False, False, False, False, False, "")"#
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::BookOpened {
                title,
                content,
                book_type,
                ..
            } => {
                assert_eq!(title, "Yesterday's Shop Logs");
                assert_eq!(content, "Toncom bought Guava x5");
                assert_eq!(book_type, "PlayerShopLog");
            }
            _ => panic!("Expected BookOpened"),
        }
    }

    /// Item names containing \" must resolve correctly
    #[test]
    fn test_parse_book_hoplology_with_escaped_quotes() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[19:00:01] LocalPlayer: ProcessBook("Skill Info: Hoplology", "Equipment Studied:\n\n  \"Nice\" Sword\n  Thentree Harness\n  Bob's \"Lucky\" Helm\n", "SkillReport", "", "", False, False, False, False, False, "")"#
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::BookOpened {
                title,
                content,
                book_type,
                ..
            } => {
                assert_eq!(title, "Skill Info: Hoplology");
                assert_eq!(book_type, "SkillReport");
                // Quotes resolved, newlines still escaped.
                assert_eq!(
                    content,
                    "Equipment Studied:\\n\\n  \"Nice\" Sword\\n  Thentree Harness\\n  Bob's \"Lucky\" Helm\\n"
                );
                let items: Vec<&str> = content
                    .split("\\n")
                    .map(str::trim)
                    .filter(|l| !l.is_empty() && !l.starts_with("Equipment Studied:"))
                    .collect();
                assert_eq!(
                    items,
                    vec!["\"Nice\" Sword", "Thentree Harness", "Bob's \"Lucky\" Helm"]
                );
            }
            _ => panic!("Expected BookOpened"),
        }
    }

    /// Live ProcessBook content carries escaped newlines; the coordinator's
    /// bind extraction must see real line breaks after normalization
    /// (historically it split on '\n' and silently matched no fields).
    #[test]
    fn test_parse_book_bind_report_keeps_escaped_newlines() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[19:00:01] LocalPlayer: ProcessBook("Skill Info: Teleportation", "Teleportation Status:\n\nPrimary Bind Location: Serbule\nSecondary Bind Location: (none)\n", "SkillReport", "", "", False, False, False, False, False, "")"#
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::BookOpened {
                content, book_type, ..
            } => {
                assert_eq!(book_type, "SkillReport");
                // Content is NOT pre-unescaped by the parser — the coordinator
                // normalizes. Prove both: raw is escaped; after the same
                // normalize the coordinator runs, bind fields resolve.
                assert!(content.contains("\\n"));
                let normalized = content.replace("\\n", "\n");
                let primary = normalized
                    .lines()
                    .map(str::trim)
                    .find_map(|l| l.strip_prefix("Primary Bind Location:"))
                    .map(str::trim)
                    .map(str::to_string);
                assert_eq!(primary.as_deref(), Some("Serbule"));
            }
            _ => panic!("Expected BookOpened"),
        }
    }

    #[test]
    fn test_non_player_line_ignored() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[00:15:46] entity_192620: OnAttackHitMe(Spider Bite). Evaded = False"#,
        );
        assert!(events.is_empty());
    }

    #[test]
    fn test_flush_all_pending() {
        let mut parser = PlayerEventParser::new();
        parser.process_line(r#"[16:00:00] LocalPlayer: ProcessAddItem(TestItem(12345), 1, False)"#);
        parser.process_line(r#"[16:33:03] LocalPlayer: ProcessDeleteItem(12345)"#);
        // No more lines — flush manually
        let events = parser.flush_all_pending();
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::ItemDeleted {
                instance_id,
                context,
                ..
            } => {
                assert_eq!(*instance_id, 12345);
                assert_eq!(*context, DeleteContext::Unknown);
            }
            _ => panic!("Expected ItemDeleted"),
        }
    }

    #[test]
    fn test_delete_then_vendor_update_item() {
        let mut parser = PlayerEventParser::new();

        parser.process_line(
            r#"[16:32:27] LocalPlayer: ProcessAddItem(SomeStackable(115271948), 1, False)"#,
        );

        // Delete
        let events = parser.process_line(r#"[16:32:27] LocalPlayer: ProcessDeleteItem(115271948)"#);
        assert!(events.is_empty());

        // VendorUpdateItem (selling stackable vendor already has)
        let events = parser.process_line(
            r#"[16:32:27] LocalPlayer: ProcessVendorUpdateItem(115249145, 200909, 7)"#,
        );
        assert_eq!(events.len(), 2);
        match &events[0] {
            PlayerEvent::ItemDeleted { context, .. } => {
                assert_eq!(*context, DeleteContext::VendorSale);
            }
            _ => panic!("Expected ItemDeleted with VendorSale"),
        }
    }

    #[test]
    fn test_negative_instance_id_add_and_delete_resolve() {
        // Regression: Project: Gorgon item instance IDs are signed 32-bit and
        // are frequently negative. The parser used to read them as u64, which
        // failed the parse and silently dropped the entire ProcessAddItem /
        // ProcessDeleteItem line — losing ~half of all item events and making
        // survey tracking fire only intermittently. These are the exact lines
        // from a real Serbule Rubywall geology survey use.
        let mut parser = PlayerEventParser::new();

        // Survey map enters inventory (crafted) — negative instance id.
        parser.process_line(
            r#"[23:54:27] LocalPlayer: ProcessAddItem(GeologySurveySerbule0(-1796085135), -1, True)"#,
        );

        // Survey map consumed (used) — same negative instance id. Buffered.
        let events =
            parser.process_line(r#"[23:55:24] LocalPlayer: ProcessDeleteItem(-1796085135)"#);
        assert!(events.is_empty(), "delete is buffered until the next line");

        // Within retention the delete stays buffered; lines past it flush.
        let events =
            parser.process_line(r#"[23:55:24] LocalPlayer: ProcessScreenText(ImportantInfo, "x")"#);
        assert!(
            events.iter().all(|e| !matches!(e, PlayerEvent::ItemDeleted { .. })),
            "delete stays buffered within retention"
        );
        let mut flushed = Vec::new();
        for _ in 0..6 {
            flushed.extend(
                parser.process_line(r#"[23:55:25] LocalPlayer: ProcessScreenText(ImportantInfo, "x")"#),
            );
        }
        let deleted = flushed
            .iter()
            .find_map(|e| match e {
                PlayerEvent::ItemDeleted {
                    item_name,
                    instance_id,
                    ..
                } => Some((item_name.clone(), *instance_id)),
                _ => None,
            })
            .expect("expected an ItemDeleted event");

        // Name must resolve from the registry seeded by the negative-id add,
        // and the instance id must round-trip via the bit-preserving cast.
        assert_eq!(deleted.0.as_deref(), Some("GeologySurveySerbule0"));
        assert_eq!(deleted.1, -1796085135i64 as u64);
    }

    #[test]
    fn test_parse_delay_loop_surveying() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[00:08:36] LocalPlayer: ProcessDoDelayLoop(5, UseTeleportationCircle, "Surveying", 5305, AbortIfAttacked)"#
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::DelayLoopStarted {
                duration,
                action_type,
                label,
                entity_id,
                abort_condition,
                ..
            } => {
                assert!((duration - 5.0).abs() < 0.01);
                assert_eq!(action_type, "UseTeleportationCircle");
                assert_eq!(label, "Surveying");
                assert_eq!(*entity_id, 5305);
                assert_eq!(abort_condition, "AbortIfAttacked");
            }
            _ => panic!("Expected DelayLoopStarted"),
        }
    }

    #[test]
    fn test_parse_delay_loop_using_survey() {
        // Fixture-backed: "Using Eltibule Blue Mineral Survey" x216 in
        // 100x-eltblue Player.log; "Using Povus Marvelous Mining Survey"
        // x157 in 50x-povusmarvelous Player.log.
        for label in [
            "Using Eltibule Green Mineral Survey",
            "Using Eltibule Blue Mineral Survey",
            "Using Povus Marvelous Mining Survey",
        ] {
            let mut parser = PlayerEventParser::new();
            let line = format!(
                "[16:17:47] LocalPlayer: ProcessDoDelayLoop(0.5, Unset, \"{}\", 5305, AbortIfAttacked)",
                label
            );
            let events = parser.process_line(&line);
            assert_eq!(events.len(), 1);
            match &events[0] {
                PlayerEvent::DelayLoopStarted {
                    duration,
                    action_type,
                    label: got,
                    ..
                } => {
                    assert!((duration - 0.5).abs() < 0.01);
                    assert_eq!(action_type, "Unset");
                    assert_eq!(got, label);
                }
                _ => panic!("Expected DelayLoopStarted"),
            }
            assert!(matches!(
                classify_delay_loop("Unset", label, &None),
                Some(ActivitySource::SurveyMapUse { .. })
            ));
        }
        // "Using <X> Motherlode Map" is the same activity (120x in fixtures).
        assert!(matches!(
            classify_delay_loop("Unset", "Using Serbule Motherlode Map", &None),
            Some(ActivitySource::SurveyMapUse {
                survey_map_internal_name: Some(_)
            })
        ));
    }

    #[test]
    fn test_parse_delay_loop_eating() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[00:08:00] LocalPlayer: ProcessDoDelayLoop(1.5, Eat, "Using Gobbledygook", 6223, AbortIfAttacked)"#
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::DelayLoopStarted {
                action_type,
                label,
                entity_id,
                ..
            } => {
                assert_eq!(action_type, "Eat");
                assert_eq!(label, "Using Gobbledygook");
                assert_eq!(*entity_id, 6223);
            }
            _ => panic!("Expected DelayLoopStarted"),
        }
    }

    #[test]
    fn test_instance_registry_baseline_emits_correction() {
        let mut parser = PlayerEventParser::new();
        // Existing item loaded at session start (is_new=False)
        parser.process_line(
            r#"[16:00:00] LocalPlayer: ProcessAddItem(MetalSlab2(136937342), 5, False)"#,
        );

        // First UpdateItemCode establishes baseline — emits delta=0 so the DB
        // gets the correct absolute stack size (AddItem only wrote stack_size=1).
        let events = parser.process_line(
            r#"[16:00:01] LocalPlayer: ProcessUpdateItemCode(136937342, 65536, True)"#,
        );
        assert_eq!(
            events.len(),
            1,
            "First UpdateItemCode should emit a baseline correction"
        );
        match &events[0] {
            PlayerEvent::ItemStackChanged {
                item_name,
                old_stack_size,
                new_stack_size,
                delta,
                provenance,
                ..
            } => {
                assert_eq!(item_name.as_deref(), Some("MetalSlab2"));
                assert_eq!(*new_stack_size, 2); // (65536 >> 16) + 1
                assert_eq!(*old_stack_size, 2); // no prior → old = new - 0
                assert_eq!(*delta, 0); // baseline, not a gain
                assert_eq!(*provenance, ItemProvenance::NotApplicable);
            }
            _ => panic!("Expected ItemStackChanged"),
        }

        // Subsequent UpdateItemCode DOES emit a real change event
        let events = parser.process_line(
            r#"[16:00:02] LocalPlayer: ProcessUpdateItemCode(136937342, 196608, True)"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::ItemStackChanged {
                item_name,
                old_stack_size,
                new_stack_size,
                delta,
                ..
            } => {
                assert_eq!(item_name.as_deref(), Some("MetalSlab2"));
                assert_eq!(*old_stack_size, 2); // (65536 >> 16) + 1
                assert_eq!(*new_stack_size, 4); // (196608 >> 16) + 1
                assert_eq!(*delta, 2);
            }
            _ => panic!("Expected ItemStackChanged"),
        }
    }

    #[test]
    fn test_new_item_seeds_stack_and_emits_change() {
        let mut parser = PlayerEventParser::new();
        // Genuinely new item (is_new=True, slot=-1) — seeds stack_size=1
        parser.process_line(
            r#"[16:00:00] LocalPlayer: ProcessAddItem(RoyalJelly(12345678), -1, True)"#,
        );

        // UpdateItemCode should emit change with delta = new - 1
        let events = parser.process_line(
            r#"[16:00:01] LocalPlayer: ProcessUpdateItemCode(12345678, 327697, True)"#,
        );
        // (327697 >> 16) + 1 = 6, 327697 & 0xFFFF = 17
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::ItemStackChanged {
                item_name,
                old_stack_size,
                new_stack_size,
                delta,
                ..
            } => {
                assert_eq!(item_name.as_deref(), Some("RoyalJelly"));
                assert_eq!(*old_stack_size, 1); // seeded from ProcessAddItem(is_new=True)
                assert_eq!(*new_stack_size, 6); // (327697 >> 16) + 1
                assert_eq!(*delta, 5);
            }
            _ => panic!("Expected ItemStackChanged"),
        }
    }

    #[test]
    fn test_storage_withdrawal_seeds_stack_from_vault_quantity() {
        let mut parser = PlayerEventParser::new();

        // Storage withdrawal: AddItem with slot>=0 should NOT seed stack to 1
        let events = parser.process_line(
            r#"[15:46:07] LocalPlayer: ProcessAddItem(Phlogiston7(172708606), 60, True)"#,
        );
        assert_eq!(events.len(), 1);
        // Stack should NOT be seeded yet (slot>=0 = storage withdrawal)
        assert!(!parser.stack_sizes.contains_key(&172708606));

        // RemoveFromStorageVault seeds the stack from vault quantity
        let events = parser.process_line(
            r#"[15:46:07] LocalPlayer: ProcessRemoveFromStorageVault(302055, -1, 172708606, 1000)"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::StorageWithdrawal { quantity, .. } => {
                assert_eq!(*quantity, 1000);
            }
            _ => panic!("Expected StorageWithdrawal"),
        }
        // Stack should now be seeded from vault quantity
        assert_eq!(parser.stack_sizes.get(&172708606), Some(&1000));

        // A subsequent UpdateItemCode should compute correct delta from 1000
        let events = parser.process_line(
            r#"[15:48:12] LocalPlayer: ProcessUpdateItemCode(172708606, 16346571, False)"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::ItemStackChanged {
                old_stack_size,
                new_stack_size,
                delta,
                ..
            } => {
                assert_eq!(*old_stack_size, 1000);
                // (16346571 >> 16) + 1 = 250
                assert_eq!(*new_stack_size, 250);
                assert_eq!(*delta, -750);
            }
            _ => panic!("Expected ItemStackChanged"),
        }
    }

    #[test]
    fn test_storage_withdrawal_no_false_delta_on_update_item_code() {
        let mut parser = PlayerEventParser::new();

        // Withdraw IceCore stack of 10 from storage
        parser.process_line(
            r#"[21:17:44] LocalPlayer: ProcessAddItem(IceCore(151917070), 68, True)"#,
        );
        parser.process_line(
            r#"[21:17:44] LocalPlayer: ProcessRemoveFromStorageVault(9964570, -1, 151917070, 10)"#,
        );

        // UpdateItemCode confirming stack=10 should have delta=0, not +9
        // encoded: (9 << 16) | 1153 = 590977, actual stack = 9+1 = 10
        let events = parser.process_line(
            r#"[21:17:44] LocalPlayer: ProcessUpdateItemCode(151917070, 590977, True)"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::ItemStackChanged {
                old_stack_size,
                new_stack_size,
                delta,
                ..
            } => {
                assert_eq!(*old_stack_size, 10);
                assert_eq!(*new_stack_size, 10); // (590977 >> 16) + 1 = 10
                assert_eq!(*delta, 0); // No false gain!
            }
            _ => panic!("Expected ItemStackChanged"),
        }
    }

    // ============================================================
    // New Event Type Tests
    // ============================================================

    #[test]
    fn test_parse_end_interaction() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(r#"[13:27:00] LocalPlayer: ProcessEndInteraction(14804)"#);
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::InteractionEnded { entity_id, .. } => {
                assert_eq!(*entity_id, 14804);
            }
            _ => panic!("Expected InteractionEnded"),
        }
    }

    #[test]
    fn test_parse_end_interaction_negative_entity_id() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(r#"[16:06:27] LocalPlayer: ProcessEndInteraction(-153)"#);
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::InteractionEnded { entity_id, .. } => {
                assert_eq!(*entity_id, -153);
            }
            _ => panic!("Expected InteractionEnded"),
        }
    }

    #[test]
    fn test_end_interaction_clears_context() {
        let mut parser = PlayerEventParser::new();
        // Start an interaction
        parser.process_line(
            r#"[13:26:55] LocalPlayer: ProcessStartInteraction(14804, 7, 1200, True, "NPC_Qatik")"#,
        );
        assert!(parser.current_interaction.is_some());

        // End the interaction
        parser.process_line(r#"[13:27:00] LocalPlayer: ProcessEndInteraction(14804)"#);
        assert!(parser.current_interaction.is_none());
    }

    #[test]
    fn test_parse_set_active_skills() {
        let mut parser = PlayerEventParser::new();
        let events = parser
            .process_line(r#"[23:33:22] LocalPlayer: ProcessSetActiveSkills(Riding, Mentalism)"#);
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::ActiveSkillsChanged { skill1, skill2, .. } => {
                assert_eq!(skill1, "Riding");
                assert_eq!(skill2, "Mentalism");
            }
            _ => panic!("Expected ActiveSkillsChanged"),
        }
    }

    #[test]
    fn test_parse_set_active_skills_combat() {
        let mut parser = PlayerEventParser::new();
        let events = parser
            .process_line(r#"[23:33:31] LocalPlayer: ProcessSetActiveSkills(Hammer, Mentalism)"#);
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::ActiveSkillsChanged { skill1, skill2, .. } => {
                assert_eq!(skill1, "Hammer");
                assert_eq!(skill2, "Mentalism");
            }
            _ => panic!("Expected ActiveSkillsChanged"),
        }
    }

    #[test]
    fn test_parse_player_mount() {
        let mut parser = PlayerEventParser::new();
        let events =
            parser.process_line(r#"[23:33:25] LocalPlayer: ProcessPlayerMount(11921978, True)"#);
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::MountStateChanged {
                entity_id,
                is_mounting,
                ..
            } => {
                assert_eq!(*entity_id, 11921978);
                assert!(*is_mounting);
            }
            _ => panic!("Expected MountStateChanged"),
        }
    }

    #[test]
    fn test_parse_player_dismount() {
        let mut parser = PlayerEventParser::new();
        let events =
            parser.process_line(r#"[23:33:31] LocalPlayer: ProcessPlayerMount(11921978, False)"#);
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::MountStateChanged {
                entity_id,
                is_mounting,
                ..
            } => {
                assert_eq!(*entity_id, 11921978);
                assert!(!*is_mounting);
            }
            _ => panic!("Expected MountStateChanged"),
        }
    }

    #[test]
    fn test_parse_set_weather() {
        let mut parser = PlayerEventParser::new();
        let events =
            parser.process_line(r#"[23:32:47] LocalPlayer: ProcessSetWeather("Clear Sky", True)"#);
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::WeatherChanged {
                weather_name,
                is_active,
                ..
            } => {
                assert_eq!(weather_name, "Clear Sky");
                assert!(*is_active);
            }
            _ => panic!("Expected WeatherChanged"),
        }
    }

    #[test]
    fn test_parse_set_weather_cloudy() {
        let mut parser = PlayerEventParser::new();
        let events =
            parser.process_line(r#"[16:06:32] LocalPlayer: ProcessSetWeather("Cloudy 3", True)"#);
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::WeatherChanged { weather_name, .. } => {
                assert_eq!(weather_name, "Cloudy 3");
            }
            _ => panic!("Expected WeatherChanged"),
        }
    }

    #[test]
    fn test_parse_area_config_weather() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line("[20:23:55] AreaConfig Setting weather: Rain");
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::WeatherChanged {
                weather_name,
                is_active,
                ..
            } => {
                assert_eq!(weather_name, "Rain");
                assert!(*is_active);
            }
            _ => panic!("Expected WeatherChanged"),
        }
    }

    #[test]
    fn test_parse_area_config_weather_multiword() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line("[20:06:47] AreaConfig Setting weather: Clear Sky");
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::WeatherChanged { weather_name, .. } => {
                assert_eq!(weather_name, "Clear Sky");
            }
            _ => panic!("Expected WeatherChanged"),
        }
    }

    #[test]
    fn test_parse_update_recipe() {
        let mut parser = PlayerEventParser::new();
        let events =
            parser.process_line(r#"[16:10:13] LocalPlayer: ProcessUpdateRecipe(21052, 151)"#);
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::RecipeUpdated {
                recipe_id,
                completion_count,
                ..
            } => {
                assert_eq!(*recipe_id, 21052);
                assert_eq!(*completion_count, 151);
            }
            _ => panic!("Expected RecipeUpdated"),
        }
    }

    #[test]
    fn test_parse_update_recipe_new() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(r#"[16:10:13] LocalPlayer: ProcessUpdateRecipe(5001, 0)"#);
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::RecipeUpdated {
                recipe_id,
                completion_count,
                ..
            } => {
                assert_eq!(*recipe_id, 5001);
                assert_eq!(*completion_count, 0);
            }
            _ => panic!("Expected RecipeUpdated"),
        }
    }

    #[test]
    fn test_parse_combat_mode_not_in_combat() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[23:33:33] LocalPlayer: ProcessCombatModeStatus(NotInCombat, System.Int32[])"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::CombatStateChanged { in_combat, .. } => {
                assert!(!*in_combat);
            }
            _ => panic!("Expected CombatStateChanged"),
        }
    }

    #[test]
    fn test_parse_combat_mode_in_combat() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[23:33:33] LocalPlayer: ProcessCombatModeStatus(InCombat, System.Int32[])"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::CombatStateChanged { in_combat, .. } => {
                assert!(*in_combat);
            }
            _ => panic!("Expected CombatStateChanged"),
        }
    }

    #[test]
    fn test_parse_vendor_update_gold() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[16:32:25] LocalPlayer: ProcessVendorUpdateAvailableGold(14880, 123456789, 15000)"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::VendorGoldChanged {
                current_gold,
                server_id,
                max_gold,
                ..
            } => {
                assert_eq!(*current_gold, 14880);
                assert_eq!(*server_id, 123456789);
                assert_eq!(*max_gold, 15000);
            }
            _ => panic!("Expected VendorGoldChanged"),
        }
    }

    #[test]
    fn test_parse_vendor_update_gold_after_sale() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[16:32:27] LocalPlayer: ProcessVendorUpdateAvailableGold(14776, 123456789, 15000)"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::VendorGoldChanged { current_gold, .. } => {
                assert_eq!(*current_gold, 14776);
            }
            _ => panic!("Expected VendorGoldChanged"),
        }
    }

    #[test]
    fn test_parse_set_attributes_single() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[23:33:25] LocalPlayer: ProcessSetAttributes(11921978, "[IS_MOUNTED], [1]")"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::AttributesChanged {
                entity_id,
                attributes,
                ..
            } => {
                assert_eq!(*entity_id, 11921978);
                assert_eq!(attributes.len(), 1);
                assert_eq!(attributes[0].name, "IS_MOUNTED");
                assert!((attributes[0].value - 1.0).abs() < 0.001);
            }
            _ => panic!("Expected AttributesChanged"),
        }
    }

    #[test]
    fn test_parse_set_attributes_multiple() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[23:33:33] LocalPlayer: ProcessSetAttributes(11921978, "[CUR_HEALTH, MAX_HEALTH, CUR_POWER, MAX_POWER], [667, 667, 442, 442]")"#
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::AttributesChanged {
                entity_id,
                attributes,
                ..
            } => {
                assert_eq!(*entity_id, 11921978);
                assert_eq!(attributes.len(), 4);
                assert_eq!(attributes[0].name, "CUR_HEALTH");
                assert!((attributes[0].value - 667.0).abs() < 0.001);
                assert_eq!(attributes[1].name, "MAX_HEALTH");
                assert_eq!(attributes[2].name, "CUR_POWER");
                assert!((attributes[2].value - 442.0).abs() < 0.001);
                assert_eq!(attributes[3].name, "MAX_POWER");
            }
            _ => panic!("Expected AttributesChanged"),
        }
    }

    #[test]
    fn test_parse_set_attributes_float_value() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[23:32:47] LocalPlayer: ProcessSetAttributes(11921435, "[WORKORDER_COIN_REWARD_MOD], [1.36]")"#
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::AttributesChanged { attributes, .. } => {
                assert_eq!(attributes.len(), 1);
                assert_eq!(attributes[0].name, "WORKORDER_COIN_REWARD_MOD");
                assert!((attributes[0].value - 1.36).abs() < 0.001);
            }
            _ => panic!("Expected AttributesChanged"),
        }
    }

    // ============================================================
    // Login Snapshot Event Tests
    // ============================================================

    #[test]
    fn test_parse_load_abilities() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[23:32:47] LocalPlayer: ProcessLoadAbilities(System.Int32[], Hammer, Mentalism, AbilityBarContents[])"#
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::AbilitiesLoaded { skill1, skill2, .. } => {
                assert_eq!(skill1, "Hammer");
                assert_eq!(skill2, "Mentalism");
            }
            _ => panic!("Expected AbilitiesLoaded"),
        }
    }

    #[test]
    fn test_parse_load_abilities_riding() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[23:33:25] LocalPlayer: ProcessLoadAbilities(System.Int32[], Riding, Mentalism, AbilityBarContents[])"#
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::AbilitiesLoaded { skill1, skill2, .. } => {
                assert_eq!(skill1, "Riding");
                assert_eq!(skill2, "Mentalism");
            }
            _ => panic!("Expected AbilitiesLoaded"),
        }
    }

    #[test]
    fn test_parse_load_recipes() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[23:46:08] LocalPlayer: ProcessLoadRecipes(System.Int32[], System.Int32[])"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::RecipesLoaded { timestamp, .. } => {
                assert_eq!(timestamp, "23:46:08");
            }
            _ => panic!("Expected RecipesLoaded"),
        }
    }

    #[test]
    fn test_parse_set_equipped_items() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[23:32:47] LocalPlayer: ProcessSetEquippedItems(System.Int32[], System.Int32[], System.Int32[], "@Base2-f(sex=f;race=h;@Chest=@eq-{sex}2-chest-steel-02(^Armor={sex}2-body-steel-02-thorian2;Color1=500050);@MainHand=eq-x-hammer1;MainHandEquip=Hammer;@Feet=@eq-{sex}2-feet-greaves-steel-02(^Armor={sex}2-feet-greaves-steel-02-thorian2;Color1=800080))", 11921435)"#
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::EquipmentChanged {
                entity_id,
                equipment,
                ..
            } => {
                assert_eq!(*entity_id, 11921435);
                // Should find Chest, MainHand, Feet, MainHandEquip
                let slot_names: Vec<&str> = equipment.iter().map(|s| s.slot.as_str()).collect();
                assert!(
                    slot_names.contains(&"Chest"),
                    "Missing Chest slot: {:?}",
                    slot_names
                );
                assert!(
                    slot_names.contains(&"MainHand"),
                    "Missing MainHand slot: {:?}",
                    slot_names
                );
                assert!(
                    slot_names.contains(&"Feet"),
                    "Missing Feet slot: {:?}",
                    slot_names
                );
                assert!(
                    slot_names.contains(&"MainHandEquip"),
                    "Missing MainHandEquip: {:?}",
                    slot_names
                );

                // Verify MainHand value
                let main_hand = equipment.iter().find(|s| s.slot == "MainHand").unwrap();
                assert_eq!(main_hand.appearance_key, "eq-x-hammer1");

                // Verify MainHandEquip value
                let equip_type = equipment
                    .iter()
                    .find(|s| s.slot == "MainHandEquip")
                    .unwrap();
                assert_eq!(equip_type.appearance_key, "Hammer");
            }
            _ => panic!("Expected EquipmentChanged"),
        }
    }

    #[test]
    fn test_parse_set_equipped_items_mount_entity() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[23:33:25] LocalPlayer: ProcessSetEquippedItems(System.Int32[], System.Int32[], System.Int32[], "@Base2-f(sex=f;race=h;@Head=FloatingGem2;@OffHandShield=eq-x-shield5;OffHandEquip=Shield)", 11921978)"#
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::EquipmentChanged {
                entity_id,
                equipment,
                ..
            } => {
                assert_eq!(*entity_id, 11921978);
                let slot_names: Vec<&str> = equipment.iter().map(|s| s.slot.as_str()).collect();
                assert!(
                    slot_names.contains(&"Head"),
                    "Missing Head: {:?}",
                    slot_names
                );
                assert!(
                    slot_names.contains(&"OffHandShield"),
                    "Missing OffHandShield: {:?}",
                    slot_names
                );
                assert!(
                    slot_names.contains(&"OffHandEquip"),
                    "Missing OffHandEquip: {:?}",
                    slot_names
                );
            }
            _ => panic!("Expected EquipmentChanged"),
        }
    }

    #[test]
    fn test_equipment_slot_parsing_nested_parens() {
        // Verify that nested parens in appearance values are handled correctly
        let slots = parse_equipment_slots(
            "@Chest=@eq-f2-chest-steel-02(^Armor=f2-body-steel-02;Color1=500050;Color2=C0C0C0);@Legs=@eq-f2-legs-steel-02(^Armor=f2-body-steel-02;Color1=500050)"
        );
        assert_eq!(slots.len(), 2);
        assert_eq!(slots[0].slot, "Chest");
        assert_eq!(
            slots[0].appearance_key,
            "@eq-f2-chest-steel-02(^Armor=f2-body-steel-02;Color1=500050;Color2=C0C0C0)"
        );
        assert_eq!(slots[1].slot, "Legs");
        assert_eq!(
            slots[1].appearance_key,
            "@eq-f2-legs-steel-02(^Armor=f2-body-steel-02;Color1=500050)"
        );
    }

    // ── Effect Event Tests ───────────────────────────────────────────────

    #[test]
    fn test_parse_add_effects_login_batch() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[23:32:46] LocalPlayer: ProcessAddEffects(11921435, 0, "[302, 303, 13330, 26297]", False)"#
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::EffectsAdded {
                entity_id,
                source_entity_id,
                effect_ids,
                is_login_batch,
                ..
            } => {
                assert_eq!(*entity_id, 11921435);
                assert_eq!(*source_entity_id, 0);
                assert_eq!(*effect_ids, vec![302, 303, 13330, 26297]);
                assert!(*is_login_batch);
            }
            _ => panic!("Expected EffectsAdded"),
        }
    }

    #[test]
    fn test_parse_add_effects_gameplay() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[23:32:47] LocalPlayer: ProcessAddEffects(11921435, 11921435, "[13304, ]", True)"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::EffectsAdded {
                source_entity_id,
                effect_ids,
                is_login_batch,
                ..
            } => {
                assert_eq!(*source_entity_id, 11921435);
                assert_eq!(*effect_ids, vec![13304]);
                assert!(!*is_login_batch);
            }
            _ => panic!("Expected EffectsAdded"),
        }
    }

    #[test]
    fn test_parse_remove_effects() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[23:35:12] LocalPlayer: ProcessRemoveEffects(11921435, System.Int32[])"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::EffectsRemoved { entity_id, .. } => {
                assert_eq!(*entity_id, 11921435);
            }
            _ => panic!("Expected EffectsRemoved"),
        }
    }

    #[test]
    fn test_parse_update_effect_name() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[23:32:48] LocalPlayer: ProcessUpdateEffectName(11921435, 123456, "Performance Appreciation, Level 0")"#
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::EffectNameUpdated {
                entity_id,
                effect_instance_id,
                display_name,
                ..
            } => {
                assert_eq!(*entity_id, 11921435);
                assert_eq!(*effect_instance_id, 123456);
                assert_eq!(display_name, "Performance Appreciation, Level 0");
            }
            _ => panic!("Expected EffectNameUpdated"),
        }
    }

    #[test]
    fn test_parse_update_description_gardening() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[21:12:31] LocalPlayer: ProcessUpdateDescription(9960026, "Ripe Onion", "This onion is fully grown and in peak condition.", "Harvest Onion", UseItem, "Onion(Scale=1)", 0)"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::EntityDescriptionUpdated {
                entity_id,
                name,
                description,
                action,
                action_type,
                appearance,
                flags,
                ..
            } => {
                assert_eq!(*entity_id, 9960026);
                assert_eq!(name, "Ripe Onion");
                assert_eq!(
                    description,
                    "This onion is fully grown and in peak condition."
                );
                assert_eq!(action, "Harvest Onion");
                assert_eq!(action_type, "UseItem");
                assert_eq!(appearance, "Onion(Scale=1)");
                assert_eq!(*flags, 0);
            }
            _ => panic!("Expected EntityDescriptionUpdated"),
        }
    }

    #[test]
    fn test_parse_update_description_crafting() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[14:05:22] LocalPlayer: ProcessUpdateDescription(1601585, "Rising Simple Sourdough", "Proofing for 00:00:05", "Bake Simple Sourdough", UseItem, "Dough(Scale=0.36547)", 0)"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::EntityDescriptionUpdated {
                entity_id,
                name,
                description,
                action,
                ..
            } => {
                assert_eq!(*entity_id, 1601585);
                assert_eq!(name, "Rising Simple Sourdough");
                assert_eq!(description, "Proofing for 00:00:05");
                assert_eq!(action, "Bake Simple Sourdough");
            }
            _ => panic!("Expected EntityDescriptionUpdated"),
        }
    }

    // ============================================================
    // Activity Context tests (Phase 1)
    // ============================================================

    #[test]
    fn test_activity_mining_named_node() {
        // Real-world case: StartInteraction carries "MiningNodeFromSurvey9",
        // DoDelayLoop(6, ChopLumber, "Mining...") follows within the same second.
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[18:04:23] LocalPlayer: ProcessStartInteraction(435138, 7, 0, False, "MiningNodeFromSurvey9")"#,
        );
        parser.process_line(
            r#"[18:04:23] LocalPlayer: ProcessDoDelayLoop(6, ChopLumber, "Mining...", 0, AbortIfAttacked, IsInteractorDelayLoop)"#,
        );

        let ctxs = parser.active_activities();
        assert_eq!(ctxs.len(), 1);
        match &ctxs[0].source {
            ActivitySource::Mining {
                node_entity_id,
                node_name,
            } => {
                assert_eq!(*node_entity_id, Some(435138));
                assert_eq!(node_name.as_deref(), Some("MiningNodeFromSurvey9"));
            }
            other => panic!("Expected Mining source, got {:?}", other),
        }
        assert_eq!(ctxs[0].entity_id, Some(435138));
    }

    #[test]
    fn test_activity_mining_unnamed_node() {
        // Motherlode / plain world nodes often have empty interaction names.
        // We must still detect Mining from the DoDelayLoop label alone.
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[22:14:23] LocalPlayer: ProcessStartInteraction(1176954, 7, 0, False, "")"#,
        );
        parser.process_line(
            r#"[22:14:23] LocalPlayer: ProcessDoDelayLoop(7, ChopLumber, "Mining ...", 0, AbortIfAttacked, IsInteractorDelayLoop)"#,
        );

        let ctxs = parser.active_activities();
        assert_eq!(
            ctxs.len(),
            1,
            "Mining context should be detected despite empty name"
        );
        match &ctxs[0].source {
            ActivitySource::Mining {
                node_entity_id,
                node_name,
            } => {
                assert_eq!(*node_entity_id, Some(1176954));
                assert_eq!(
                    node_name.as_deref(),
                    None,
                    "name was empty — stored as None"
                );
            }
            other => panic!("Expected Mining source, got {:?}", other),
        }
    }

    #[test]
    fn test_activity_mining_label_space_variant() {
        // Both "Mining..." and "Mining ..." have appeared across game versions.
        // The classifier must treat them identically.
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[18:04:23] LocalPlayer: ProcessDoDelayLoop(6, ChopLumber, "Mining ...", 0, AbortIfAttacked, IsInteractorDelayLoop)"#,
        );
        assert!(matches!(
            parser.active_activities()[0].source,
            ActivitySource::Mining { .. }
        ));
    }

    #[test]
    fn test_activity_survey_map_use() {
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[15:43:36] LocalPlayer: ProcessDoDelayLoop(0.5, Unset, "Using Serbule Blue Mineral Survey", 5305, AbortIfAttacked)"#,
        );

        let ctxs = parser.active_activities();
        assert_eq!(ctxs.len(), 1);
        match &ctxs[0].source {
            ActivitySource::SurveyMapUse {
                survey_map_internal_name,
            } => {
                assert_eq!(
                    survey_map_internal_name.as_deref(),
                    Some("Serbule Blue Mineral Survey")
                );
            }
            other => panic!("Expected SurveyMapUse, got {:?}", other),
        }
    }

    #[test]
    fn test_activity_survey_map_craft() {
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[15:24:59] LocalPlayer: ProcessDoDelayLoop(5, UseTeleportationCircle, "Surveying", 5305, AbortIfAttacked)"#,
        );
        assert_eq!(
            parser.active_activities()[0].source,
            ActivitySource::SurveyMapCraft
        );
    }

    #[test]
    fn test_activity_explicit_end_closes_mining() {
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[18:04:23] LocalPlayer: ProcessStartInteraction(435138, 7, 0, False, "MiningNodeFromSurvey9")"#,
        );
        parser.process_line(
            r#"[18:04:23] LocalPlayer: ProcessDoDelayLoop(6, ChopLumber, "Mining...", 0, AbortIfAttacked, IsInteractorDelayLoop)"#,
        );
        assert_eq!(parser.active_activities().len(), 1);

        parser.process_line(r#"[18:04:29] LocalPlayer: ProcessEndInteraction(435138)"#);
        assert_eq!(
            parser.active_activities().len(),
            0,
            "EndInteraction should close matching mining context"
        );
    }

    #[test]
    fn test_activity_times_out_after_deadline() {
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[18:04:23] LocalPlayer: ProcessDoDelayLoop(6, ChopLumber, "Mining...", 0, AbortIfAttacked, IsInteractorDelayLoop)"#,
        );
        assert_eq!(parser.active_activities().len(), 1);

        // 6s duration + 2s slack = should still be active at 18:04:30
        parser.process_line(r#"[18:04:30] LocalPlayer: ProcessCombatModeStatus(NotInCombat, [])"#);
        assert_eq!(parser.active_activities().len(), 1);

        // By 18:04:32 (9s after start) the context should have expired
        parser.process_line(r#"[18:04:32] LocalPlayer: ProcessCombatModeStatus(NotInCombat, [])"#);
        assert_eq!(parser.active_activities().len(), 0);
    }

    #[test]
    fn test_activity_repeated_delay_loop_does_not_stack() {
        // Some delay loops re-emit on every tick for the same activity. We
        // must refresh the deadline in place, not push duplicate contexts.
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[18:04:23] LocalPlayer: ProcessStartInteraction(435138, 7, 0, False, "MiningNodeFromSurvey9")"#,
        );
        parser.process_line(
            r#"[18:04:23] LocalPlayer: ProcessDoDelayLoop(6, ChopLumber, "Mining...", 0, AbortIfAttacked, IsInteractorDelayLoop)"#,
        );
        parser.process_line(
            r#"[18:04:24] LocalPlayer: ProcessDoDelayLoop(6, ChopLumber, "Mining...", 0, AbortIfAttacked, IsInteractorDelayLoop)"#,
        );
        parser.process_line(
            r#"[18:04:25] LocalPlayer: ProcessDoDelayLoop(6, ChopLumber, "Mining...", 0, AbortIfAttacked, IsInteractorDelayLoop)"#,
        );
        assert_eq!(
            parser.active_activities().len(),
            1,
            "same-entity repeated loops collapse"
        );
    }

    #[test]
    fn test_activity_corpse_search_from_talk_screen() {
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[18:04:38] LocalPlayer: ProcessStartInteraction(435269, 10, 0, False, "")"#,
        );
        parser.process_line(
            r#"[18:04:38] LocalPlayer: ProcessTalkScreen(435269, "Search Corpse of Ratkin Miner", "", "", System.Int32[], System.String[], 0, Corpse)"#,
        );

        let ctxs = parser.active_activities();
        assert_eq!(ctxs.len(), 1);
        match &ctxs[0].source {
            ActivitySource::CorpseSearch {
                entity_id,
                corpse_name,
            } => {
                assert_eq!(*entity_id, 435269);
                assert_eq!(corpse_name, "Ratkin Miner");
            }
            other => panic!("Expected CorpseSearch, got {:?}", other),
        }
    }

    /// Helper: pull the first LootPickedUp event out of a batch.
    fn first_loot_pickup(events: &[PlayerEvent]) -> Option<&PlayerEvent> {
        events
            .iter()
            .find(|e| matches!(e, PlayerEvent::LootPickedUp { .. }))
    }

    #[test]
    fn test_loot_attributes_to_fresh_corpse_search() {
        // Genuine corpse loot: the item is taken in the same tick the corpse's
        // `Search Corpse of X` window (re-)fires. It must be filed under that
        // corpse.
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[18:04:38] LocalPlayer: ProcessTalkScreen(435269, "Search Corpse of Ratkin Miner", "", "", System.Int32[], System.String[], 1, Corpse)"#,
        );
        parser.process_line(r#"[18:04:38] LocalPlayer: ProcessAddItem(SpruceWood(111), -1, True)"#);
        let events = parser.process_line(r#"[18:04:38] LocalPlayer: ProcessRemoveLoot(111)"#);
        match first_loot_pickup(&events).expect("LootPickedUp present") {
            PlayerEvent::LootPickedUp {
                corpse_entity_id,
                corpse_name,
                item_name,
                ..
            } => {
                assert_eq!(*corpse_entity_id, Some(435269));
                assert_eq!(corpse_name.as_deref(), Some("Ratkin Miner"));
                assert_eq!(item_name.as_deref(), Some("SpruceWood"));
            }
            other => panic!("Expected LootPickedUp, got {:?}", other),
        }
    }

    #[test]
    fn test_manual_loot_two_seconds_after_search_still_attributes() {
        // Manual first-grab off a boss: window opens, item taken 2s later with
        // no re-fire (real Throgg-the-Gemlord pattern). 2s is within the fresh
        // window, so it still counts as that corpse's drop.
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[19:20:03] LocalPlayer: ProcessTalkScreen(428100, "Search Corpse of Throgg the Gemlord", "", "", System.Int32[], System.String[], 0, Corpse)"#,
        );
        parser.process_line(
            r#"[19:20:05] LocalPlayer: ProcessAddItem(CouncilCoinpurse5(222), -1, True)"#,
        );
        let events = parser.process_line(r#"[19:20:05] LocalPlayer: ProcessRemoveLoot(222)"#);
        match first_loot_pickup(&events).expect("LootPickedUp present") {
            PlayerEvent::LootPickedUp {
                corpse_entity_id,
                corpse_name,
                ..
            } => {
                assert_eq!(*corpse_entity_id, Some(428100));
                assert_eq!(corpse_name.as_deref(), Some("Throgg the Gemlord"));
            }
            other => panic!("Expected LootPickedUp, got {:?}", other),
        }
    }

    #[test]
    fn test_ground_loot_after_stale_corpse_is_not_attributed() {
        // The Xogrite bug: a monster is killed and its corpse searched, then the
        // player gathers a ground node (Xogrite Chunk) several seconds later.
        // The gather fires its own ProcessRemoveLoot with no corpse window; it
        // must NOT be filed under the (now-stale) monster.
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[12:00:00] LocalPlayer: ProcessTalkScreen(500, "Search Corpse of Aktaari Royal Guard", "", "", System.Int32[], System.String[], 1, Corpse)"#,
        );
        // ... 8 seconds later, gather Xogrite from a ground node.
        parser
            .process_line(r#"[12:00:08] LocalPlayer: ProcessAddItem(XogriteChunk(999), -1, True)"#);
        let events = parser.process_line(r#"[12:00:08] LocalPlayer: ProcessRemoveLoot(999)"#);
        match first_loot_pickup(&events).expect("LootPickedUp present") {
            PlayerEvent::LootPickedUp {
                corpse_entity_id,
                corpse_name,
                item_name,
                ..
            } => {
                assert_eq!(
                    *corpse_entity_id, None,
                    "ground loot must not borrow a stale corpse"
                );
                assert_eq!(*corpse_name, None);
                assert_eq!(item_name.as_deref(), Some("XogriteChunk"));
            }
            other => panic!("Expected LootPickedUp, got {:?}", other),
        }
    }

    #[test]
    fn test_end_interaction_clears_corpse_mark() {
        // Closing the corpse window drops the mark, so a ground pickup right
        // after (even within the fresh window) isn't filed under the corpse.
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[12:00:00] LocalPlayer: ProcessTalkScreen(500, "Search Corpse of Aktaari Royal Guard", "", "", System.Int32[], System.String[], 1, Corpse)"#,
        );
        parser.process_line(r#"[12:00:00] LocalPlayer: ProcessEndInteraction(500)"#);
        parser
            .process_line(r#"[12:00:01] LocalPlayer: ProcessAddItem(XogriteChunk(999), -1, True)"#);
        let events = parser.process_line(r#"[12:00:01] LocalPlayer: ProcessRemoveLoot(999)"#);
        match first_loot_pickup(&events).expect("LootPickedUp present") {
            PlayerEvent::LootPickedUp {
                corpse_entity_id, ..
            } => {
                assert_eq!(
                    *corpse_entity_id, None,
                    "closed corpse window must not attribute"
                );
            }
            other => panic!("Expected LootPickedUp, got {:?}", other),
        }
    }

    #[test]
    fn test_activity_vendor_browsing_enriched_with_npc_name() {
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[15:54:17] LocalPlayer: ProcessStartInteraction(32857, 7, 522.75, True, "NPC_FarsightFlemmings")"#,
        );
        parser.process_line(
            r#"[15:54:18] LocalPlayer: ProcessVendorScreen(32857, Friends, 20000, 0, 20000, "", VendorInfo[], VendorInfo[], VendorInfo[], VendorPurchaseCap[], [-1201,-1601,-1101,-1301,-2001,-1801,], System.String[], -1601)"#,
        );

        let ctxs = parser.active_activities();
        assert_eq!(ctxs.len(), 1);
        match &ctxs[0].source {
            ActivitySource::VendorBrowsing {
                npc_entity_id,
                npc_name,
            } => {
                assert_eq!(*npc_entity_id, 32857);
                assert_eq!(npc_name.as_deref(), Some("NPC_FarsightFlemmings"));
            }
            other => panic!("Expected VendorBrowsing, got {:?}", other),
        }
    }

    #[test]
    fn test_player_vendor_screen_other_stall_emits_opened() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[12:29:24] LocalPlayer: ProcessPlayerVendorScreen(13676, "Remember: press SHIFT to buy multiples quickly.", System.Collections.Generic.List`1[PlayerVendorItemForSale], 41, False, False, [], System.String[], 1)"#,
        );

        let opened: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, PlayerEvent::VendorScreenOpened { .. }))
            .collect();
        assert_eq!(opened.len(), 1);
        match &opened[0] {
            PlayerEvent::VendorScreenOpened {
                timestamp,
                npc_entity_id,
                is_own_stall,
            } => {
                assert_eq!(timestamp, "12:29:24");
                assert_eq!(*npc_entity_id, 13676);
                assert!(!*is_own_stall);
            }
            other => panic!("Expected VendorScreenOpened, got {:?}", other),
        }
    }

    #[test]
    fn test_player_vendor_screen_own_stall_emits_nothing() {
        let mut parser = PlayerEventParser::new();
        // 5th arg True = own stall management mode: no capture signal.
        let events = parser.process_line(
            r#"[12:30:00] LocalPlayer: ProcessPlayerVendorScreen(9999, "Remember: press SHIFT to buy multiples quickly.", System.Collections.Generic.List`1[PlayerVendorItemForSale], 41, True, False, [], System.String[], 1)"#,
        );

        assert!(
            !events
                .iter()
                .any(|e| matches!(e, PlayerEvent::VendorScreenOpened { .. })),
            "own-stall open must not emit VendorScreenOpened"
        );
        // But the session is still tracked: a Remove on it must not count as a
        // purchase either.
        let events = parser
            .process_line(r#"[12:30:10] LocalPlayer: ProcessPlayerVendorScreenRemove(9999, 42)"#);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, PlayerEvent::VendorItemTaken { .. })),
            "collecting own listing must not emit VendorItemTaken"
        );
    }

    #[test]
    fn test_player_vendor_purchase_emits_item_taken() {
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[12:29:24] LocalPlayer: ProcessPlayerVendorScreen(13676, "Remember: press SHIFT to buy multiples quickly.", System.Collections.Generic.List`1[PlayerVendorItemForSale], 41, False, False, [], System.String[], 1)"#,
        );
        parser.process_line(
            r#"[12:29:45] LocalPlayer: ProcessAddItem(CantaloupeWine(256596391), -1, True)"#,
        );
        let events = parser.process_line(
            r#"[12:29:45] LocalPlayer: ProcessPlayerVendorScreenRemove(13676, 256596391)"#,
        );

        let taken: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, PlayerEvent::VendorItemTaken { .. }))
            .collect();
        assert_eq!(taken.len(), 1);
        match &taken[0] {
            PlayerEvent::VendorItemTaken {
                timestamp,
                npc_entity_id,
                instance_id,
                is_own_stall,
            } => {
                assert_eq!(timestamp, "12:29:45");
                assert_eq!(*npc_entity_id, 13676);
                assert_eq!(*instance_id, 256596391);
                assert!(!*is_own_stall);
            }
            other => panic!("Expected VendorItemTaken, got {:?}", other),
        }
    }

    #[test]
    fn test_end_interaction_clears_vendor_screen_session() {
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[12:29:24] LocalPlayer: ProcessPlayerVendorScreen(13676, "Remember: press SHIFT to buy multiples quickly.", System.Collections.Generic.List`1[PlayerVendorItemForSale], 41, False, False, [], System.String[], 1)"#,
        );
        parser.process_line(r#"[12:29:40] LocalPlayer: ProcessEndInteraction(13676)"#);
        // A later Remove from a DIFFERENT npc (no screen open) must stay silent.
        let events = parser
            .process_line(r#"[12:30:10] LocalPlayer: ProcessPlayerVendorScreenRemove(8888, 77)"#);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, PlayerEvent::VendorItemTaken { .. })),
            "EndInteraction must clear the vendor session"
        );
    }

    #[test]
    fn test_player_vendor_screen_remove_wrong_npc_no_event() {
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[12:29:24] LocalPlayer: ProcessPlayerVendorScreen(13676, "Remember: press SHIFT to buy multiples quickly.", System.Collections.Generic.List`1[PlayerVendorItemForSale], 41, False, False, [], System.String[], 1)"#,
        );
        // Remove for a different npc id than the open screen: not our stall.
        let events = parser.process_line(
            r#"[12:29:45] LocalPlayer: ProcessPlayerVendorScreenRemove(8888, 256596391)"#,
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, PlayerEvent::VendorItemTaken { .. })),
            "Remove from a non-matching npc must not emit"
        );
    }

    #[test]
    fn test_activity_storage_browsing_captures_vault_name() {
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[15:45:38] LocalPlayer: ProcessShowStorageVault(302055, 114, "Saddlebag", "Access saddlebag contents here", 80, System.Collections.Generic.List`1[Item], System.String[], "", [], System.String[], 0)"#,
        );

        let ctxs = parser.active_activities();
        assert_eq!(ctxs.len(), 1);
        match &ctxs[0].source {
            ActivitySource::StorageBrowsing {
                vault_owner_entity_id,
                vault_name,
            } => {
                assert_eq!(*vault_owner_entity_id, 302055);
                assert_eq!(vault_name, "Saddlebag");
            }
            other => panic!("Expected StorageBrowsing, got {:?}", other),
        }
    }

    #[test]
    fn test_activity_overlapping_contexts_coexist() {
        // Mining gets interrupted by combat, player kills attacker, searches
        // corpse — all before the mining context times out. Both contexts
        // should be present on the stack during the overlap.
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[18:04:23] LocalPlayer: ProcessStartInteraction(435138, 7, 0, False, "MiningNodeFromSurvey9")"#,
        );
        parser.process_line(
            r#"[18:04:23] LocalPlayer: ProcessDoDelayLoop(6, ChopLumber, "Mining...", 0, AbortIfAttacked, IsInteractorDelayLoop)"#,
        );
        parser.process_line(
            r#"[18:04:29] LocalPlayer: ProcessStartInteraction(435214, 10, 0, False, "")"#,
        );
        parser.process_line(
            r#"[18:04:29] LocalPlayer: ProcessTalkScreen(435214, "Search Corpse of Ratkin Miner", "", "", System.Int32[], System.String[], 0, Corpse)"#,
        );

        let ctxs = parser.active_activities();
        assert_eq!(
            ctxs.len(),
            2,
            "both mining and corpse-search should be active"
        );
        // Mining was pushed first, corpse search second — stack preserves order
        assert!(matches!(ctxs[0].source, ActivitySource::Mining { .. }));
        assert!(matches!(
            ctxs[1].source,
            ActivitySource::CorpseSearch { .. }
        ));
    }

    #[test]
    fn test_activity_closing_one_context_leaves_others() {
        // When the corpse search ends but mining hasn't expired, mining remains.
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[18:04:23] LocalPlayer: ProcessStartInteraction(435138, 7, 0, False, "MiningNodeFromSurvey9")"#,
        );
        parser.process_line(
            r#"[18:04:23] LocalPlayer: ProcessDoDelayLoop(6, ChopLumber, "Mining...", 0, AbortIfAttacked, IsInteractorDelayLoop)"#,
        );
        parser.process_line(
            r#"[18:04:25] LocalPlayer: ProcessStartInteraction(435214, 10, 0, False, "")"#,
        );
        parser.process_line(
            r#"[18:04:25] LocalPlayer: ProcessTalkScreen(435214, "Search Corpse of Ratkin Miner", "", "", System.Int32[], System.String[], 0, Corpse)"#,
        );
        assert_eq!(parser.active_activities().len(), 2);

        parser.process_line(r#"[18:04:26] LocalPlayer: ProcessEndInteraction(435214)"#);
        let ctxs = parser.active_activities();
        assert_eq!(ctxs.len(), 1);
        assert!(matches!(ctxs[0].source, ActivitySource::Mining { .. }));
    }

    #[test]
    fn test_activity_non_matching_end_interaction_no_effect() {
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[18:04:23] LocalPlayer: ProcessStartInteraction(435138, 7, 0, False, "MiningNodeFromSurvey9")"#,
        );
        parser.process_line(
            r#"[18:04:23] LocalPlayer: ProcessDoDelayLoop(6, ChopLumber, "Mining...", 0, AbortIfAttacked, IsInteractorDelayLoop)"#,
        );

        // EndInteraction for a different entity — mining context should remain
        parser.process_line(r#"[18:04:24] LocalPlayer: ProcessEndInteraction(999999)"#);
        assert_eq!(parser.active_activities().len(), 1);
    }

    // ============================================================
    // Provenance attribution tests (Phase 2)
    // ============================================================

    /// Helper: extract the provenance from the first gain event in a batch.
    fn first_provenance(events: &[PlayerEvent]) -> ItemProvenance {
        for ev in events {
            match ev {
                PlayerEvent::ItemAdded { provenance, .. } => return provenance.clone(),
                PlayerEvent::ItemStackChanged { provenance, .. } => return provenance.clone(),
                PlayerEvent::StorageWithdrawal { provenance, .. } => return provenance.clone(),
                _ => {}
            }
        }
        panic!("no gain event with provenance in batch");
    }

    #[test]
    fn test_provenance_unknown_when_no_context() {
        // Genuine new item drop with nothing else going on: should be
        // UnknownSource. This bucket is expected to be sizeable.
        let mut parser = PlayerEventParser::new();
        let events = parser
            .process_line(r#"[12:00:00] LocalPlayer: ProcessAddItem(RandomLoot(12345), -1, True)"#);
        assert_eq!(first_provenance(&events), ItemProvenance::UnknownSource);
    }

    #[test]
    fn test_provenance_session_load_is_not_applicable() {
        // AddItem with is_new=false is a session-load reload — not a gain,
        // even if contexts happen to be active.
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[12:00:00] LocalPlayer: ProcessAddItem(ExistingItem(999), -1, False)"#,
        );
        assert_eq!(first_provenance(&events), ItemProvenance::NotApplicable);
    }

    #[test]
    fn test_provenance_mining_confident_attribution() {
        // Mining context active → item drop attributes to Mining confidently.
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[18:04:41] LocalPlayer: ProcessStartInteraction(435138, 7, 0, False, "MiningNodeFromSurvey9")"#,
        );
        parser.process_line(
            r#"[18:04:41] LocalPlayer: ProcessDoDelayLoop(6, ChopLumber, "Mining...", 0, AbortIfAttacked, IsInteractorDelayLoop)"#,
        );
        let events = parser.process_line(
            r#"[18:04:47] LocalPlayer: ProcessAddItem(Orichalcum(185976137), -1, True)"#,
        );

        match first_provenance(&events) {
            ItemProvenance::Attributed {
                source, confidence, ..
            } => {
                assert!(matches!(source, ActivitySource::Mining { .. }));
                assert_eq!(confidence, AttributionConfidence::Confident);
            }
            other => panic!("Expected Attributed Mining, got {:?}", other),
        }
    }

    #[test]
    fn test_provenance_survey_map_use() {
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[15:43:36] LocalPlayer: ProcessDoDelayLoop(0.5, Unset, "Using Serbule Blue Mineral Survey", 5305, AbortIfAttacked)"#,
        );
        let events = parser.process_line(
            r#"[15:43:36] LocalPlayer: ProcessAddItem(Fluorite(181871021), -1, True)"#,
        );
        match first_provenance(&events) {
            ItemProvenance::Attributed { source, .. } => {
                assert!(matches!(source, ActivitySource::SurveyMapUse { .. }));
            }
            other => panic!("Expected Attributed SurveyMapUse, got {:?}", other),
        }
    }

    #[test]
    fn test_provenance_mining_wins_over_passive_corpse_search() {
        // Real overlap scenario from the 50x-povus dataset: player mines a
        // multihit node, gets attacked between swings, kills the attacker,
        // and a corpse-search screen opens. Then the next mining swing
        // produces loot. With both Mining and CorpseSearch contexts active,
        // the loot belongs to Mining (the explicit active task). The
        // parser's tie-breaker should pick Mining with Probable confidence.
        //
        // This matters at scale: in the povus dataset, this overlap drops
        // ~40% of mining loot into Uncertain without the tie-breaker. With
        // it, mining-loot attribution rises from ~60% to ~99%.
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[18:04:23] LocalPlayer: ProcessStartInteraction(435138, 7, 0, False, "MiningNodeFromSurvey9")"#,
        );
        parser.process_line(
            r#"[18:04:23] LocalPlayer: ProcessDoDelayLoop(6, ChopLumber, "Mining...", 0, AbortIfAttacked, IsInteractorDelayLoop)"#,
        );
        parser.process_line(
            r#"[18:04:25] LocalPlayer: ProcessStartInteraction(435214, 10, 0, False, "")"#,
        );
        parser.process_line(
            r#"[18:04:25] LocalPlayer: ProcessTalkScreen(435214, "Search Corpse of Ratkin Miner", "", "", System.Int32[], System.String[], 0, Corpse)"#,
        );
        let events = parser
            .process_line(r#"[18:04:26] LocalPlayer: ProcessAddItem(Orichalcum(99999), -1, True)"#);
        match first_provenance(&events) {
            ItemProvenance::Attributed {
                source, confidence, ..
            } => {
                assert!(
                    matches!(source, ActivitySource::Mining { .. }),
                    "expected Mining attribution, got {:?}",
                    source
                );
                assert_eq!(
                    confidence,
                    AttributionConfidence::Probable,
                    "tie-breaker outcome should be Probable, not Confident"
                );
            }
            other => panic!("Expected Attributed Mining, got {:?}", other),
        }
    }

    #[test]
    fn test_provenance_uncertain_when_two_non_mining_contexts() {
        // The Mining-vs-passive tie-breaker only applies when Mining is one
        // of the candidates. Other multi-context overlaps (e.g., corpse
        // search while a vendor screen is open) still produce Uncertain so
        // downstream code doesn't get a confident wrong answer.
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[12:00:00] LocalPlayer: ProcessStartInteraction(100, 7, 0, False, "NPC_Vendor")"#,
        );
        parser.process_line(
            r#"[12:00:00] LocalPlayer: ProcessVendorScreen(100, Friends, 20000, 0, 20000, "", VendorInfo[], VendorInfo[], VendorInfo[], VendorPurchaseCap[], [], System.String[], -1601)"#,
        );
        parser.process_line(
            r#"[12:00:01] LocalPlayer: ProcessStartInteraction(200, 10, 0, False, "")"#,
        );
        parser.process_line(
            r#"[12:00:01] LocalPlayer: ProcessTalkScreen(200, "Search Corpse of Some Mob", "", "", System.Int32[], System.String[], 0, Corpse)"#,
        );
        let events = parser.process_line(
            r#"[12:00:02] LocalPlayer: ProcessAddItem(MysteryItem(77777), -1, True)"#,
        );
        match first_provenance(&events) {
            ItemProvenance::Uncertain { candidates } => {
                assert_eq!(
                    candidates.len(),
                    2,
                    "expected two candidates for non-mining overlap"
                );
            }
            other => panic!("Expected Uncertain, got {:?}", other),
        }
    }

    #[test]
    fn test_provenance_on_stack_change_positive_delta() {
        // Existing stack gaining items during mining → ItemStackChanged gets
        // Mining provenance. Simulates survey-loot-into-existing-stack.
        let mut parser = PlayerEventParser::new();
        // Establish baseline for an item
        parser
            .process_line(r#"[17:00:00] LocalPlayer: ProcessAddItem(Rubyquartz(555), -1, False)"#);
        parser.process_line(
            r#"[17:00:01] LocalPlayer: ProcessUpdateItemCode(555, 131072, True)"#, // stack=3
        );

        // Open mining context
        parser.process_line(
            r#"[17:05:00] LocalPlayer: ProcessDoDelayLoop(6, ChopLumber, "Mining...", 0, AbortIfAttacked, IsInteractorDelayLoop)"#,
        );
        // Stack grows from 3 to 5 (+2) — should be attributed to Mining
        let events = parser
            .process_line(r#"[17:05:05] LocalPlayer: ProcessUpdateItemCode(555, 262144, True)"#);

        match first_provenance(&events) {
            ItemProvenance::Attributed { source, .. } => {
                assert!(matches!(source, ActivitySource::Mining { .. }));
            }
            other => panic!("Expected Attributed Mining, got {:?}", other),
        }
    }

    #[test]
    fn test_provenance_negative_delta_is_not_applicable() {
        // Stack shrinking (consumption during crafting, selling, etc.) is a
        // loss, not a gain. Provenance should be NotApplicable regardless of
        // active contexts so gain-aggregates don't include it.
        let mut parser = PlayerEventParser::new();
        parser.process_line(r#"[10:00:00] LocalPlayer: ProcessAddItem(Ink(42), -1, False)"#);
        parser.process_line(r#"[10:00:01] LocalPlayer: ProcessUpdateItemCode(42, 262144, True)"#); // stack=5

        // Open a crafting context
        parser.process_line(
            r#"[10:01:00] LocalPlayer: ProcessDoDelayLoop(5, UseTeleportationCircle, "Surveying", 5305, AbortIfAttacked)"#,
        );
        // Stack drops 5 → 4 (consumption)
        let events = parser
            .process_line(r#"[10:01:05] LocalPlayer: ProcessUpdateItemCode(42, 196608, True)"#);

        // Stack delta is -1 → should be NotApplicable
        assert_eq!(first_provenance(&events), ItemProvenance::NotApplicable);
    }

    #[test]
    fn test_provenance_storage_withdrawal_attributes_to_storage() {
        // Storage withdrawal should attribute to the StorageBrowsing context
        // whose owner matches the vault npc_id.
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[15:45:38] LocalPlayer: ProcessShowStorageVault(302055, 114, "Saddlebag", "Access saddlebag contents here", 80, System.Collections.Generic.List`1[Item], System.String[], "", [], System.String[], 0)"#,
        );
        // ProcessAddItem for the withdrawal (slot>=0 branch doesn't seed stack)
        parser.process_line(
            r#"[15:46:07] LocalPlayer: ProcessAddItem(Phlogiston7(172708606), 60, True)"#,
        );
        let events = parser.process_line(
            r#"[15:46:07] LocalPlayer: ProcessRemoveFromStorageVault(302055, -1, 172708606, 1000)"#,
        );

        // There may be an ItemAdded event first; find the StorageWithdrawal.
        let prov = events
            .iter()
            .find_map(|ev| match ev {
                PlayerEvent::StorageWithdrawal { provenance, .. } => Some(provenance.clone()),
                _ => None,
            })
            .expect("StorageWithdrawal event present");

        match prov {
            ItemProvenance::Attributed {
                source, confidence, ..
            } => {
                match source {
                    ActivitySource::StorageBrowsing {
                        vault_owner_entity_id,
                        vault_name,
                    } => {
                        assert_eq!(vault_owner_entity_id, 302055);
                        assert_eq!(vault_name, "Saddlebag");
                    }
                    other => panic!("Expected StorageBrowsing, got {:?}", other),
                }
                assert_eq!(confidence, AttributionConfidence::Confident);
            }
            other => panic!("Expected Attributed StorageBrowsing, got {:?}", other),
        }
    }

    #[test]
    fn test_provenance_after_context_timeout_is_unknown() {
        // A mining context that times out before the item drops should
        // produce UnknownSource, not attribute to the stale context.
        let mut parser = PlayerEventParser::new();
        parser.process_line(
            r#"[12:00:00] LocalPlayer: ProcessDoDelayLoop(6, ChopLumber, "Mining...", 0, AbortIfAttacked, IsInteractorDelayLoop)"#,
        );
        // Item drops way past the 6s + 2s slack deadline
        let events = parser
            .process_line(r#"[12:01:00] LocalPlayer: ProcessAddItem(LateGain(777), -1, True)"#);
        assert_eq!(first_provenance(&events), ItemProvenance::UnknownSource);
    }

    // ============================================================
    // Chat-gain quantity seeding tests (Phase 3)
    // ============================================================

    #[test]
    fn test_chat_gain_seeds_stack_from_chat_quantity() {
        // Classic vendor-buy case: player bought 5 brains. Chat reports qty=5
        // while player.log's AddItem has no quantity info. The chat event
        // arrives slightly before the player.log AddItem (or vice versa);
        // the parser must correlate them and seed the stack to 5.
        let mut parser = PlayerEventParser::new();
        parser.feed_chat_gain("Brain".to_string(), 5, "15:56:03");

        parser
            .process_line(r#"[15:56:03] LocalPlayer: ProcessAddItem(Brain(187042326), -1, True)"#);

        // Stack should now be seeded at 5. A subsequent UpdateItemCode going
        // from 5 to 6 should report delta=+1, not +5 (as it would with the
        // old seed-to-1 behavior).
        let events = parser.process_line(
            // encoded 328790 → (328790 >> 16) + 1 = 6, typeId = 1110
            r#"[15:56:36] LocalPlayer: ProcessUpdateItemCode(187042326, 328790, True)"#,
        );
        let change = events
            .iter()
            .find_map(|ev| match ev {
                PlayerEvent::ItemStackChanged {
                    old_stack_size,
                    new_stack_size,
                    delta,
                    ..
                } => Some((*old_stack_size, *new_stack_size, *delta)),
                _ => None,
            })
            .expect("ItemStackChanged expected");

        assert_eq!(
            change.0, 5,
            "prior stack should have been seeded to 5 from chat"
        );
        assert_eq!(change.1, 6);
        assert_eq!(change.2, 1);
    }

    #[test]
    fn test_chat_gain_no_match_falls_back_to_one() {
        // AddItem with no matching chat gain in the buffer should still
        // seed to 1 (the prior behavior) — correct for single-item pickups.
        let mut parser = PlayerEventParser::new();
        parser
            .process_line(r#"[12:00:00] LocalPlayer: ProcessAddItem(Mushroom8(12345), -1, True)"#);
        assert_eq!(*parser.stack_sizes.get(&12345).unwrap(), 1);
    }

    #[test]
    fn test_chat_gain_outside_window_is_ignored() {
        // Chat gain far in the past shouldn't match a new AddItem.
        let mut parser = PlayerEventParser::new();
        parser.feed_chat_gain("Fluorite".to_string(), 3, "12:00:00");

        // AddItem 10 minutes later — outside the match window
        parser.process_line(r#"[12:10:00] LocalPlayer: ProcessAddItem(Fluorite(999), -1, True)"#);
        assert_eq!(
            *parser.stack_sizes.get(&999).unwrap(),
            1,
            "out-of-window chat gain must not seed"
        );
    }

    #[test]
    fn test_chat_gain_consumed_on_first_match() {
        // Two AddItems for the same item type, only one chat gain buffered.
        // The first AddItem should claim the chat quantity; the second falls
        // back to 1. This mirrors the real game behavior where a single
        // pickup can split across stacks — each chat line corresponds to one
        // destination stack.
        let mut parser = PlayerEventParser::new();
        parser.feed_chat_gain("Phlogiston7".to_string(), 750, "15:51:42");

        parser
            .process_line(r#"[15:51:42] LocalPlayer: ProcessAddItem(Phlogiston7(111), -1, True)"#);
        parser
            .process_line(r#"[15:51:42] LocalPlayer: ProcessAddItem(Phlogiston7(222), -1, True)"#);

        // First got the 750, second fell back
        assert_eq!(*parser.stack_sizes.get(&111).unwrap(), 750);
        assert_eq!(*parser.stack_sizes.get(&222).unwrap(), 1);
    }

    #[test]
    fn test_chat_gain_multiple_buffered_all_matchable() {
        // Split pickup case from paired log data: pickup of 1000 phlog
        // fires TWO chat lines (x750 + x250) for the two destination stacks.
        // Both should seed their respective AddItems.
        let mut parser = PlayerEventParser::new();
        parser.feed_chat_gain("Phlogiston7".to_string(), 750, "15:51:42");
        parser.feed_chat_gain("Phlogiston7".to_string(), 250, "15:51:42");

        parser
            .process_line(r#"[15:51:42] LocalPlayer: ProcessAddItem(Phlogiston7(111), -1, True)"#);
        parser
            .process_line(r#"[15:51:42] LocalPlayer: ProcessAddItem(Phlogiston7(222), -1, True)"#);

        // Both instances seeded from chat. Order is not guaranteed
        // (swap_remove reorders the buffer), so just check total seeded mass.
        let a = *parser.stack_sizes.get(&111).unwrap();
        let b = *parser.stack_sizes.get(&222).unwrap();
        assert_eq!(a + b, 1000);
        assert!((a == 750 && b == 250) || (a == 250 && b == 750));
    }

    #[test]
    fn test_chat_gain_different_item_no_cross_match() {
        // Chat gain for one item must not seed a different item.
        let mut parser = PlayerEventParser::new();
        parser.feed_chat_gain("Fluorite".to_string(), 3, "15:43:36");

        parser.process_line(r#"[15:43:36] LocalPlayer: ProcessAddItem(BlueSpinel(999), -1, True)"#);
        assert_eq!(
            *parser.stack_sizes.get(&999).unwrap(),
            1,
            "different item must not be seeded"
        );
    }

    // ============================================================
    // ProvenanceColumns tests (Phase 4)
    // ============================================================

    #[test]
    fn test_provenance_columns_attributed_mining() {
        let prov = ItemProvenance::Attributed {
            source: ActivitySource::Mining {
                node_entity_id: Some(435138),
                node_name: Some("MiningNodeFromSurvey9".to_string()),
            },
            confidence: AttributionConfidence::Confident,
            survey_use_id: None,
        };
        let cols = prov.to_columns();
        assert_eq!(cols.source_kind.as_deref(), Some("mining"));
        assert_eq!(cols.confidence.as_deref(), Some("confident"));
        let details = cols
            .source_details
            .expect("mining should have details JSON");
        assert!(details.contains("435138"));
        assert!(details.contains("MiningNodeFromSurvey9"));
    }

    #[test]
    fn test_provenance_columns_attributed_unnamed_mining() {
        // Nameless motherlode nodes — node_name None must serialize cleanly
        // without breaking downstream JSON parsers.
        let prov = ItemProvenance::Attributed {
            source: ActivitySource::Mining {
                node_entity_id: Some(1176954),
                node_name: None,
            },
            confidence: AttributionConfidence::Confident,
            survey_use_id: None,
        };
        let cols = prov.to_columns();
        assert_eq!(cols.source_kind.as_deref(), Some("mining"));
        let details = cols.source_details.unwrap();
        assert!(details.contains("1176954"));
        // Should include a JSON null for node_name — not a quoted empty string
        assert!(details.contains("\"node_name\":null"));
    }

    #[test]
    fn test_provenance_columns_uncertain_preserves_candidates() {
        let prov = ItemProvenance::Uncertain {
            candidates: vec![
                ActivitySource::Mining {
                    node_entity_id: Some(1),
                    node_name: None,
                },
                ActivitySource::CorpseSearch {
                    entity_id: 2,
                    corpse_name: "Rat".to_string(),
                },
            ],
        };
        let cols = prov.to_columns();
        assert_eq!(cols.source_kind.as_deref(), Some("uncertain"));
        assert_eq!(cols.confidence, None);
        let details = cols
            .source_details
            .expect("uncertain must preserve candidates");
        // Must contain both candidate kinds so future re-classification can
        // inspect what was active at the time.
        assert!(details.contains("Mining"));
        assert!(details.contains("CorpseSearch"));
        assert!(details.contains("Rat"));
    }

    #[test]
    fn test_provenance_columns_attributed_with_survey_use_id() {
        // A3 stitching: when a feature aggregator (the survey tracker) sets
        // survey_use_id, it must appear inside source_details JSON for query
        // joins via `source_details->>'survey_use_id'`.
        let prov = ItemProvenance::Attributed {
            source: ActivitySource::Mining {
                node_entity_id: Some(435138),
                node_name: Some("MiningNodeFromSurvey9".to_string()),
            },
            confidence: AttributionConfidence::Confident,
            survey_use_id: Some(42),
        };
        let cols = prov.to_columns();
        let details = cols.source_details.expect("details JSON expected");
        // Original Mining fields preserved
        assert!(details.contains("435138"));
        assert!(details.contains("MiningNodeFromSurvey9"));
        // Stitching link present
        assert!(details.contains("\"survey_use_id\":42"));

        // Round-trip parse to confirm the JSON is valid
        let parsed: serde_json::Value = serde_json::from_str(&details).unwrap();
        assert_eq!(
            parsed.get("survey_use_id").and_then(|v| v.as_i64()),
            Some(42)
        );
    }

    #[test]
    fn test_provenance_columns_attributed_survey_use_id_no_other_details() {
        // Edge case: SurveyMapCraft has no native details JSON. With a
        // survey_use_id the function must synthesize an object containing
        // only survey_use_id rather than producing invalid JSON.
        let prov = ItemProvenance::Attributed {
            source: ActivitySource::SurveyMapCraft,
            confidence: AttributionConfidence::Confident,
            survey_use_id: Some(7),
        };
        let cols = prov.to_columns();
        let details = cols.source_details.expect("synthesized details expected");
        let parsed: serde_json::Value = serde_json::from_str(&details).unwrap();
        assert_eq!(
            parsed.get("survey_use_id").and_then(|v| v.as_i64()),
            Some(7)
        );
    }

    #[test]
    fn test_provenance_columns_unknown_source() {
        let cols = ItemProvenance::UnknownSource.to_columns();
        assert_eq!(cols.source_kind.as_deref(), Some("unknown"));
        assert_eq!(cols.source_details, None);
        assert_eq!(cols.confidence, None);
    }

    #[test]
    fn test_provenance_columns_not_applicable() {
        let cols = ItemProvenance::NotApplicable.to_columns();
        assert_eq!(cols.source_kind.as_deref(), Some("not_applicable"));
        assert_eq!(cols.source_details, None);
        assert_eq!(cols.confidence, None);
    }

    #[test]
    fn test_provenance_columns_all_attributed_kinds() {
        // Sanity: every ActivitySource variant maps to a distinct source_kind.
        // If a new variant is added without updating activity_source_to_kind_and_details
        // the match will fail to compile — that's the intended guard.
        let cases = [
            (
                ActivitySource::SurveyMapCraft,
                "survey_map_craft",
                false, // no details
            ),
            (
                ActivitySource::SurveyMapUse {
                    survey_map_internal_name: Some("Serbule Blue Mineral Survey".to_string()),
                },
                "survey_map_use",
                true,
            ),
            (
                ActivitySource::GeneralCraft {
                    action_type: "Cook".to_string(),
                    label: "Baking Biscuits".to_string(),
                },
                "general_craft",
                true,
            ),
            (
                ActivitySource::VendorBrowsing {
                    npc_entity_id: 32857,
                    npc_name: Some("NPC_FarsightFlemmings".to_string()),
                },
                "vendor_browsing",
                true,
            ),
            (
                ActivitySource::StorageBrowsing {
                    vault_owner_entity_id: 302055,
                    vault_name: "Saddlebag".to_string(),
                },
                "storage_browsing",
                true,
            ),
        ];
        for (src, expected_kind, expects_details) in cases {
            let cols = ItemProvenance::Attributed {
                source: src,
                confidence: AttributionConfidence::Confident,
                survey_use_id: None,
            }
            .to_columns();
            assert_eq!(cols.source_kind.as_deref(), Some(expected_kind));
            assert_eq!(cols.confidence.as_deref(), Some("confident"));
            assert_eq!(
                cols.source_details.is_some(),
                expects_details,
                "{} details presence mismatch",
                expected_kind
            );
        }
    }

    #[test]
    fn test_chat_gain_storage_withdrawal_does_not_consume_chat_entry() {
        // Storage withdrawal AddItems (slot >= 0) get their qty from
        // RemoveFromStorageVault, not from chat. A chat gain meant for a
        // future real pickup must not be wrongly consumed here.
        let mut parser = PlayerEventParser::new();
        parser.feed_chat_gain("Phlogiston7".to_string(), 750, "15:47:00");

        // Storage withdrawal — slot >= 0 branch, should not consume chat gain
        parser
            .process_line(r#"[15:46:07] LocalPlayer: ProcessAddItem(Phlogiston7(111), 60, True)"#);
        parser.process_line(
            r#"[15:46:07] LocalPlayer: ProcessRemoveFromStorageVault(302055, -1, 111, 1000)"#,
        );
        assert_eq!(
            *parser.stack_sizes.get(&111).unwrap(),
            1000,
            "seeded from vault qty"
        );

        // Later, a real pickup: should claim the chat gain
        parser
            .process_line(r#"[15:47:00] LocalPlayer: ProcessAddItem(Phlogiston7(222), -1, True)"#);
        assert_eq!(*parser.stack_sizes.get(&222).unwrap(), 750);
    }

    // ── New game state parser tests ─────────────────────────────

    #[test]
    fn test_parse_celestial_info() {
        let mut parser = PlayerEventParser::new();
        let events = parser
            .process_line(r#"[23:32:47] LocalPlayer: ProcessSetCelestialInfo(WaxingCrescentMoon)"#);
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::MoonPhaseChanged { phase, .. } => {
                assert_eq!(phase, "WaxingCrescentMoon");
            }
            other => panic!("Expected MoonPhaseChanged, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_celestial_info_full_moon() {
        let mut parser = PlayerEventParser::new();
        let events =
            parser.process_line(r#"[12:00:00] LocalPlayer: ProcessSetCelestialInfo(FullMoon)"#);
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::MoonPhaseChanged { phase, .. } => {
                assert_eq!(phase, "FullMoon");
            }
            other => panic!("Expected MoonPhaseChanged, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_guild_general_info() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[15:16:24] LocalPlayer: ProcessGuildGeneralInfo(57, "SpaceMagic", "Welcome to the guild!")"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::GuildInfoLoaded {
                guild_id,
                guild_name,
                motd,
                ..
            } => {
                assert_eq!(*guild_id, 57);
                assert_eq!(guild_name, "SpaceMagic");
                assert_eq!(motd, "Welcome to the guild!");
            }
            other => panic!("Expected GuildInfoLoaded, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_guild_general_info_empty_motd() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[15:16:24] LocalPlayer: ProcessGuildGeneralInfo(42, "TestGuild", "")"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::GuildInfoLoaded {
                guild_id,
                guild_name,
                motd,
                ..
            } => {
                assert_eq!(*guild_id, 42);
                assert_eq!(guild_name, "TestGuild");
                assert_eq!(motd, "");
            }
            other => panic!("Expected GuildInfoLoaded, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_complete_directed_goals() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[15:16:24] LocalPlayer: ProcessCompleteDirectedGoals([3200,8000,1,70,3400,5000,])"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::DirectedGoalsLoaded { goal_ids, .. } => {
                assert_eq!(goal_ids, &[3200, 8000, 1, 70, 3400, 5000]);
            }
            other => panic!("Expected DirectedGoalsLoaded, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_complete_directed_goals_empty() {
        let mut parser = PlayerEventParser::new();
        let events =
            parser.process_line(r#"[15:16:24] LocalPlayer: ProcessCompleteDirectedGoals([])"#);
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::DirectedGoalsLoaded { goal_ids, .. } => {
                assert!(goal_ids.is_empty());
            }
            other => panic!("Expected DirectedGoalsLoaded, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_set_string_notepad() {
        let mut parser = PlayerEventParser::new();
        let events = parser
            .process_line(r#"[15:16:24] LocalPlayer: ProcessSetString(NOTEPAD, "My notes here")"#);
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::PlayerStringUpdated { key, value, .. } => {
                assert_eq!(key, "NOTEPAD");
                assert_eq!(value, "My notes here");
            }
            other => panic!("Expected PlayerStringUpdated, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_set_string_friend_status() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[15:16:24] LocalPlayer: ProcessSetString(FRIEND_STATUS, "110 Hammer, 101 Ment")"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PlayerEvent::PlayerStringUpdated { key, value, .. } => {
                assert_eq!(key, "FRIEND_STATUS");
                assert_eq!(value, "110 Hammer, 101 Ment");
            }
            other => panic!("Expected PlayerStringUpdated, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_set_string_skips_unknown_keys() {
        let mut parser = PlayerEventParser::new();
        let events = parser.process_line(
            r#"[15:16:24] LocalPlayer: ProcessSetString(MOUNT_APPEARANCE, "@Horse1(stuff)")"#,
        );
        assert!(events.is_empty(), "MOUNT_APPEARANCE should be skipped");
    }

    #[test]
    fn test_parse_set_string_all_notepad_tabs() {
        let mut parser = PlayerEventParser::new();
        for tab in &[
            "NOTEPAD_TAB_1",
            "NOTEPAD_TAB_2",
            "NOTEPAD_TAB_3",
            "NOTEPAD_TAB_4",
            "NOTEPAD_TAB_NAMES",
        ] {
            let line = format!(
                r#"[15:16:24] LocalPlayer: ProcessSetString({}, "content")"#,
                tab
            );
            let events = parser.process_line(&line);
            assert_eq!(events.len(), 1, "Should parse {}", tab);
            match &events[0] {
                PlayerEvent::PlayerStringUpdated { key, .. } => {
                    assert_eq!(key, *tab);
                }
                other => panic!("Expected PlayerStringUpdated for {}, got {:?}", tab, other),
            }
        }
    }

    // ── Poem parsing ─────────────────────────────────────────────────────

    #[test]
    fn test_parse_poem_basic() {
        let mut parser = PlayerEventParser::new();
        let line = r#"[05:07:17] LocalPlayer: ProcessTalkScreen(-51, "Poem by Snowkit", "<i>The following poem was spoken by Snowkit. Review it to receive Poetry Appreciation XP!</i>\n\n---\n\n<b>swiped snacks</b>\n\nA flash of red, a bushy tail,\nThrough garden green, a scent to hail.\n\nNo harm he means, just a quick peep,\nWhile happy folks their secrets keep.\n\n\n---\n\n<i>What did you think of the poem? Your review will be aggregated and sent to Snowkit. This rating has no impact on how much XP anybody earns, so be honest!</i>", "", [1,2,3,4,5,], System.String[], 0, Generic)"#;
        let events = parser.process_line(line);
        let poem = events
            .iter()
            .find_map(|e| match e {
                PlayerEvent::PoemRecorded {
                    author,
                    title,
                    content,
                    ..
                } => Some((author.clone(), title.clone(), content.clone())),
                _ => None,
            })
            .expect("should emit a PoemRecorded event");
        assert_eq!(poem.0, "Snowkit");
        assert_eq!(poem.1, "swiped snacks");
        // Intro/outro blurbs and fences stripped; stanzas preserved with newlines.
        assert!(poem.2.starts_with("A flash of red, a bushy tail,"));
        assert!(poem.2.ends_with("While happy folks their secrets keep."));
        assert!(!poem.2.contains("Poetry Appreciation"));
        assert!(!poem.2.contains("What did you think"));
        assert!(!poem.2.contains("---"));
        assert!(!poem.2.contains("swiped snacks")); // title not duplicated in body
        assert!(poem.2.contains('\n')); // real newlines, not literal \n
    }

    #[test]
    fn test_parse_poem_with_escaped_quotes() {
        // A poem whose body contains \"-escaped quotes must not be truncated.
        let mut parser = PlayerEventParser::new();
        let line = r#"[18:05:31] LocalPlayer: ProcessTalkScreen(-51, "Poem by TheScare", "<i>The following poem was spoken by TheScare. Review it to receive Poetry Appreciation XP!</i>\n\n---\n\n<b>Bees take over the world</b>\n\nIt started with a note: \"Give us the flowers.\"\nWe laughed. We wrote back \"lol no.\"\n\n\n---\n\n<i>What did you think of the poem? Your review will be aggregated and sent to TheScare. This rating has no impact on how much XP anybody earns, so be honest!</i>", "", [1,2,3,4,5,], System.String[], 0, Generic)"#;
        let events = parser.process_line(line);
        let content = events
            .iter()
            .find_map(|e| match e {
                PlayerEvent::PoemRecorded { content, .. } => Some(content.clone()),
                _ => None,
            })
            .expect("should emit a PoemRecorded event");
        assert!(
            content.contains(r#""Give us the flowers.""#),
            "got: {content}"
        );
        assert!(content.contains(r#""lol no.""#), "got: {content}");
    }

    #[test]
    fn test_parse_poem_title_only() {
        // Empty-body poem (just a bold title) still records, with empty content.
        let mut parser = PlayerEventParser::new();
        let line = r#"[05:13:28] LocalPlayer: ProcessTalkScreen(-51, "Poem by Banbu", "<i>The following poem was spoken by Banbu. Review it to receive Poetry Appreciation XP!</i>\n\n---\n\n<b>waaaaaaaaaaaaaaaaaaaaagh</b>\n\n\n\n---\n\n<i>What did you think of the poem? Your review will be aggregated and sent to Banbu. This rating has no impact on how much XP anybody earns, so be honest!</i>", "", [1,2,3,4,5,], System.String[], 0, Generic)"#;
        let events = parser.process_line(line);
        let poem = events
            .iter()
            .find_map(|e| match e {
                PlayerEvent::PoemRecorded {
                    author,
                    title,
                    content,
                    ..
                } => Some((author.clone(), title.clone(), content.clone())),
                _ => None,
            })
            .expect("should emit a PoemRecorded event");
        assert_eq!(poem.0, "Banbu");
        assert_eq!(poem.1, "waaaaaaaaaaaaaaaaaaaaagh");
        assert_eq!(poem.2, "");
    }

    #[test]
    fn test_poem_preserves_html_entities() {
        // The game HTML-entity-escapes player text (< > &); we keep it verbatim
        // so the view layer can render it safely.
        let mut parser = PlayerEventParser::new();
        let line = r#"[18:02:59] LocalPlayer: ProcessTalkScreen(-51, "Poem by Inne", "<i>The following poem was spoken by Inne. Review it to receive Poetry Appreciation XP!</i>\n\n---\n\n<b>Thanks</b>\n\nLove you Dreva People! &lt;3 &lt;3\n\n\n---\n\n<i>What did you think of the poem? Your review will be aggregated and sent to Inne. This rating has no impact on how much XP anybody earns, so be honest!</i>", "", [1,2,3,4,5,], System.String[], 0, Generic)"#;
        let events = parser.process_line(line);
        let content = events
            .iter()
            .find_map(|e| match e {
                PlayerEvent::PoemRecorded { content, .. } => Some(content.clone()),
                _ => None,
            })
            .expect("should emit a PoemRecorded event");
        assert!(content.contains("&lt;3 &lt;3"), "got: {content}");
    }

    #[test]
    fn test_talk_screen_negative_id_does_not_break_corpse_path() {
        // Sanity: a normal positive-id corpse search still parses after the
        // i64 entity-id change.
        let mut parser = PlayerEventParser::new();
        let line = r#"[18:04:38] LocalPlayer: ProcessTalkScreen(435269, "Search Corpse of Ratkin Miner", "", "", System.Int32[], System.String[], 0, Corpse)"#;
        let events = parser.process_line(line);
        assert!(events
            .iter()
            .any(|e| matches!(e, PlayerEvent::CorpseSearched { corpse_name, .. } if corpse_name == "Ratkin Miner")));
    }
}
