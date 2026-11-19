<script>
  /**
   * A person's picture, in a circle: the one they put on their card
   * (`load`, its URL to come), or, without one, until it comes, or if it
   * never does, their handle's initial.
   */
  let { handle, load = null } = $props();
  const initial = $derived((handle[0] ?? '?').toUpperCase());
</script>

{#snippet letter()}<span class="avatar" aria-hidden="true">{initial}</span>{/snippet}
{#if load}{#await load}{@render letter()}{:then src}<img
      class="avatar"
      {src}
      alt=""
    />{:catch}{@render letter()}{/await}{:else}{@render letter()}{/if}
