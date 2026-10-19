import type { UseQueryOptions } from "@tanstack/react-query";
import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import type { CallOptions, CommandName, Input, Output } from "@/api/v2";
import { ApiError, call } from "@/api/v2";
import { useSession } from "@/stores/session";

type QueryOpts<K extends CommandName> = Omit<
  UseQueryOptions<Output<K>, ApiError>,
  "queryKey" | "queryFn"
>;

/** A read command as a cached query. The key is the command name and its input. */
export function useCommand<K extends CommandName>(
  command: K,
  input: Input<K>,
  options: QueryOpts<K> = {},
) {
  const signedIn = useSession((s) => s.status === "signedIn");
  return useQuery<Output<K>, ApiError>({
    queryKey: [command, input],
    queryFn: () => call(command, input),
    ...options,
    enabled: signedIn && (options.enabled ?? true),
  });
}

/** A paged or filtered list: the previous page stays on screen while the next one loads. */
export function useList<K extends CommandName>(
  command: K,
  input: Input<K>,
  options: QueryOpts<K> = {},
) {
  return useCommand(command, input, { placeholderData: keepPreviousData, ...options });
}

/** The message to show for a failed action (the backend's own text is written for people). */
export function errorText(error: unknown): string {
  if (error instanceof ApiError) return error.message;
  return "Something went wrong. Please try again.";
}

interface ActionOptions<K extends CommandName> {
  /** Commands whose cached results change: their queries are refreshed. */
  invalidates?: CommandName[];
  /** Toast after success. */
  success?: string | ((result: Output<K>) => string);
  onSuccess?: (result: Output<K>, input: Input<K>) => void;
  /** Errors are shown as a toast unless the screen handles them itself. */
  quiet?: boolean;
}

/** A write command. Approvals are asked for by `call`; errors become a toast. */
export function useAction<K extends CommandName>(command: K, options: ActionOptions<K> = {}) {
  const queryClient = useQueryClient();
  return useMutation<Output<K>, ApiError, { input: Input<K>; options?: CallOptions }>({
    mutationFn: ({ input, options: callOptions }) => call(command, input, callOptions),
    onSuccess: (result, { input }) => {
      for (const name of options.invalidates ?? []) {
        void queryClient.invalidateQueries({ queryKey: [name] });
      }
      if (options.success) {
        toast.success(
          typeof options.success === "function" ? options.success(result) : options.success,
        );
      }
      options.onSuccess?.(result, input);
    },
    onError: (error) => {
      if (options.quiet) return;
      // A cancelled approval is the user's own choice, not an error to shout about.
      if (error.code === "APPROVAL_REQUIRED") {
        toast.info("Cancelled: this needs a manager's approval.");
        return;
      }
      if (error.code !== "UNAUTHORIZED") toast.error(errorText(error));
    },
  });
}

/** The field a VALIDATION error points at (`details.field`), for inline messages. */
export function fieldError(error: unknown, field: string): string | undefined {
  if (!(error instanceof ApiError) || error.code !== "VALIDATION") return undefined;
  const details = error.details as { field?: unknown } | undefined;
  return details?.field === field ? error.message : undefined;
}
