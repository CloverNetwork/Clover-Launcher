<!-- A cape's outer face (10×16 at 1,1 in the cape texture), drawn pixel-exact. -->
<script lang="ts">
  let { texture, scale = 3 }: { texture: string | null; scale?: number } = $props();

  let canvas: HTMLCanvasElement | undefined = $state();

  $effect(() => {
    const [target, src, s] = [canvas, texture, scale];
    if (!target || !src) return;
    let current = true;
    const image = new Image();
    image.onload = () => {
      if (!current) return;
      const ctx = target.getContext("2d")!;
      ctx.imageSmoothingEnabled = false;
      const unit = image.width / 64;
      ctx.clearRect(0, 0, target.width, target.height);
      ctx.drawImage(image, unit, unit, 10 * unit, 16 * unit, 0, 0, 10 * s, 16 * s);
    };
    image.src = src;
    return () => (current = false);
  });
</script>

<canvas bind:this={canvas} width={10 * scale} height={16 * scale} aria-hidden="true"></canvas>

<style>
  canvas {
    display: block;
    image-rendering: pixelated;
  }
</style>
