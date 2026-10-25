<!--
  Skins & Capes: preview, a library of skin files, and the capes the account owns.
  Choices are previewed first and sent to Mojang only on Apply, because skin uploads
  are rate-limited and a click shouldn't spend one.
-->
<script lang="ts">
  import { Menu } from "@tauri-apps/api/menu";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, errorText, type LibrarySkin, type SkinVariant, type Wardrobe } from "$lib/api";
  import { launcher } from "$lib/launcher.svelte";
  import CapeThumb from "./CapeThumb.svelte";
  import Icon from "./Icon.svelte";
  import SkinPreview from "./SkinPreview.svelte";

  type Selection = { kind: "worn" } | { kind: "library"; id: string };

  let dialog: HTMLDialogElement | undefined = $state();
  let wardrobe = $state<Wardrobe | null>(null);
  let library = $state<LibrarySkin[]>([]);
  let textures = $state<Record<string, string>>({});
  let loading = $state(false);
  let applying = $state(false);
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);

  let selected = $state<Selection>({ kind: "worn" });
  let variant = $state<SkinVariant>("classic");
  let selectedCape = $state<string | null>(null);
  let view = $state<"front" | "back">("front");

  const online = $derived(wardrobe !== null);
  const worn = $derived(wardrobe?.skins.find((s) => s.state === "ACTIVE") ?? null);
  const wornVariant = $derived<SkinVariant>(worn?.variant === "SLIM" ? "slim" : "classic");
  const wornCape = $derived(wardrobe?.capes.find((c) => c.state === "ACTIVE")?.id ?? null);
  const selectedLibrarySkin = $derived.by(() => {
    const current = selected;
    return current.kind === "library" ? library.find((s) => s.id === current.id) : undefined;
  });

  const previewTexture = $derived.by(() => {
    const current = selected;
    return current.kind === "worn" ? (worn ? textures[worn.url] : null) : textures[current.id];
  });
  const previewCape = $derived(selectedCape ? textures[`cape:${selectedCape}`] : null);
  const skinChanged = $derived(selected.kind === "library" || variant !== wornVariant);
  const capeChanged = $derived(online && selectedCape !== wornCape);

  $effect(() => {
    if (launcher.skinsOpen) {
      dialog?.showModal();
      load();
    } else {
      dialog?.close();
    }
  });

  async function load() {
    loading = true;
    error = notice = null;
    try {
      const [remote, skins] = await Promise.all([api.getWardrobe(), api.listSkinLibrary()]);
      wardrobe = remote;
      library = skins;
      resetSelection();
      await Promise.all([
        ...skins.map(async (s) => (textures[s.id] ??= await api.librarySkinTexture(s.id))),
        ...(remote?.skins ?? []).map(async (s) => (textures[s.url] ??= await api.skinTexture(s.url))),
        ...(remote?.capes ?? []).map(async (c) => (textures[`cape:${c.id}`] ??= await api.skinTexture(c.url))),
      ]);
    } catch (e) {
      error = errorText(e);
    } finally {
      loading = false;
    }
  }

  function resetSelection() {
    const first = library[0];
    selected = online || !first ? { kind: "worn" } : { kind: "library", id: first.id };
    variant = selected.kind === "library" ? first.variant : wornVariant;
    selectedCape = wornCape;
  }

  function select(next: Selection) {
    selected = next;
    variant = next.kind === "library" ? (library.find((s) => s.id === next.id)?.variant ?? "classic") : wornVariant;
    view = "front";
  }

  function selectCape(id: string | null) {
    selectedCape = id;
    view = "back";
  }

  async function changeVariant(next: SkinVariant) {
    variant = next;
    // A library skin's arm model is a local setting, saved straight away.
    if (selectedLibrarySkin && selectedLibrarySkin.variant !== next) {
      const updated = await api.updateLibrarySkin(selectedLibrarySkin.id, { variant: next });
      library = library.map((s) => (s.id === updated.id ? updated : s));
    }
  }

  async function addSkin() {
    const path = await open({ title: "Choose a Skin", filters: [{ name: "Skin", extensions: ["png"] }] });
    if (typeof path !== "string") return;
    error = null;
    try {
      const skin = await api.addSkinToLibrary(path, "classic");
      if (!library.some((s) => s.id === skin.id)) library = [...library, skin];
      textures[skin.id] = await api.librarySkinTexture(skin.id);
      select({ kind: "library", id: skin.id });
    } catch (e) {
      error = errorText(e);
    }
  }

  async function showSkinMenu(e: MouseEvent, selection: Selection) {
    e.preventDefault();
    select(selection);
    const items =
      selection.kind === "library"
        ? [{ id: "remove", text: "Remove from Library", action: () => removeSkin(selection.id) }]
        : [{ id: "save", text: "Save to Library", enabled: !!worn, action: saveWorn }];
    await (await Menu.new({ items })).popup();
  }

  async function removeSkin(id: string) {
    await api.removeLibrarySkin(id);
    library = library.filter((s) => s.id !== id);
    if (selected.kind === "library" && selected.id === id) select({ kind: "worn" });
  }

  async function saveWorn() {
    if (!worn) return;
    try {
      const name = `${launcher.account?.name ?? "Saved"} skin`;
      const skin = await api.saveWornSkin(worn.url, wornVariant, name);
      if (!library.some((s) => s.id === skin.id)) library = [...library, skin];
      textures[skin.id] ??= await api.librarySkinTexture(skin.id);
      notice = "Saved the current skin to your library.";
    } catch (e) {
      error = errorText(e);
    }
  }

  async function apply() {
    applying = true;
    error = notice = null;
    try {
      let result = wardrobe;
      if (skinChanged) {
        result = selected.kind === "library" ? await api.wearLibrarySkin(selected.id) : await api.setSkinVariant(variant);
      }
      if (capeChanged) result = await api.wearCape(selectedCape);
      wardrobe = result;
      await Promise.all((result?.skins ?? []).map(async (s) => (textures[s.url] ??= await api.skinTexture(s.url))));
      select({ kind: "worn" });
      selectedCape = wornCape;
      notice = "Updated. Servers show the change the next time you join.";
    } catch (e) {
      error = errorText(e);
    } finally {
      applying = false;
    }
  }

  function onTileKeydown(e: KeyboardEvent, id: string) {
    if (e.key === "Backspace" || e.key === "Delete") removeSkin(id);
  }
