<!-- A world's data packs, in a sheet over the instance page. -->
<script lang="ts">
  import { launcher } from "$lib/launcher.svelte";
  import ContentSection from "./ContentSection.svelte";

  let dialog: HTMLDialogElement | undefined = $state();
  const instance = $derived(launcher.selected);
  const world = $derived(launcher.dataPacksWorld);

  $effect(() => {
    if (world && instance) dialog?.showModal();
    else dialog?.close();
  });
</script>

<dialog class="sheet datapacks" bind:this={dialog} aria-label="Data Packs for {world?.name ?? ''}" onclose={() => (launcher.dataPacksWorld = null)}>
  {#if world && instance}
    <div class="scroll">
      <ContentSection {instance} place={{ kind: "datapack", world: world.folder }} subtitle={world.name} />
    </div>
  {/if}
  <div class="actions">
    <button class="button default" onclick={() => (launcher.dataPacksWorld = null)}>Done</button>
  </div>
</dialog>

<style>
  dialog.datapacks {
    width: min(600px, calc(100vw - 32px));
  }

  .scroll {
    max-height: min(480px, 60vh);
    overflow-y: auto;
  }

  .scroll :global(.mods) {
    margin-top: 0;
  }
</style>
