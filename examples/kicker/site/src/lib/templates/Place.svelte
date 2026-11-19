<script>
  import SiteHeader from '../organisms/SiteHeader.svelte';
  /**
   * A place's layout: its header, drawn once, and under it the room open
   * (`children`), which changes in place as the visitor moves.
   */
  let { name, children } = $props();
</script>

<SiteHeader {name} />
<main class="page">{@render children()}</main>
