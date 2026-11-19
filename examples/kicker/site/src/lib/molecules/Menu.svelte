<script>
  /**
   * What may be done to a thing, behind a button of three dots: a menu
   * that opens under it, of `items`, a snippet given `done`, which each
   * item calls as it is chosen, closing the menu. Escape or a click
   * elsewhere closes it too. `label` names the button to a screen reader.
   */
  let { label, items } = $props();
  let open = $state(false);
  let el;
  const done = () => (open = false);
  const away = (e) => {
    if (open && !el.contains(e.target)) open = false;
  };
  const escape = (e) => {
    if (open && e.key === 'Escape') open = false;
  };
</script>

<svelte:window onclick={away} onkeydown={escape} />
<div class="menu" bind:this={el}>
  <button
    type="button"
    class="kebab"
    aria-label={label}
    aria-haspopup="menu"
    aria-expanded={open}
    onclick={() => (open = !open)}
  >
    <svg viewBox="0 0 4 16" aria-hidden="true">
      <circle cx="2" cy="2" r="1.6" /><circle cx="2" cy="8" r="1.6" /><circle
        cx="2"
        cy="14"
        r="1.6"
      />
    </svg>
  </button>
  {#if open}<div class="menu-list" role="menu">{@render items(done)}</div>{/if}
</div>
