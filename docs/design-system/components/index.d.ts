export type SuggestionKind = "subcommand" | "option" | "path" | "argument";
/** [start, end) in code points of the label; ascending, non-overlapping, snapped to grapheme clusters by the adapter. */
export type MatchRange = [number, number];
export interface Suggestion { label: string; description?: string; kind?: SuggestionKind; matches?: MatchRange[] }
export interface KeycapProps { children: string }
export interface SuggestionRowProps { label: string; description?: string; kind?: SuggestionKind; selected?: boolean; matches?: MatchRange[]; id?: string }
export interface SuggestionPopupProps {
  items: Suggestion[]; selectedIndex?: number; maxRows?: number; context: string;
  /** Prefix for option ids: "<id>-opt-<index>"; the listbox itself gets "<id>". */
  id?: string;
  /** First visible index from the previous render, so the window moves one row at a time. */
  windowStart?: number;
}
export interface InlineFallbackProps { prompt?: string; before: string; insertion: string; after?: string; alternatives?: number }
export interface PermissionNoticeProps { capability: string; reason: string; denied?: boolean; recovery?: string }
