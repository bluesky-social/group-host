<script>
  /**
   * A dialog over the page: the page dimmed under
   * it, closed by its ×, Escape, or `open` set false. `label` names it to
   * a screen reader, in a few fixed words.
   */
  let { open = $bindable(false), label, children } = $props();
  let el;
  $effect(() => {
    if (open && !el.open) el.showModal();
    else if (!open && el.open) el.close();
  });
</script>

<dialog bind:this={el} aria-label={label} onclose={() => (open = false)}>
  <button type="button" class="dialog-close" aria-label="Close" onclick={() => (open = false)}
    >×</button
  >
  <div class="dialog-body">{@render children()}</div>
</dialog>
