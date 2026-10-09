export type Mode = 'off' | 'ask' | 'auto';
export interface Permission { id: string; name: string; observe: boolean; volume: Mode; sensitive: boolean; explicit: boolean }
export interface Context { app_id: string; app_name: string; device: string; hour: number; weekday: number | null; local_day: number | null; session: string; recent: string[]; activity: string | null; power: string | null }
export interface Settings { paused: boolean; automatic_learning: boolean; auto_adjustments:boolean; time_recommendations: boolean; floating_cards: boolean; associations:boolean; website_observation:boolean; app_min_days:number; app_threshold:number; app_margin:number; suggest_threshold: number; auto_threshold: number; min_auto_samples: number; min_auto_sessions: number; max_volume: number; max_auto_delta: number; retention_days: number }
export interface Proposal { id: string; context: Context; created: number; prediction: { target: number; confidence: number; similarity: number; feedback_mean: number; samples: number; sessions: number; reason: string } }
export interface Journal { id: string; app_name: string; device: string; before: number; after: number; status: string; at: number; undoable: boolean }
export interface Status { experience:ExperienceStatus; demo: boolean; settings: Settings; current: Context | null; volume: number | null; apps: Permission[]; proposal: Proposal | null; journal: Journal[]; samples: number; events: {app_name: string; kind: string; at: number}[]; event_count: number; browser_last_event:number; message: string; memories: MemoryItem[]; targets: LaunchTarget[]; app_recommendation: AppRecommendation | null; quick_apps:RankedApp[]; capabilities:Capability[]; extensions:ExtensionInfo[]; startup:StartupStatus }

export interface MemoryItem { app_id:string; name:string; pinned:boolean; importance:number; last_used:number; days:number; samples:number }
export interface LaunchTarget {kind:string; app_id:string; name:string; enabled:boolean }
export interface AppRecommendation { id:string; app_id:string; app_name:string; confidence:number; days:number; reason:string; created:number }

export interface RankedApp {kind:string;app_id:string;name:string;score:number;overall_score:number;period_score:number;association_score:number;days:number;period_days:number;active:boolean}
export interface Capability {id:string;name:string;available:boolean;autonomous:boolean;authorization:string}
export interface ExtensionInfo {id:string;name:string;api_version:number;scope:string}
export interface StartupStatus {supported:boolean;enabled:boolean;simulated:boolean;message:string}

export type ExperienceKind='volume'|'brightness'|'app_choice';
export interface Experience {id:string;kind:ExperienceKind;scope:{app_id:string;app_name:string;device:string;period:number;activity:string};candidates:{id:string;name:string}[];evidence:number;revision:number;last_at:number;active:boolean;broad:boolean;preferred:string|null;wake:string;advice:{interpretation:'keep_scene'|'hold_app_device'|'choose_app'|'no_change';app_id:string|null;explanation:string}|null;awaiting_confirmation:boolean}
export interface ExperienceStatus {rules:Experience[];pending:Experience[];question:Experience|null;local_hits:number;automatic_calls_today:number}
