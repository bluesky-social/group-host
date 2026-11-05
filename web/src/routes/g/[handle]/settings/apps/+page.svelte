<script lang="ts">
  import Section from '#lib/molecules/Section.svelte';
  import AppPasswords from '#lib/organisms/AppPasswords.svelte';
  import GroupActivity from '#lib/organisms/GroupActivity.svelte';
  import GroupApps from '#lib/organisms/GroupApps.svelte';
  let { data, form } = $props();
  const g = $derived(data.group);
</script>

<svelte:head><title>Apps · {g.displayName ?? g.handle} · opensocial</title></svelte:head>

{#if form?.failed}<p class="danger">{form.failed}</p>{/if}
{#if data.tokens}
  <Section title="Apps acting as the group" count={data.tokens.sessions.length}>
    <GroupApps sessions={data.tokens.sessions} />
  </Section>
{/if}
{#if data.passwords}
  <Section title="App passwords" count={data.passwords.length}>
    {#snippet actions()}<button class="quiet" type="button" popovertarget="make-password"
        >Create an app password</button
      >{/snippet}
    <AppPasswords passwords={data.passwords} made={form?.made} />
  </Section>
{/if}
{#if data.tokens}
  <Section title="What apps did as the group">
    {#snippet intro()}Each write an app made while signed in as the group, newest first.{/snippet}
    <GroupActivity activity={data.tokens.activity} />
  </Section>
{/if}
