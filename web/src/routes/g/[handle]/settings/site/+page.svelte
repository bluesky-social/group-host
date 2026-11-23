<script lang="ts">
  import { ParaglideMessage } from '@inlang/paraglide-js-svelte';
  import { m } from '#lib/paraglide/messages.js';
  import Section from '#lib/molecules/Section.svelte';
  import GroupSite from '#lib/organisms/GroupSite.svelte';
  import SiteDomain from '#lib/organisms/SiteDomain.svelte';
  let { data, form } = $props();
  const g = $derived(data.group);
</script>

<svelte:head
  ><title>{m.route_group_settings_site_head({ group: g.displayName ?? g.handle })}</title
  ></svelte:head
>

{#if form?.failed}<p class="danger">{form.failed}</p>{/if}
<Section title={m.route_group_settings_site_title()}>
  {#snippet actions()}<button class="quiet" type="button" popovertarget="publish-site"
      >{m.route_group_settings_site_publish()}</button
    >{/snippet}
  {#snippet intro()}<ParaglideMessage message={m.route_group_settings_site_intro}>
      {#snippet address()}<span class="mono">{data.served}</span>{/snippet}
    </ParaglideMessage>{/snippet}
  <GroupSite address={data.served} site={data.site} />
</Section>
<Section title={m.route_group_settings_site_domain()}>
  {#snippet intro()}<ParaglideMessage message={m.route_group_settings_site_domain_intro}>
      {#snippet address()}<span class="mono">{data.address}</span>{/snippet}
    </ParaglideMessage>{/snippet}
  <SiteDomain
    address={data.address}
    domain={data.domain}
    on={data.on}
    host={data.host}
    profile="/g/{g.handle}/settings"
  />
</Section>
