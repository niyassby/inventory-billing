import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { Legacy } from "@inventory/shared";
import { api } from "../client";

type ProductListArgs = Legacy.CommandArgs<"list_products_filtered_cmd">;

export const productKeys = {
  all: ["products"] as const,
  list: (args: ProductListArgs) => ["products", "list", args] as const,
};

export function useProducts(args: ProductListArgs) {
  return useQuery({
    queryKey: productKeys.list(args),
    queryFn: () => api.call("list_products_filtered_cmd", args),
    placeholderData: keepPreviousData,
  });
}

export type SaveProductRequest =
  | { cmd: "create_product_cmd"; data: Legacy.CommandArgs<"create_product_cmd"> }
  | { cmd: "update_product_cmd"; data: Legacy.CommandArgs<"update_product_cmd"> };

export function useCreateOrUpdateProduct() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (request: SaveProductRequest) =>
      request.cmd === "create_product_cmd"
        ? api.call("create_product_cmd", request.data)
        : api.call("update_product_cmd", request.data),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: productKeys.all }),
  });
}

export function useDeleteProduct() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (args: Legacy.CommandArgs<"delete_product_cmd">) =>
      api.call("delete_product_cmd", args),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: productKeys.all }),
  });
}
