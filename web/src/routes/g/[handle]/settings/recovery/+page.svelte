<script lang="ts">
  import Empty from '#lib/molecules/Empty.svelte';
  import Section from '#lib/molecules/Section.svelte';
  import RecoveryKeys from '#lib/organisms/RecoveryKeys.svelte';
  let { data, form } = $props();
  const g = $derived(data.group);
  const r = $derived(data.recovery);
  const stewards = $derived(r?.keys.filter((k) => k.holder === 'steward').length ?? 0);
</script>

<svelte:head><title>Recovery keys · {g.displayName ?? g.handle} · opensocial</title></svelte:head>

{#if form?.failed}<p class="danger">{form.failed}</p>{/if}
<Section title="Recovery keys">
  {#snippet actions()}
    {#if r?.hostHolds && stewards < r.max}<button
        class="quiet"
        type="button"
        popovertarget="add-key">Add a recovery key</button
      >{/if}
  {/snippet}
  {#snippet intro()}Keys that can change the group's identity, highest ranked first. A key you hold
    can undo what this host does, or move the group elsewhere without it.{/snippet}
  {#if !r}
    <Empty title="This host keeps no keys here">This host can't change this group's identity.</Empty
    >
  {:else}
    {#if !r.hostHolds}<p class="mute">
        This host no longer holds a key to the group's identity, so it can't change the keys.
      </p>{/if}
    <RecoveryKeys keys={r.keys} hostHolds={r.hostHolds} />
  {/if}
</Section>
