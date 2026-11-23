<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  /** The foot of every page: the host it is, by its address. */
  let { host }: { host: string } = $props();
</script>

<footer class="wrap foot">{m.site_footer_runs({ host })}</footer>