</script>

<dialog
  class="sheet wardrobe"
  bind:this={dialog}
  aria-labelledby="wardrobe-title"
  onclose={() => (launcher.skinsOpen = false)}
>
  <header>
    <h2 id="wardrobe-title">Skins & Capes</h2>
    <p class="secondary">{launcher.account?.name ?? ""}</p>
  </header>

  {#if !loading && !online}
    <p class="note secondary">
      Skins and capes need a Microsoft account. You can still collect skins here and wear them once you've signed in.
    </p>
  {/if}

  <div class="layout">
    <section class="preview" aria-label="Preview">
      <div class="figure">
        <SkinPreview skin={previewTexture} cape={previewCape} {variant} {view} scale={9} label="Skin preview, {view} view" />
      </div>
      <div class="segmented" role="radiogroup" aria-label="View">
        <button role="radio" aria-checked={view === "front"} onclick={() => (view = "front")}>Front</button>
        <button role="radio" aria-checked={view === "back"} onclick={() => (view = "back")}>Back</button>
      </div>
      <div class="segmented" role="radiogroup" aria-label="Arm model">
        <button role="radio" aria-checked={variant === "classic"} onclick={() => changeVariant("classic")}>Classic</button>
        <button role="radio" aria-checked={variant === "slim"} onclick={() => changeVariant("slim")}>Slim</button>
      </div>
    </section>

    <div class="choices">
      <h3 id="skins-heading">Skins</h3>
      <div class="grid" role="radiogroup" aria-labelledby="skins-heading">
        {#if online}
          <button
            class="tile"
            role="radio"
            aria-checked={selected.kind === "worn"}
            onclick={() => select({ kind: "worn" })}
            oncontextmenu={(e) => showSkinMenu(e, { kind: "worn" })}
          >
            <SkinPreview skin={worn ? textures[worn.url] : null} variant={wornVariant} scale={3} />
            <span>Current</span>
          </button>
        {/if}
        {#each library as skin (skin.id)}
          <button
            class="tile"
            role="radio"
            aria-checked={selected.kind === "library" && selected.id === skin.id}
            onclick={() => select({ kind: "library", id: skin.id })}
            oncontextmenu={(e) => showSkinMenu(e, { kind: "library", id: skin.id })}
            onkeydown={(e) => onTileKeydown(e, skin.id)}
          >
            <SkinPreview skin={textures[skin.id]} variant={skin.variant} scale={3} />
            <span>{skin.name}</span>
          </button>
        {/each}
        <button class="tile add" onclick={addSkin}>
          <Icon name="plus" size={20} />
          <span>Add Skin…</span>
        </button>
      </div>

      {#if online}
        <h3 id="capes-heading">Capes</h3>
        <div class="grid" role="radiogroup" aria-labelledby="capes-heading">
          <button class="tile cape" role="radio" aria-checked={selectedCape === null} onclick={() => selectCape(null)}>
            <span class="none" aria-hidden="true"></span>
            <span>No Cape</span>
          </button>
          {#each wardrobe?.capes ?? [] as cape (cape.id)}
            <button class="tile cape" role="radio" aria-checked={selectedCape === cape.id} onclick={() => selectCape(cape.id)}>
              <CapeThumb texture={textures[`cape:${cape.id}`] ?? null} scale={4} />
              <span>{cape.alias ?? "Cape"}</span>
            </button>
          {/each}
        </div>
        {#if wardrobe?.capes.length === 0}
          <p class="secondary hint">This account doesn't own any capes.</p>
        {/if}
      {/if}
    </div>
  </div>

  <div class="footer">
    <p class="message" role="status" aria-live="polite">
      {#if error}<span class="error">{error}</span>{:else if notice}<span class="secondary">{notice}</span>{/if}
    </p>
    <button class="button" onclick={() => (launcher.skinsOpen = false)}>Done</button>
    {#if online}
      <button class="button default" disabled={applying || loading || (!skinChanged && !capeChanged)} onclick={apply}>
        {applying ? "Applying…" : "Apply"}
      </button>
    {/if}
  </div>
</dialog>

<style>
  dialog.wardrobe {
    width: min(720px, calc(100vw - 32px));
  }

  header {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }

  header p {
    margin: 0;
  }

  .note {
    margin: 8px 0 0;
  }

  .layout {
    display: flex;
    gap: 24px;
    margin-top: 16px;
  }

  .preview {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    flex: none;
  }

  .figure {
    display: grid;
    place-items: center;
    width: 184px;
    height: 312px;
    border-radius: var(--radius-panel);
    background: var(--bg-control);
  }

  .segmented {
    display: inline-flex;
    padding: 2px;
    border-radius: var(--radius-control);
    background: var(--bg-control);
  }

  .segmented button {
    min-width: 72px;
    padding: 3px 10px;
    border: none;
    border-radius: 5px;
    background: transparent;
    font-weight: 500;
  }

  .segmented button[aria-checked="true"] {
    background: var(--bg-segment-selected);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.15);
  }

  .choices {
    flex: 1;
    min-width: 0;
    max-height: 380px;
    overflow-y: auto;
  }

  h3 {
    margin: 0 0 8px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
  }

  .grid + h3 {
    margin-top: 18px;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(84px, 1fr));
    gap: 8px;
  }

  .tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
    min-height: 124px;
    padding: 10px 6px 8px;
    border: 2px solid transparent;
    border-radius: var(--radius-panel);
    background: var(--bg-control);
    color: var(--text);
  }

  .tile:hover {
    background: var(--bg-control-hover);
  }

  .tile[aria-checked="true"] {
    border-color: var(--accent);
  }

  .tile span:last-child {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
  }

  .tile.add {
    justify-content: center;
    color: var(--text-secondary);
  }

  .tile.cape {
    min-height: 100px;
  }

  .none {
    width: 40px;
    height: 64px;
    border: 1.5px dashed var(--separator);
    border-radius: 3px;
  }

  .hint {
    margin: 8px 0 0;
    font-size: 11px;
  }

  .footer {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 20px;
  }

  .message {
    flex: 1;
    margin: 0;
    min-width: 0;
  }

  .error {
    color: var(--danger);
  }
</style>
