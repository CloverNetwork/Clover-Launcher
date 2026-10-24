<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api, errorText, type DeviceCode } from "$lib/api";
  import { launcher } from "$lib/launcher.svelte";
  import BlockIcon from "./BlockIcon.svelte";

  let dialog: HTMLDialogElement | undefined = $state();
  let code = $state<DeviceCode | null>(null);
  let error = $state<string | null>(null);
  let starting = $state(false);
  let copied = $state(false);

  $effect(() => {
    if (launcher.accountOpen) {
      code = null;
      error = null;
      dialog?.showModal();
    } else {
      dialog?.close();
    }
  });

  async function signIn() {
    starting = true;
    error = null;
    try {
      code = await api.beginSignIn();
      await openUrl(code.verificationUri);
      const profile = await api.finishSignIn(code);
      launcher.account = profile;
      launcher.accountOpen = false;
    } catch (e) {
      code = null;
      error = errorText(e);
    } finally {
      starting = false;
    }
  }

  async function copyCode() {
    if (!code) return;
    await navigator.clipboard.writeText(code.userCode);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }
</script>

<dialog
  class="sheet"
  bind:this={dialog}
  aria-labelledby="account-title"
  onclose={() => (launcher.accountOpen = false)}
>
  {#if launcher.account}
    <h2 id="account-title">Account</h2>
    <div class="profile">
      <BlockIcon seed={launcher.account.uuid} size={40} />
      <div>
        <strong>{launcher.account.name}</strong>
        <p class="secondary">Microsoft account</p>
      </div>
    </div>
    <div class="actions">
      <button class="button destructive" onclick={() => launcher.signOut()}>Sign Out</button>
      <button class="button default" onclick={() => (launcher.accountOpen = false)}>Done</button>
    </div>
  {:else if code}
    <h2 id="account-title">Finish Signing In</h2>
    <p class="secondary">In the browser window that opened, enter this code:</p>
    <div class="code">
      <output aria-label="Sign-in code">{code.userCode}</output>
      <button class="button" onclick={copyCode}>{copied ? "Copied" : "Copy Code"}</button>
    </div>
    <p class="secondary waiting" role="status">Waiting for you to finish on microsoft.com…</p>
    <div class="actions">
      <button class="button" onclick={() => openUrl(code!.verificationUri)}>Open Browser Again</button>
      <button class="button" onclick={() => (launcher.accountOpen = false)}>Cancel</button>
    </div>
  {:else}
    <h2 id="account-title">Sign In to Play</h2>
    <p class="secondary">Use the Microsoft account that owns Minecraft: Java Edition. Your password goes only to Microsoft.</p>
    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
    <div class="actions">
      <button class="button" onclick={() => (launcher.accountOpen = false)}>Cancel</button>
      <button class="button default" disabled={starting} onclick={signIn}>Sign In with Microsoft</button>
    </div>
  {/if}
</dialog>

<style>
  p {
    margin: 0;
  }

  .profile {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 12px;
  }

  .code {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: 14px 0;
    padding: 10px 14px;
    border-radius: var(--radius-panel);
    background: var(--bg-control);
  }

  output {
    font: 600 22px/1 var(--mono);
    letter-spacing: 0.12em;
    -webkit-user-select: all;
    user-select: all;
  }

  .error {
    margin-top: 12px;
    color: var(--danger);
  }
</style>
