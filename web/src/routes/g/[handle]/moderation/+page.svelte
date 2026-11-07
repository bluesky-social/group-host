<script lang="ts">
  import Empty from '#lib/molecules/Empty.svelte';
  import Section from '#lib/molecules/Section.svelte';
  import Tabs from '#lib/molecules/Tabs.svelte';
  import FindAccount from '#lib/organisms/FindAccount.svelte';
  import ModerationQueue from '#lib/organisms/ModerationQueue.svelte';
  import Page from '#lib/templates/Page.svelte';
  import { page } from '$app/state';
  let { data, form } = $props();
  const g = $derived(data.group);
  const at = $derived(`/g/${g.handle}/moderation`);
  const tabs = $derived(
    ['open', 'escalated', 'resolved'].map((s) => ({
      href: `${at}?status=${s}`,
      text: s[0].toUpperCase() + s.slice(1)
    }))
  );
  const can = $derived({
    label: data.actions.includes('label'),
    takedown: data.actions.includes('takedown'),
    resolve: data.actions.includes('mod.resolve')
  });
  const finds = $derived(!!data.subjects && (can.label || can.takedown));
</script>

<svelte:head><title>Moderation · {g.displayName ?? g.handle} · opensocial</title></svelte:head>

<Page title="Moderation" back={{ href: `/g/${g.handle}`, text: g.displayName ?? g.handle }}>
  {#snippet nav()}
    {#if data.subjects}<Tabs
        {tabs}
        current={`${page.url.pathname}?status=${data.status}`}
        label="The queue"
      />{/if}
  {/snippet}
  {#if form?.failed}<p class="danger">{form.failed}</p>{/if}
  {#if data.subjects}
    <Section title="Subjects" count={data.subjects.length}>
      {#if finds}<FindAccount />{/if}
      <ModerationQueue
        subjects={data.subjects}
        status={data.status}
        {can}
        rules={data.rules}
        ruleList={data.ruleList}
        reasons={data.reasons}
        found={data.found}
        reviewing={data.reviewing}
        defined={g.labels ?? []}
      />
    </Section>
  {:else}
    <Empty title="Nothing here is yours to see"
      >Only people whose roles let them read the moderation queue can see it.</Empty
    >
  {/if}
</Page>
