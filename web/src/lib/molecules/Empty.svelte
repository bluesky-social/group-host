<script lang="ts">
  import type { Snippet } from 'svelte';
  /**
   * What a list or part says where its rows would be, when it has none,
   * the same on every page: `title`, what is empty, in the reader's
   * terms; `children`, a line on why, or what fills it; `action`, the way
   * out, where there is one.
   */
  let { title, children, action }: { title: string; children?: Snippet; action?: Snippet } =
    $props();
</script>

<div class="empty">
  <p class="empty-title">{title}</p>
  {#if children}<p>{@render children()}</p>{/if}
  {#if action}<div class="empty-action">{@render action()}</div>{/if}
</div>
