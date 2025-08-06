import { create } from "zustand";
import { createEventSlice, EventSlice } from "./slices/event";
import { createSelectSlice, SelectSlice } from "./slices/select";
import { createGroupSlice, GroupSlice } from "./slices/group";
import { devtools } from "zustand/middleware";

export const useBlockStore = create<EventSlice & GroupSlice & SelectSlice>()(
  devtools((...a) => ({
    ...createGroupSlice(...a),
    ...createSelectSlice(...a),
    ...createEventSlice(...a),
  })),
);
