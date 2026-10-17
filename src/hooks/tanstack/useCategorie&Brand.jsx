import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";

export const useCategories = (data) => {
  return useQuery({
    queryKey: ["categories", data],
    queryFn: async () => await invoke("list_categories_cmd", data),
  });
};

export const useBrands = (data) => {
  return useQuery({
    queryKey: ["brands", data],
    queryFn: async () => await invoke("list_brands_cmd", data),
  });
};

export const useCreateCategorie = () => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (data) => await invoke("create_category_cmd", data),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ["categories"],
      });
    },
  });
};
export const useCreateBrand = () => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (data) => await invoke("create_brand_cmd", data),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ["brands"],
      });
    },
  });
};

export const useDeleteCategory = () => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (data) => await invoke("delete_category_cmd", data),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ["categories"],
      });
    },
  });
};
export const useDeleteBrand = () => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (data) => await invoke("delete_brand_cmd", data),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ["brands"],
      });
    },
  });
};
