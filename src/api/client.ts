import { invoke } from "@tauri-apps/api/core";
import type { Api, Legacy } from "@inventory/shared";
import { toApiError } from "./errors";

type Commands = Legacy.LegacyCommands;
type CommandName = Legacy.LegacyCommandName;
type Args<K extends CommandName> = Legacy.CommandArgs<K>;
type Result<K extends CommandName> = Legacy.CommandResult<K>;

/**
 * How a command reaches the backend. Today: Tauri IPC on the same machine. Phase 4 adds an HTTP
 * transport for client devices talking to the LAN hub; screens don't change.
 */
export interface Transport {
  call(command: string, args: Record<string, unknown>): Promise<unknown>;
}

export const tauriTransport: Transport = {
  call: (command, args) => invoke(command, args),
};

export interface ApiClient {
  call<K extends CommandName>(command: K, args: Args<K>): Promise<Result<K>>;
}

/**
 * Typed client over a transport. Every failure is rethrown as an ApiError.
 *
 * The result cast is the one trust boundary between the Rust contract and TypeScript; it goes away
 * when Phase 3 generates these types from the Rust DTOs.
 */
export function createApiClient(transport: Transport): ApiClient {
  return {
    async call<K extends keyof Commands>(command: K, args: Args<K>): Promise<Result<K>> {
      try {
        return (await transport.call(command, args as Record<string, unknown>)) as Result<K>;
      } catch (error) {
        throw toApiError(error);
      }
    },
  };
}

export const api = createApiClient(tauriTransport);

// ---------------------------------------------------------------------------------------------
// v2: the typed backend (src-tauri/src/api). Types are generated from the Rust contract.

type V2Name = Api.CommandName;

/** Where the v2 client finds the current session token (empty when signed out). */
export type TokenSource = () => string | null;

export interface V2Client {
  call<K extends V2Name>(
    command: K,
    input: Api.CommandInput<K>,
    approval?: Api.Approval,
  ): Promise<Api.CommandOutput<K>>;
}

/**
 * Typed v2 client. Every command receives `{ token, input, approval }`; public commands (setup,
 * sign-in) ignore the token. Failures arrive as `{ code, message, details }` and become ApiErrors.
 */
export function createV2Client(transport: Transport, token: TokenSource): V2Client {
  return {
    async call<K extends V2Name>(
      command: K,
      input: Api.CommandInput<K>,
      approval?: Api.Approval,
    ): Promise<Api.CommandOutput<K>> {
      try {
        const args = { token: token() ?? "", input, approval: approval ?? null };
        return await transport.call(command, args);
      } catch (error) {
        throw toApiError(error);
      }
    },
  };
}
