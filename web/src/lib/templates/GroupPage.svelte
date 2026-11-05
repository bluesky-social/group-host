<script lang="ts">
  import type { Snippet } from 'svelte';
  /**
   * A group's own page, for someone signed in: what the page has to say
   * first (`notice`), the group's card, then its parts, one under another.
   */
  let { notice, card, children }: { notice?: Snippet; card: Snippet; children?: Snippet } =
    $props();
</script>

<div class="stack group-page">
  {@render notice?.()}
  {@render card()}
  {@render children?.()}
</div>
