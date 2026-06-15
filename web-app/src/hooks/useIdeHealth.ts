import { useSyncExternalStore } from "react";
import {
  getIdeHealthSnapshot,
  type IdeHealthState,
  subscribeIdeHealth
} from "@/libs/utils/ideHealth";

/** Subscribe to the app-wide developer-environment availability signal. */
export default function useIdeHealth(): IdeHealthState {
  return useSyncExternalStore(subscribeIdeHealth, getIdeHealthSnapshot, getIdeHealthSnapshot);
}
