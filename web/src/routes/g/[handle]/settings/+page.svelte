<script lang="ts">
  import { ParaglideMessage } from '@inlang/paraglide-js-svelte';
  import { m } from '#lib/paraglide/messages.js';
  import Section from '#lib/molecules/Section.svelte';
  import GroupHandle from '#lib/organisms/GroupHandle.svelte';
  import LinksEditor from '#lib/organisms/LinksEditor.svelte';
  import ProfileForm from '#lib/organisms/ProfileForm.svelte';
  import ProfilePictures from '#lib/organisms/ProfilePictures.svelte';
  import SwitchOff from '#lib/organisms/SwitchOff.svelte';
  import Visibility from '#lib/organisms/Visibility.svelte';
  import { image, linksOf } from '#lib/images.ts';
  let { data, form } = $props();
  const g = $derived(data.group);
</script>

<svelte:head
  ><title>{m.route_group_settings_head({ group: g.displayName ?? g.handle })}</title></svelte:head
>

{#if form?.failed}<p class="danger">{form.failed}</p>{/if}
<Section title={m.route_group_settings_about()}>
  <ProfileForm
    displayName={g.displayName}
    description={g.description}
    joinPolicy={g.joinPolicy}
    url={g.url}
    location={g.location}
    online={g.online}
    topics={g.topics}
    languages={g.languages}
  />
</Section>
<Section title={m.route_group_settings_links()} count={g.links?.length ?? 0}>
  {#snippet actions()}
    {#if (g.links?.length ?? 0) < 5}<button class="quiet" type="button" popovertarget="add-link"
        >{m.route_group_settings_add_link()}</button
      >{/if}
  {/snippet}
  <LinksEditor links={linksOf(g.did, g.links)} />
</Section>
<Section title={m.route_group_settings_pictures()}>
  <ProfilePictures
    name={g.displayName ?? g.handle}
    avatar={image(g.did, g.avatar)}
    banner={image(g.did, g.banner)}
  />
</Section>
<Section title={m.route_group_settings_who_sees()}>
  <Visibility listed={data.listed} />
</Section>
<Section title={m.route_group_settings_handle()}>
  {#snippet actions()}<button class="quiet" type="button" popovertarget="use-domain"
      >{m.route_group_settings_use_domain()}</button
    >{/snippet}
  {#snippet intro()}<ParaglideMessage message={m.route_group_settings_handle_intro}>
      {#snippet address()}<span class="mono">{data.address}</span>{/snippet}
    </ParaglideMessage>{/snippet}
  <GroupHandle did={g.did} handle={g.handle} address={data.address} pointing={data.pointing} />
</Section>
<Section title={m.route_group_settings_switch_off()}>
  {#snippet intro()}{m.route_group_settings_switch_off_intro()}{/snippet}
  <SwitchOff />
</Section>
