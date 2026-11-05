<script lang="ts">
  /** The foot of every page: the host it is, by its address. */
  let { host }: { host: string } = $props();
</script>

<footer class="wrap foot">{host} runs opensocial</footer>
