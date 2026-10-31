<script lang="ts">
  import { untrack } from "svelte";
  import { api, errorText, type DeviceCode } from "$lib/api";
  import { launcher } from "$lib/launcher.svelte";
  import { t } from "$lib/i18n";

  let dialog: HTMLDialogElement | undefined = $state();
  let code = $state<DeviceCode | null>(null);
  let error = $state<string | null>(null);
  let starting = $state(false);
  let copied = $state(false);
  let offlineName = $state("");

  /** Offline accounts are for testing before Microsoft approves the app; release builds don't offer them. */
  // The Rust side decides, since only development builds of it accept offline accounts.
  let offlineAllowed = $state(false);
  api.developmentBuild().then((dev) => (offlineAllowed = dev), () => {});
  const validOfflineName = $derived(/^[A-Za-z0-9_]{3,16}$/.test(offlineName.trim()));

  async function signInOffline(e: SubmitEvent) {
    e.preventDefault();
    error = null;
    try {
      launcher.setAccounts(await api.signInOffline(offlineName));
      offlineName = "";
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
      // Cancelling closes Microsoft's page too.
      if (untrack(() => code)) api.closeSignInWindow().catch(() => {});
    }
  });

  async function signIn() {
    starting = true;
    error = null;
    try {
      code = await api.beginSignIn();
      await api.openSignInWindow(code);
      launcher.setAccounts(await api.finishSignIn(code));
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
  {#if code}
    <h2 id="account-title">{t("Finish Signing In")}</h2>
    <p class="secondary">{t("Sign in to Microsoft in the window that opened. If it asks for a code, enter this one:")}</p>
    <div class="code">
      <output aria-label={t("Sign-in code")}>{code.userCode}</output>
      <button class="button" onclick={copyCode}>{copied ? t("Copied") : t("Copy Code")}</button>
    </div>
    <p class="secondary waiting" role="status">{t("Waiting for you to finish on microsoft.com…")}</p>
    <div class="actions">
      <button class="button" onclick={() => api.openSignInWindow(code!)}>{t("Show Sign-In Window")}</button>
      <button class="button" onclick={() => (launcher.accountOpen = false)}>{t("Cancel")}</button>
    </div>
  {:else}
    <h2 id="account-title">{launcher.accounts.length ? t("Add Account") : t("Sign In to Play")}</h2>
    <p class="secondary">{t("Use the Microsoft account that owns Minecraft: Java Edition. Your password goes only to Microsoft.")}</p>
    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
    <div class="actions">
      <button class="button" onclick={() => (launcher.accountOpen = false)}>{t("Cancel")}</button>
      <button class="button default" disabled={starting} onclick={signIn}>{t("Sign In with Microsoft")}</button>
    </div>

    {#if offlineAllowed}
      <form class="offline" onsubmit={signInOffline}>
        <h3>{t("Offline Account")} <span class="badge">{t("Development")}</span></h3>
        <p class="secondary">
          {t("For testing until Microsoft sign-in is set up. Online servers and Realms won't accept it, and release builds don't include it.")}
        </p>
        <div class="offline-row">
          <label class="visually-hidden" for="offline-name">{t("Player name")}</label>
          <input
            id="offline-name"
            class="field"
            bind:value={offlineName}
            placeholder={t("Player name")}
            maxlength="16"
            autocomplete="off"
            spellcheck="false"
          />
          <button type="submit" class="button" disabled={!validOfflineName}>{t("Use Offline")}</button>
        </div>
      </form>
    {/if}
  {/if}
</dialog>

<style>
  p {
    margin: 0;
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
