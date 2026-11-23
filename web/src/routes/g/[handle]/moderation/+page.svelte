<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
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
    (
      [
        ['open', m.route_group_moderation_tab_open],
        ['escalated', m.route_group_moderation_tab_escalated],
        ['resolved', m.route_group_moderation_tab_resolved]
      ] as const
    ).map(([s, text]) => ({
      href: `${at}?status=${s}`,
      text: text()
    }))
  );
  const can = $derived({
    label: data.actions.includes('label'),
    resolve: data.actions.includes('mod.resolve')
  });
  const finds = $derived(!!data.subjects && can.label);
</script>

<svelte:head
  ><title>{m.route_group_moderation_head({ group: g.displayName ?? g.handle })}</title></svelte:head
>

<Page
  title={m.route_group_moderation_title()}
  back={{ href: `/g/${g.handle}`, text: g.displayName ?? g.handle }}
>
  {#snippet nav()}
    {#if data.subjects}<Tabs
        {tabs}
        current={`${page.url.pathname}?status=${data.status}`}
        label={m.route_group_moderation_queue()}
      />{/if}
  {/snippet}
  {#if form?.failed}<p class="danger">{form.failed}</p>{/if}
  {#if data.subjects}
    <Section title={m.route_group_moderation_subjects()} count={data.subjects.length}>
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
    <Empty title={m.route_group_moderation_not_yours_title()}
      >{m.route_group_moderation_not_yours_body()}</Empty
    >
  {/if}
</Page>
