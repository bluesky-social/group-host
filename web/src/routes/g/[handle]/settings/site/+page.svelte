<script lang="ts">
  import Section from '#lib/molecules/Section.svelte';
  import GroupSite from '#lib/organisms/GroupSite.svelte';
  let { data, form } = $props();
  const g = $derived(data.group);
</script>

<svelte:head><title>Site · {g.displayName ?? g.handle} · opensocial</title></svelte:head>

{#if form?.failed}<p class="danger">{form.failed}</p>{/if}
<Section title="Site">
  {#snippet actions()}<button class="quiet" type="button" popovertarget="publish-site"
      >Publish a site</button
    >{/snippet}
  {#snippet intro()}Pages you publish here are open to anyone at <span class="mono"
      >{data.address}</span
    >, signed in or not.{/snippet}
  <GroupSite address={data.address} site={data.site} />
</Section>
