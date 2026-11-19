<script>
  /**
   * What a list or room says where its rows would be, when it has none:
   * `title`, what is empty, in the reader's terms; `children`, why, or what
   * fills it; `action`, the way out, where there is one.
   */
  let { title, children, action } = $props();
</script>

<div class="empty">
  <p class="empty-title">{title}</p>
  {#if children}<p>{@render children()}</p>{/if}
  {#if action}<div class="acts">{@render action()}</div>{/if}
</div>
