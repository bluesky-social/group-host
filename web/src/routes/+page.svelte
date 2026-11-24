<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import GroupList from '#lib/organisms/GroupList.svelte';
  import Invitations from '#lib/organisms/Invitations.svelte';
  import Section from '#lib/molecules/Section.svelte';
  import Page from '#lib/templates/Page.svelte';
  import Welcome from '#lib/templates/Welcome.svelte';
  import { page } from '$app/state';
  let { data, form } = $props();
</script>

<svelte:head><title>{m.route_home_head()}</title></svelte:head>

{#if !data.viewer}
  <Welcome title={m.route_home_welcome_title({ host: page.url.hostname })}
    >{m.route_home_welcome_body()}</Welcome
  >
{:else}
  <Page title={m.route_home_title()}>
    {#snippet actions()}<a class="button" href="/new">{m.route_home_start()}</a>{/snippet}
    {#if form?.failed}<p class="danger">{form.failed}</p>{/if}
    <Section id="invites" title={m.route_home_invites()} count={data.invitations.length}>
      <Invitations invitations={data.invitations} />
    </Section>
    {#if data.mine}
      <Section title={m.route_home_mine()} count={data.mine.length}>
        <GroupList groups={data.mine} />
      </Section>
    {/if}
  </Page>
{/if}
