import { create } from "zustand";
import type { Api } from "@inventory/shared";
import type { ApprovalRequest } from "@/api/v2";
import { setApprovalRequester } from "@/api/v2";

interface Pending extends ApprovalRequest {
  resolve: (approval: Api.Approval | null) => void;
}

interface ApprovalState {
  pending: Pending | null;
  request: (request: ApprovalRequest) => Promise<Api.Approval | null>;
  answer: (approval: Api.Approval | null) => void;
}

/** One manager-approval prompt at a time; the dialog (ApprovalDialog) answers it. */
export const useApproval = create<ApprovalState>()((set, get) => ({
  pending: null,
  request(request) {
    // A second request while one is open cancels the first.
    get().pending?.resolve(null);
    return new Promise((resolve) => {
      set({ pending: { ...request, resolve } });
    });
  },
  answer(approval) {
    const pending = get().pending;
    set({ pending: null });
    pending?.resolve(approval);
  },
}));

setApprovalRequester((request) => useApproval.getState().request(request));
