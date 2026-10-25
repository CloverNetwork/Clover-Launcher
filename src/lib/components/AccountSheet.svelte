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
  let offlineName = $state("");

  /** Offline accounts are for testing before Microsoft approves the app; release builds don't offer them. */
  const offlineAllowed = import.meta.env.DEV;
  const validOfflineName = $derived(/^[A-Za-z0-9_]{3,16}$/.test(offlineName.trim()));

  async function signInOffline(e: SubmitEvent) {
    e.preventDefault();
    error = null;
    try {
      launcher.account = await api.signInOffline(offlineName);
      launcher.accountOpen = false;
    } catch (err) {
      error = errorText(err);
    }
  }

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
        <p class="secondary">{launcher.account.offline ? "Offline account (development only)" : "Microsoft account"}</p>
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

    {#if offlineAllowed}
      <form class="offline" onsubmit={signInOffline}>
        <h3>Offline Account <span class="badge">Development</span></h3>
        <p class="secondary">
          For testing until Microsoft sign-in is set up. Online servers and Realms won't accept it, and release builds
          don't include it.
        </p>
        <div class="offline-row">
          <label class="visually-hidden" for="offline-name">Player name</label>
          <input
            id="offline-name"
            class="field"
            bind:value={offlineName}
            placeholder="Player name"
            maxlength="16"
            autocomplete="off"
            spellcheck="false"
          />
          <button type="submit" class="button" disabled={!validOfflineName}>Use Offline</button>
        </div>
      </form>
    {/if}
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

  .offline {
    margin-top: 20px;
    padding-top: 16px;
    border-top: 1px solid var(--separator);
  }

  h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 4px;
    font-size: 13px;
    font-weight: 600;
  }

  .badge {
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--bg-control);
    color: var(--text-secondary);
    font-size: 11px;
    font-weight: 500;
  }

  .offline-row {
    display: flex;
    gap: 8px;
    margin-top: 10px;
  }

  .offline-row .button {
    flex: none;
    white-space: nowrap;
  }
</style>
