<script lang="ts">
  import { ParaglideMessage } from '@inlang/paraglide-js-svelte';
  import { m } from '#lib/paraglide/messages.js';
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
  <p>
    <ParaglideMessage message={m.back_to_app_taking}>
      {#snippet link({ children })}<a href={to}>{@render children?.()}</a>{/snippet}
    </ParaglideMessage>
  </p>
{:else}
  <p class="danger">{m.back_to_app_unsafe()}</p>
{/if}
