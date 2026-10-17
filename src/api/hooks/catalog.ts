import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { Legacy } from "@inventory/shared";
import { api } from "../client";

type TokenArgs = { token: string };

export const catalogKeys = {
  categories: ["categories"] as const,
  brands: ["brands"] as const,
  units: ["units"] as const,
};

export function useCategories(args: TokenArgs) {
  return useQuery({
    queryKey: [...catalogKeys.categories, args.token],
    queryFn: () => api.call("list_categories_cmd", args),
  });
}

export function useBrands(args: TokenArgs) {
  return useQuery({
    queryKey: [...catalogKeys.brands, args.token],
    queryFn: () => api.call("list_brands_cmd", args),
  });
}

export function useUnits(args: TokenArgs) {
  return useQuery({
    queryKey: [...catalogKeys.units, args.token],
    queryFn: () => api.call("list_units_cmd", args),
  });
}

export function useCreateCategory() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (args: Legacy.CommandArgs<"create_category_cmd">) =>
      api.call("create_category_cmd", args),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: catalogKeys.categories }),
  });
}

export function useDeleteCategory() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (args: Legacy.CommandArgs<"delete_category_cmd">) =>
      api.call("delete_category_cmd", args),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: catalogKeys.categories }),
  });
}

export function useCreateBrand() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (args: Legacy.CommandArgs<"create_brand_cmd">) =>
      api.call("create_brand_cmd", args),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: catalogKeys.brands }),
  });
}

export function useDeleteBrand() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (args: Legacy.CommandArgs<"delete_brand_cmd">) =>
      api.call("delete_brand_cmd", args),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: catalogKeys.brands }),
  });
}

export function useCreateUnit() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (args: Legacy.CommandArgs<"create_unit_cmd">) => api.call("create_unit_cmd", args),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: catalogKeys.units }),
  });
}

export function useDeleteUnit() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (args: Legacy.CommandArgs<"delete_unit_cmd">) => api.call("delete_unit_cmd", args),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: catalogKeys.units }),
  });
}
