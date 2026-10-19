import type { Api } from "@inventory/shared";
import type { V2Client } from "./client";
import { createV2Client } from "./client";
import { ApiError, toApiError } from "./errors";
import { defaultTransport } from "./transport";

export type CommandName = Api.CommandName;
export type Input<K extends CommandName> = Api.CommandInput<K>;
export type Output<K extends CommandName> = Api.CommandOutput<K>;

/** What the manager-approval dialog is asked to show. */
export interface ApprovalRequest {
  /** "give a discount", "void an invoice" … (from the backend). */
  reason: string;
  /** The previous attempt's problem ("Approval credentials are incorrect."). */
  error?: string;
}

export type ApprovalRequester = (request: ApprovalRequest) => Promise<Api.Approval | null>;

export interface CallerDeps {
  client: V2Client;
  /** Shows the approval dialog; null when the user cancels. */
  requestApproval: ApprovalRequester;
  /** The session ended (UNAUTHORIZED) or must change its password first. */
  onAuthError: (error: ApiError) => void;
}

export interface CallOptions {
  approval?: Api.Approval;
}

/** How often a wrong manager PIN may be retried in one go. */
const MAX_APPROVAL_TRIES = 3;

function approvalReason(error: ApiError): string {
  const details = error.details as { reason?: unknown } | undefined;
  return typeof details?.reason === "string" ? details.reason : error.message;
}

/**
 * Calls a v2 command. When the backend answers APPROVAL_REQUIRED, asks for a manager's PIN or
 * password and sends the same request again with the approval (a wrong PIN asks again). Session
 * problems are reported to `onAuthError`. Every failure is an `ApiError`.
 */
export function createCaller(deps: CallerDeps) {
  return async function call<K extends CommandName>(
    command: K,
    input: Input<K>,
    options: CallOptions = {},
  ): Promise<Output<K>> {
    let approval = options.approval;
    let tries = 0;
    let reason = "";
    for (;;) {
      try {
        return await deps.client.call(command, input, approval);
      } catch (raw) {
        const error = toApiError(raw);
        const wrongApproval = approval !== undefined && error.code === "FORBIDDEN" && tries > 0;
        if (error.code === "APPROVAL_REQUIRED" || wrongApproval) {
          if (error.code === "APPROVAL_REQUIRED") reason = approvalReason(error);
          if (tries >= MAX_APPROVAL_TRIES) throw error;
          tries += 1;
          const next = await deps.requestApproval({
            reason,
            ...(wrongApproval ? { error: error.message } : {}),
          });
          if (!next) throw error;
          approval = next;
          continue;
        }
        if (error.code === "UNAUTHORIZED" || error.code === "PASSWORD_CHANGE_REQUIRED") {
          deps.onAuthError(error);
        }
        throw error;
      }
    }
  };
}

export type Call = ReturnType<typeof createCaller>;

// ---------------------------------------------------------------------------------------------
// The app's instance. The session and approval stores plug themselves in at start-up.

let tokenSource: () => string | null = () => null;
let approvalRequester: ApprovalRequester = () => Promise.resolve(null);
let authErrorHandler: (error: ApiError) => void = () => undefined;

export function setTokenSource(source: () => string | null): void {
  tokenSource = source;
}

export function setApprovalRequester(requester: ApprovalRequester): void {
  approvalRequester = requester;
}

export function setAuthErrorHandler(handler: (error: ApiError) => void): void {
  authErrorHandler = handler;
}

const client = createV2Client(defaultTransport(), () => tokenSource());

export const call: Call = createCaller({
  client,
  requestApproval: (request) => approvalRequester(request),
  onAuthError: (error) => {
    authErrorHandler(error);
  },
});

/** A fresh request id for writes, so a retried click never creates a second document. */
export function requestId(): string {
  return crypto.randomUUID();
}

export { ApiError };
