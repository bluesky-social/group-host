<script lang="ts">
  import Section from '#lib/molecules/Section.svelte';
  import GroupSite from '#lib/organisms/GroupSite.svelte';
  import SiteDomain from '#lib/organisms/SiteDomain.svelte';
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
      >{data.served}</span
    >, signed in or not.{/snippet}
  <GroupSite address={data.served} site={data.site} />
</Section>
<Section title="Domain">
  {#snippet intro()}Your site can be at the group's own domain instead of <span class="mono"
      >{data.address}</span
    >.{/snippet}
  <SiteDomain
    address={data.address}
    domain={data.domain}
    on={data.on}
    host={data.host}
    profile="/g/{g.handle}/settings"
  />
</Section>
