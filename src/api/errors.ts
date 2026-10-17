import type { ErrorCode } from "@inventory/shared";
import { isErrorCode } from "@inventory/shared";

/**
 * The one error type the UI handles. Whatever the backend or transport throws is normalised into
 * an ApiError with a stable `code`, so screens branch on codes and never on message text.
 */
export class ApiError extends Error {
  override name = "ApiError";

  constructor(
    readonly code: ErrorCode,
    message: string,
    readonly details?: unknown,
  ) {
    super(message);
  }

  /** Legacy pages build messages with `"Failed: " + error`; keep that readable. */
  override toString(): string {
    return this.message;
  }
}

/** Legacy (v1) commands return plain strings; these prefixes carry the meaning. */
const LEGACY_PREFIXES: readonly [RegExp, ErrorCode][] = [
  [/^UNAUTHORIZED:\s*/i, "UNAUTHORIZED"],
  [/^Session invalid/i, "UNAUTHORIZED"],
  [/^Login failed:\s*/i, "UNAUTHORIZED"],
  [/^FORBIDDEN:\s*/i, "FORBIDDEN"],
  [/UNIQUE constraint failed/i, "CONFLICT"],
  [/FOREIGN KEY constraint failed/i, "CONFLICT"],
  [/Query returned no rows/i, "NOT_FOUND"],
];

const FRIENDLY_MESSAGES: Partial<Record<ErrorCode, string>> = {
  CONFLICT: "This record conflicts with an existing one (for example a duplicate SKU or number).",
  NOT_FOUND: "The record was not found. It may have been deleted.",
  INTERNAL: "Something went wrong. Please try again.",
};

function fromStructured(value: Record<string, unknown>): ApiError | null {
  const { code, message, details } = value;
  if (typeof code === "string" && isErrorCode(code)) {
    return new ApiError(code, typeof message === "string" ? message : code, details);
  }
  return null;
}

function fromLegacyString(raw: string): ApiError {
  for (const [pattern, code] of LEGACY_PREFIXES) {
    if (pattern.test(raw)) {
      const stripped = raw.replace(pattern, "").trim();
      const message = FRIENDLY_MESSAGES[code] ?? (stripped || raw);
      return new ApiError(code, message, { raw });
    }
  }
  return new ApiError("INTERNAL", raw || (FRIENDLY_MESSAGES.INTERNAL ?? "Error"), { raw });
}

export function toApiError(error: unknown): ApiError {
  if (error instanceof ApiError) return error;
  if (typeof error === "string") return fromLegacyString(error);
  if (typeof error === "object" && error !== null) {
    const structured = fromStructured(error as Record<string, unknown>);
    if (structured) return structured;
  }
  if (error instanceof Error) return fromLegacyString(error.message);
  return new ApiError("INTERNAL", FRIENDLY_MESSAGES.INTERNAL ?? "Error", { raw: error });
}
