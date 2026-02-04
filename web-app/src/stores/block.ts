import { create } from "zustand";
import { devtools } from "zustand/middleware";
import { createEventSlice, type EventSlice } from "./slices/event";
import { createGroupSlice, type GroupSlice } from "./slices/group";
import { createSelectSlice, type SelectSlice } from "./slices/select";

export const useBlockStore = create<EventSlice & GroupSlice & SelectSlice>()(
  devtools((...a) => ({
    ...createGroupSlice(...a),
    ...createSelectSlice(...a),
    ...createEventSlice(...a)
  }))
);
