export type CombatLogKind = 'kill' | 'death' | 'level' | 'wisdom';

export interface CombatLogEntry {
  timestamp: string;
  kind: CombatLogKind;
  label: string;
  detail?: string;
}
