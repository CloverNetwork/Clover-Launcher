<!-- One stroke-based icon set, weight-matched to 13px system text. Always decorative: label the control instead. -->
<script lang="ts">
  const paths = {
    plus: "M8 3.5v9M3.5 8h9",
    sidebar: "M2.5 3.5h11v9h-11zM6 3.5v9",
    play: "M5 3.2v9.6L12.5 8z",
    stop: "M4.5 4.5h7v7h-7z",
    folder: "M2.5 4.5a1 1 0 0 1 1-1h2.8l1.4 1.5h4.8a1 1 0 0 1 1 1v5.5a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1z",
    person: "M8 8a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5zM3 13.5c.6-2.3 2.6-3.5 5-3.5s4.4 1.2 5 3.5",
    log: "M2.5 3.5h11v9h-11zM5 6.5l2 1.5-2 1.5M8.5 10h2.5",
    gear: "M8 10a2 2 0 1 0 0-4 2 2 0 0 0 0 4zM8 1.8v1.7M8 12.5v1.7M1.8 8h1.7M12.5 8h1.7M3.6 3.6l1.2 1.2M11.2 11.2l1.2 1.2M3.6 12.4l1.2-1.2M11.2 4.8l1.2-1.2",
    warning: "M8 2.5l6 10.5H2zM8 6.5v3M8 11.2v.3",
  } as const;

  let { name, size = 16 }: { name: keyof typeof paths; size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 16 16"
  fill={name === "play" || name === "stop" ? "currentColor" : "none"}
  stroke="currentColor"
  stroke-width="1.4"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <path d={paths[name]} />
</svg>
