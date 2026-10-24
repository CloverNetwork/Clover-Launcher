<script lang="ts">
  import "../app.css";

  let { children } = $props();

  if (typeof navigator !== "undefined" && navigator.userAgent.includes("Mac")) {
    document.documentElement.dataset.platform = "macos";
  }
</script>

{@render children()}
