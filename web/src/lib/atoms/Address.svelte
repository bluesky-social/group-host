<script lang="ts">
  /**
   * A web address as text, breaking where it must after a `/` or a `.`,
   * never inside a word between them.
   */
  let { address }: { address: string } = $props();
  const parts = $derived(address.match(/[^/.]*(?:[/.]+|$)/g)?.filter(Boolean) ?? [address]);
</script>

{#each parts as part, i (i)}{part}<wbr />{/each}
