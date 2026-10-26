<!-- One stroke-based icon set, weight-matched to 13px system text. Always decorative: label the control instead. -->
<script lang="ts">
  const paths = {
    plus: "M8 3.5v9M3.5 8h9",
    sidebar: "M2.5 3.5h11v9h-11zM6 3.5v9",
    grid: "M2.5 2.5h4.5v4.5h-4.5zM9 2.5h4.5v4.5H9zM2.5 9h4.5v4.5h-4.5zM9 9h4.5v4.5H9z",
    back: "M10 3.5L5.5 8l4.5 4.5",
    play: "M5 3.2v9.6L12.5 8z",
    stop: "M4.5 4.5h7v7h-7z",
    folder: "M2.5 4.5a1 1 0 0 1 1-1h2.8l1.4 1.5h4.8a1 1 0 0 1 1 1v5.5a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1z",
    person: "M8 8a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5zM3 13.5c.6-2.3 2.6-3.5 5-3.5s4.4 1.2 5 3.5",
    log: "M2.5 3.5h11v9h-11zM5 6.5l2 1.5-2 1.5M8.5 10h2.5",
    upload: "M8 10.5V2.5M5 5.5l3-3 3 3M3 9.5v3a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1v-3",
    sort: "M4 3v10M2 11l2 2 2-2M10 4h4M10 8h3M10 12h2",
    refresh: "M13 8a5 5 0 1 1-1.5-3.55M13 2.5v3h-3",
    fullscreen: "M2.5 6V2.5H6M10 2.5h3.5V6M13.5 10v3.5H10M6 13.5H2.5V10",
    close: "M4 4l8 8M12 4l-8 8",
    trash: "M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.6 8.1a1 1 0 0 0 1 .9h3.8a1 1 0 0 0 1-.9l.6-8.1",
    search: "M7 12a5 5 0 1 0 0-10 5 5 0 0 0 0 10zM14 14l-3.5-3.5",
    popin: "M13.5 2.5L8.5 7.5M8.5 4v3.5H12M7 3.5H3.5a1 1 0 0 0-1 1v8a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V9",
    popout: "M9 2.5h4.5V7M13.5 2.5L8 8M11.5 9.5v3a1 1 0 0 1-1 1h-7a1 1 0 0 1-1-1v-7a1 1 0 0 1 1-1h3",
    gear: "M8 10a2 2 0 1 0 0-4 2 2 0 0 0 0 4zM8 1.8v1.7M8 12.5v1.7M1.8 8h1.7M12.5 8h1.7M3.6 3.6l1.2 1.2M11.2 11.2l1.2 1.2M3.6 12.4l1.2-1.2M11.2 4.8l1.2-1.2",
    warning: "M8 2.5l6 10.5H2zM8 6.5v3M8 11.2v.3",
    more: "M3.8 8h.4M7.8 8h.4M11.8 8h.4",
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
