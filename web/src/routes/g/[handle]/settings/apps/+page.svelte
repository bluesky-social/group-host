<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import Section from '#lib/molecules/Section.svelte';
  import AppPasswords from '#lib/organisms/AppPasswords.svelte';
  import GroupActivity from '#lib/organisms/GroupActivity.svelte';
  import GroupApps from '#lib/organisms/GroupApps.svelte';
  let { data, form } = $props();
  const g = $derived(data.group);
</script>

<svelte:head
  ><title>{m.route_group_settings_apps_head({ group: g.displayName ?? g.handle })}</title
  ></svelte:head
>

{#if form?.failed}<p class="danger">{form.failed}</p>{/if}
{#if data.tokens}
  <Section title={m.route_group_settings_apps_acting()} count={data.tokens.sessions.length}>
    <GroupApps sessions={data.tokens.sessions} />
  </Section>
{/if}
{#if data.passwords}
  <Section title={m.route_group_settings_apps_passwords()} count={data.passwords.length}>
    {#snippet actions()}<button class="quiet" type="button" popovertarget="make-password"
        >{m.route_group_settings_apps_create_password()}</button
      >{/snippet}
    <AppPasswords passwords={data.passwords} made={form?.made} />
  </Section>
{/if}
{#if data.tokens}
  <Section title={m.route_group_settings_apps_activity()}>
    {#snippet intro()}{m.route_group_settings_apps_activity_intro()}{/snippet}
    <GroupActivity activity={data.tokens.activity} />
  </Section>
{/if}
