<script lang="ts">
  let { lines }: { lines: string[] } = $props();

  let scroller: HTMLDivElement | undefined = $state();
  let pinned = true;

  // Follow new output unless the person has scrolled up to read.
  $effect.pre(() => {
    lines.length;
    if (!scroller) return;
    pinned = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 24;
  });

  $effect(() => {
    lines.length;
    if (pinned && scroller) scroller.scrollTop = scroller.scrollHeight;
  });
</script>

<section class="log" aria-label="Game Log">
  <!-- Focusable so the log can be scrolled from the keyboard. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="scroller" bind:this={scroller} tabindex="0" role="log">
    {#if lines.length}
      <pre>{lines.join("\n")}</pre>
    {:else}
      <p class="secondary">Output from the game appears here while it runs.</p>
    {/if}
  </div>
</section>

<style>
  .log {
    flex: 0 0 38%;
    min-height: 120px;
    border-top: 1px solid var(--separator);
    background: var(--bg-log);
  }

  .scroller {
    height: 100%;
    overflow: auto;
    padding: 10px 16px;
  }

  pre {
    margin: 0;
    font: 11px/1.5 var(--mono);
    white-space: pre-wrap;
    word-break: break-word;
    -webkit-user-select: text;
    user-select: text;
  }

  p {
    margin: 0;
  }
</style>
