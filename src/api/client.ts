import { invoke } from "@tauri-apps/api/core";
import type { Legacy } from "@inventory/shared";
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
