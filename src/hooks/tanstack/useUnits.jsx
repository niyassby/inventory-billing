import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";

export const useUnits = (data) => {
    return useQuery({
        queryKey: ["units", data],
        queryFn: async () => await invoke("list_units_cmd", data),
    });
};

export const useCreateUnit = () => {
    const queryClient = useQueryClient();
    return useMutation({
        mutationFn: async (data) => await invoke("create_unit_cmd", data),
        onSuccess: () => {
            queryClient.invalidateQueries({
                queryKey: ["units"],
            });
        },
    });
};

export const useDeleteUnit = () => {
    const queryClient = useQueryClient();
    return useMutation({
        mutationFn: async (data) => await invoke("delete_unit_cmd", data),
        onSuccess: () => {
            queryClient.invalidateQueries({
                queryKey: ["units"],
            });
        },
    });
};
