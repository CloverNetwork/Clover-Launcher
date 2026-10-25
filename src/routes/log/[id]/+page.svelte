<!-- A game log in its own window. The backend keeps recent output, so it starts complete. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { page } from "$app/state";
  import { api, events } from "$lib/api";
  import GameLog from "$lib/components/GameLog.svelte";

  const MAX_LOG_LINES = 2000;
  const id = page.params.id ?? "";
  let lines = $state<string[]>([]);

  onMount(() => {
    // Listen before loading what's already there, so nothing printed in between is lost.
    const unlisten = Promise.all([
      events.gameLog(({ instanceId, line }) => {
        if (instanceId !== id) return;
        lines.push(line);
        if (lines.length > MAX_LOG_LINES) lines.splice(0, lines.length - MAX_LOG_LINES);
      }),
      events.gameLogCleared((instanceId) => instanceId === id && (lines = [])),
    ]);
    api.gameLog(id).then((existing) => (lines = existing));
    return () => unlisten.then((fns) => fns.forEach((fn) => fn()));
  });
</script>

<GameLog instanceId={id} {lines} standalone onPopIn={() => api.popInLogWindow(id)} />
