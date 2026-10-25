<script lang="ts">
  import Icon from "./Icon.svelte";

  let {
    lines,
    onPopOut,
    standalone = false,
  }: {
    lines: string[];
    /** Shows the button that moves the log into its own window. */
    onPopOut?: () => void;
    /** Fills its own window instead of sitting under the instance page. */
    standalone?: boolean;
  } = $props();

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

<section class="log" class:standalone aria-label="Game Log">
  {#if onPopOut}
    <header>
      <h2>Game Log</h2>
      <button class="icon-button" aria-label="Open in Separate Window" title="Open in Separate Window" onclick={onPopOut}>
        <Icon name="popout" size={14} />
      </button>
    </header>
  {/if}
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
    display: flex;
    flex-direction: column;
    flex: 0 0 38%;
    min-height: 120px;
    border-top: 1px solid var(--separator);
    background: var(--bg-log);
  }

  .log.standalone {
    flex: 1;
    height: 100vh;
    border-top: none;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex: none;
    height: 30px;
    padding: 0 6px 0 16px;
    border-bottom: 1px solid var(--separator);
  }

  h2 {
    margin: 0;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
  }

  header .icon-button {
    width: 24px;
    height: 24px;
  }

  .scroller {
    flex: 1;
    min-height: 0;
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
