import { useEffect, useRef } from "react";

type TaskChannel = "generation" | "repair" | "editor";
const channels: TaskChannel[] = ["generation", "repair", "editor"];

interface ChannelState {
  version: number;
  controller: AbortController | null;
}

/**
 * Each operation owns one channel version. Starting, resetting or unmounting
 * invalidates old results even when a transport ignores AbortSignal.
 */
export function useWorkbenchTasks() {
  const mountedRef = useRef(true);
  const channelRef = useRef<Record<TaskChannel, ChannelState>>({
    generation: { version: 0, controller: null },
    repair: { version: 0, controller: null },
    editor: { version: 0, controller: null },
  });

  function invalidate(...selected: TaskChannel[]) {
    for (const channel of selected.length ? selected : channels) {
      const state = channelRef.current[channel];
      state.version += 1;
      state.controller?.abort();
      state.controller = null;
    }
  }

  function capture(...selected: TaskChannel[]) {
    const versions = (selected.length ? selected : channels)
      .map((channel) => [channel, channelRef.current[channel].version] as const);
    return () => mountedRef.current && versions.every(
      ([channel, version]) => channelRef.current[channel].version === version,
    );
  }

  function begin(channel: TaskChannel, abortable = false) {
    invalidate(channel);
    const state = channelRef.current[channel];
    if (abortable) state.controller = new AbortController();
    return { isCurrent: capture(channel), signal: state.controller?.signal };
  }

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      invalidate();
    };
  }, []);

  return { begin, capture, invalidate };
}
