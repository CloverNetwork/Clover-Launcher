<!--
  The launcher's signature: every instance gets its own 8×8 block face,
  generated from its ID so it never changes. Decorative; names carry meaning.
-->
<script lang="ts">
  let { seed, size = 20 }: { seed: string; size?: number } = $props();

  type Material = { top?: string[]; body: string[] };

  const materials: Material[] = [
    { top: ["#4f8a2b", "#5f9e32", "#6fb33a"], body: ["#6b4a2f", "#7d5838", "#8f6a46"] }, // grass
    { body: ["#6f6f6f", "#7f7f7f", "#8f8f8f", "#9a9a9a"] }, // stone
    { body: ["#cfc08a", "#d8cb98", "#e0d4a6", "#e8dcb0"] }, // sand
    { body: ["#36363b", "#3f3f45", "#4a4a50", "#55555b"] }, // deepslate
    { body: ["#b8673f", "#c77447", "#d98a5a", "#5aa38a"] }, // copper
    { body: ["#7a52b0", "#8a63c4", "#9c78d4", "#c4a6ec"] }, // amethyst
    { body: ["#d48aa9", "#e6a5c0", "#f0b8cf", "#f7cde0"] }, // cherry
    { top: ["#e8eef2", "#f4f8fa", "#ffffff"], body: ["#6b4a2f", "#7d5838", "#8f6a46"] }, // snowy
  ];

  // FNV-1a, then a small LCG for per-pixel noise.
  function hash(text: string) {
    let h = 0x811c9dc5;
    for (const c of text) h = Math.imul(h ^ c.charCodeAt(0), 0x01000193);
    return h >>> 0;
  }

  const pixels = $derived.by(() => {
    let state = hash(seed);
    const next = () => (state = (Math.imul(state, 1664525) + 1013904223) >>> 0) / 2 ** 32;
    const material = materials[state % materials.length];
    const out: string[] = [];
    for (let y = 0; y < 8; y++) {
      for (let x = 0; x < 8; x++) {
        // Grass and snow overhang the top two rows and drip unevenly into the third.
        const capped = material.top && (y < 2 || (y === 2 && next() < 0.45));
        const tones = capped ? material.top! : material.body;
        out.push(tones[Math.floor(next() * tones.length)]);
      }
    }
    return out;
  });
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 8 8"
  shape-rendering="crispEdges"
  aria-hidden="true"
  style:border-radius="{Math.max(2, size / 10)}px"
>
  {#each pixels as fill, i (i)}
    <rect x={i % 8} y={Math.floor(i / 8)} width="1" height="1" {fill} />
  {/each}
</svg>

<style>
  svg {
    flex: none;
    display: block;
    /* A hairline keeps light blocks (snow, sand) defined on light backgrounds. */
    box-shadow: inset 0 0 0 0.5px rgba(0, 0, 0, 0.2);
    overflow: hidden;
  }
</style>
