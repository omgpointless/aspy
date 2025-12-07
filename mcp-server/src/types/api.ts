/**
 * API Response Types
 *
 * Type definitions matching Rust API responses from Aspy.
 */

// ============================================================================
// Session Types
// ============================================================================

export interface StatsResponse {
  session: {
    started: string | null;
    duration_secs: number;
    events_count: number;
  };
  tokens: {
    input: number;
    output: number;
    cached: number;
    cache_created: number;
    cache_ratio_pct: number;
  };
  cost: {
    total_usd: number;
    savings_usd: number;
    by_model: Record<string, number>;
  };
  requests: {
    total: number;
    failed: number;
    success_rate_pct: number;
    avg_ttfb_ms: number;
  };
  tools: {
    total_calls: number;
    failed_calls: number;
    by_tool: Record<string, number>;
  };
  thinking: {
    blocks: number;
    total_tokens: number;
  };
}

export interface EventsResponse {
  [key: string]: unknown;
  total_in_buffer: number;
  returned: number;
  events: ProxyEvent[];
}

export interface ProxyEvent {
  type: string;
  timestamp: string;
  [key: string]: unknown;
}

export interface ContextResponse {
  [key: string]: unknown;
  current_tokens: number;
  limit_tokens: number;
  usage_pct: number;
  warning_level: "normal" | "warning" | "high" | "critical";
  compacts: number;
  breakdown: {
    input: number;
    cached: number;
  };
}

export interface SessionsResponse {
  active_count: number;
  sessions: Array<{
    key: string;
    user_id: string;
    claude_session_id: string | null;
    source: string;
    started: string;
    status: string;
    event_count: number;
    stats: {
      requests: number;
      tool_calls: number;
      input_tokens: number;
      output_tokens: number;
      cost_usd: number;
    };
  }>;
}

export interface WhoamiResponse {
  [key: string]: unknown;
  user_id: string;
  session_id?: string;
  claude_session_id?: string;
  session_started?: string;
  session_source?: string;
  session_status?: string;
  transcript_path?: string;
}

export interface SessionHistoryItem {
  session_id: string;
  user_id: string;
  claude_session_id?: string;
  started: string;
  ended?: string;
  source: string;
  end_reason?: string;
  transcript_path?: string;
  stats: {
    requests: number;
    tool_calls: number;
    input_tokens: number;
    output_tokens: number;
    cost_usd: number;
  };
}

export interface SessionHistoryResponse {
  [key: string]: unknown;
  user_id: string;
  count: number;
  has_more: boolean;
  sessions: SessionHistoryItem[];
}

export interface ContextSnapshotResponse {
  [key: string]: unknown;
  user_id: string;
  available: boolean;
  message_count: number;
  summary: string;
  breakdown: {
    tool_results: { count: number; chars: number; estimated_tokens: number; pct: number };
    tool_inputs: { count: number; chars: number; estimated_tokens: number; pct: number };
    thinking: { count: number; chars: number; estimated_tokens: number; pct: number };
    text: { count: number; chars: number; estimated_tokens: number; pct: number };
    system: { count: number; chars: number; estimated_tokens: number; pct: number };
  };
}

export interface TodosResponse {
  [key: string]: unknown;
  user_id: string;
  updated: string | null;
  count: number;
  summary: { pending: number; in_progress: number; completed: number };
  todos: Array<{ content: string; status: string; activeForm: string }>;
}

// ============================================================================
// Memory/Recall Types
// ============================================================================

export type MatchType = "thinking" | "user_prompt" | "assistant_response";

export interface ContextMatch {
  match_type: MatchType;
  session_id: string | null;
  timestamp: string;
  content: string;
  rank: number;
}

export interface HybridContextResponse {
  [key: string]: unknown;
  topic: string;
  mode: string;
  search_type: string; // "fts_only" or "hybrid"
  results: ContextMatch[];
}

export interface ThinkingMatch {
  session_id: string | null;
  timestamp: string;
  content: string;
  tokens: number | null;
  rank: number;
}

export interface PromptMatch {
  session_id: string | null;
  timestamp: string;
  content: string;
  rank: number;
}

export interface ResponseMatch {
  session_id: string | null;
  timestamp: string;
  content: string;
  rank: number;
}

export interface ThinkingSearchResponse {
  [key: string]: unknown;
  query: string;
  mode: string;
  results: ThinkingMatch[];
}

export interface PromptSearchResponse {
  [key: string]: unknown;
  query: string;
  mode: string;
  results: PromptMatch[];
}

export interface ResponseSearchResponse {
  [key: string]: unknown;
  query: string;
  mode: string;
  results: ResponseMatch[];
}

export interface TodoMatch {
  session_id: string | null;
  timestamp: string;
  content: string;
  todos_json: string;
  pending_count: number;
  in_progress_count: number;
  completed_count: number;
  rank: number;
}

export interface TodoSearchResponse {
  [key: string]: unknown;
  query: string | null;
  timeframe: string | null;
  results: TodoMatch[];
}

// ============================================================================
// Lifetime Types
// ============================================================================

export interface ModelStats {
  [key: string]: unknown;
  model: string;
  tokens: number;
  cost_usd: number;
  calls: number;
}

export interface ToolStats {
  [key: string]: unknown;
  tool: string;
  calls: number;
  avg_duration_ms: number;
  success_rate: number;
  rejections: number;
  errors: number;
}

export interface LifetimeStats {
  [key: string]: unknown;
  total_sessions: number;
  total_tokens: number;
  total_cost_usd: number;
  total_tool_calls: number;
  total_thinking_blocks: number;
  total_prompts: number;
  first_session: string | null;
  last_session: string | null;
  by_model: ModelStats[];
  by_tool: ToolStats[];
}

export interface EmbeddingStatusResponse {
  [key: string]: unknown;
  enabled: boolean;
  provider: string;
  model: string;
  dimensions: number;
  documents_embedded: number;
  documents_total: number;
  progress_pct: number;
}
