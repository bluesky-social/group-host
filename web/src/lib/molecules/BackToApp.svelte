<script lang="ts">
  /**
   * The way back to an app, once its request is answered: taken at once,
   * or by hand without the page's script. Never to a script or data URL,
   * whatever the host said: those would run here, as the console.
   */
  let { to }: { to: string } = $props();
  const safe = $derived(!/^\s*(javascript|data|vbscript|file):/i.test(to));
  $effect(() => {
    if (safe) window.location.assign(to);
  });
</script>

{#if safe}
  <p>Taking you back to the app… <a href={to}>Go back to the app</a></p>
{:else}
  <p class="danger">The app asked to be sent back somewhere it may not be.</p>
{/if}
