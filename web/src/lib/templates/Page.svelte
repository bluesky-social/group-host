<script lang="ts">
  import type { Snippet } from 'svelte';
  /**
   * A page of the console: where it is under (`back`, a group's page),
   * its heading, and the one act on the whole page (`actions`) across
   * from it; then `nav`, its parts' tabs; then what it holds.
   */
  let {
    title,
    back,
    actions,
    nav,
    children
  }: {
    title: string;
    back?: { href: string; text: string };
    actions?: Snippet;
    nav?: Snippet;
    children: Snippet;
  } = $props();
</script>

<div class="stack">
  <div class="page-head">
    <div>
      {#if back}<a class="back" href={back.href}>{back.text}</a>{/if}
      <h1>{title}</h1>
    </div>
    {@render actions?.()}
  </div>
  {@render nav?.()}
  {@render children()}
</div>
