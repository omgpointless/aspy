/**
 * API Client
 *
 * HTTP client for communicating with Aspy's REST API.
 */

// Configuration
export const API_BASE = process.env.ASPY_API_URL ?? "http://127.0.0.1:8080";

// ============================================================================
// Types
// ============================================================================

export interface ApiError {
  error: string;
  status: number;
}

export type ApiResult<T> = { ok: true; data: T } | { ok: false; error: ApiError };

// ============================================================================
// Client
// ============================================================================

/**
 * Fetch data from Aspy's REST API.
 *
 * @param endpoint - API endpoint (e.g., "/api/stats")
 * @returns ApiResult with data or error
 */
export async function fetchApi<T>(endpoint: string): Promise<ApiResult<T>> {
  try {
    const response = await fetch(`${API_BASE}${endpoint}`);

    if (!response.ok) {
      return {
        ok: false,
        error: {
          error: `HTTP ${response.status}: ${response.statusText}`,
          status: response.status,
        },
      };
    }

    const data = (await response.json()) as T;
    return { ok: true, data };
  } catch (err) {
    const message = err instanceof Error ? err.message : "Unknown error";
    return {
      ok: false,
      error: {
        error: `Failed to connect to Aspy: ${message}`,
        status: 0,
      },
    };
  }
}

/**
 * Convenience type for tool content results.
 */
export interface ToolContent {
  type: "text";
  text: string;
}

/**
 * Create error response content for tool handlers.
 */
export function errorContent(message: string): { content: ToolContent[]; isError: true } {
  return {
    content: [{ type: "text" as const, text: `Error: ${message}` }],
    isError: true,
  };
}

/**
 * Create success response content for tool handlers.
 */
export function successContent<T>(
  summary: string,
  data: T
): { content: ToolContent[]; structuredContent: T } {
  return {
    content: [
      { type: "text" as const, text: summary },
      { type: "text" as const, text: JSON.stringify(data, null, 2) },
    ],
    structuredContent: data,
  };
}

/**
 * Create simple text response content.
 */
export function textContent<T>(
  text: string,
  data: T
): { content: ToolContent[]; structuredContent: T } {
  return {
    content: [{ type: "text" as const, text }],
    structuredContent: data,
  };
}
