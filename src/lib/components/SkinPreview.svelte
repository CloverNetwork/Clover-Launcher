<!--
  Draws a skin texture as a flat, pixel-exact figure, front or back, with both layers.
  Handles 64×64 skins and the 64×32 layout used before 1.8 (whose left limbs mirror the right).
  The cape, when given, hangs on the back view.
-->
<script lang="ts">
  import type { SkinVariant } from "$lib/api";

  let {
    skin,
    cape = null,
    variant,
    view = "front",
    scale = 8,
    label = "",
  }: {
    skin: string | null;
    cape?: string | null;
    variant: SkinVariant;
    view?: "front" | "back";
    scale?: number;
    /** Describes the figure for screen readers; leave empty where a visible name already does. */
    label?: string;
  } = $props();

  let canvas: HTMLCanvasElement | undefined = $state();

  // Source rectangle in the skin texture, and where it goes on the 16×32 figure.
  type Part = { src: [number, number, number, number]; at: [number, number]; mirror?: boolean };

  function parts(back: boolean, slim: boolean, legacy: boolean): Part[] {
    const w = slim ? 3 : 4;
    // Each limb box in the texture: [u, v]. A face's x is u + 4 (front) or u + 8 + w (back).
    const face = (u: number, v: number, width: number) => (back ? u + 8 + width : u + 4);
    const limb = (u: number, v: number, width: number, at: [number, number], mirror = false): Part => ({
      src: [face(u, v, width), v + 4, width, 12],
      at,
      mirror,
    });
    // Viewer's left is the player's right from the front, and their left from the back.
    const leftSlot: [number, number] = [4 - w, 8];
    const rightSlot: [number, number] = [12, 8];
    const [rightArmAt, leftArmAt] = back ? [rightSlot, leftSlot] : [leftSlot, rightSlot];
    const [rightLegAt, leftLegAt]: [number, number][] = back ? [[8, 20], [4, 20]] : [[4, 20], [8, 20]];

    const base: Part[] = [
      { src: [back ? 24 : 8, 8, 8, 8], at: [4, 0] },
      { src: [back ? 32 : 20, 20, 8, 12], at: [4, 8] },
      limb(40, 16, w, rightArmAt),
      limb(0, 16, 4, rightLegAt),
      legacy ? limb(40, 16, w, leftArmAt, true) : limb(32, 48, w, leftArmAt),
      legacy ? limb(0, 16, 4, leftLegAt, true) : limb(16, 48, 4, leftLegAt),
    ];
    const overlay: Part[] = [{ src: [back ? 56 : 40, 8, 8, 8], at: [4, 0] }];
    if (!legacy) {
      overlay.push(
        { src: [back ? 32 : 20, 36, 8, 12], at: [4, 8] },
        limb(40, 32, w, rightArmAt),
        limb(48, 48, w, leftArmAt),
        limb(0, 32, 4, rightLegAt),
        limb(0, 48, 4, leftLegAt),
      );
    }
    return [...base, ...overlay];
  }

  function load(src: string) {
    return new Promise<HTMLImageElement>((resolve, reject) => {
      const image = new Image();
      image.onload = () => resolve(image);
      image.onerror = reject;
      image.src = src;
    });
  }

  $effect(() => {
    const [target, skinSrc, capeSrc, back, slim, s] = [canvas, skin, cape, view === "back", variant === "slim", scale];
    if (!target) return;
    let current = true;
    const ctx = target.getContext("2d")!;
    ctx.imageSmoothingEnabled = false;
    ctx.clearRect(0, 0, target.width, target.height);
    if (!skinSrc) return;

    Promise.all([load(skinSrc), capeSrc && back ? load(capeSrc) : null]).then(([image, capeImage]) => {
      if (!current) return;
      const legacy = image.height === image.width / 2;
      const unit = image.width / 64; // HD skins scale every coordinate
      for (const { src, at, mirror } of parts(back, slim, legacy)) {
        const [sx, sy, w, h] = src;
        ctx.save();
        ctx.translate(at[0] * s + (mirror ? w * s : 0), at[1] * s);
        ctx.scale(mirror ? -1 : 1, 1);
        ctx.drawImage(image, sx * unit, sy * unit, w * unit, h * unit, 0, 0, w * s, h * s);
        ctx.restore();
      }
      if (capeImage) {
        // The cape's outer face is 10×16 at (1, 1); it hangs from the shoulders.
        const capeUnit = capeImage.width / 64;
        ctx.drawImage(capeImage, capeUnit, capeUnit, 10 * capeUnit, 16 * capeUnit, 3 * s, 8 * s, 10 * s, 16 * s);
      }
    });
    return () => (current = false);
  });
</script>

<div role={label ? "img" : undefined} aria-label={label || undefined} aria-hidden={label ? undefined : "true"}>
  <canvas bind:this={canvas} width={16 * scale} height={32 * scale}></canvas>
</div>

<style>
  canvas {
    display: block;
    image-rendering: pixelated;
  }
</style>
