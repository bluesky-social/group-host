<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import Empty from '#lib/molecules/Empty.svelte';
  import Section from '#lib/molecules/Section.svelte';
  import Tabs from '#lib/molecules/Tabs.svelte';
  import FindAccount from '#lib/organisms/FindAccount.svelte';
  import ModerationQueue from '#lib/organisms/ModerationQueue.svelte';
  import Page from '#lib/templates/Page.svelte';
  import { ESCALATED, besideOf } from '#lib/moderation.ts';
  let { data, form } = $props();
  const g = $derived(data.group);
  const at = $derived(`/g/${g.handle}/moderation`);
  // Each tab by what the queue is asked for: escalated is the open ones tagged so.
  const asked = {
    open: '?status=open',
    escalated: `?status=open&tags=${ESCALATED}`,
    resolved: '?status=resolved'
  };
  const tabs = $derived([
    { href: `${at}${asked.open}`, text: m.route_group_moderation_tab_open() },
    { href: `${at}${asked.escalated}`, text: m.route_group_moderation_tab_escalated() },
    { href: `${at}${asked.resolved}`, text: m.route_group_moderation_tab_resolved() }
  ]);
  // Labelling a subject where its labels go, if the viewer's roles label there.
  const labelling = $derived((g.viewer?.labelSpaces ?? []).map((s): string => s.space));
  const can = $derived({
    label: (key: string) => labelling.includes(besideOf(g.did, key)),
    resolve: data.actions.includes('mod.resolve')
  });
  const finds = $derived(!!data.subjects && can.label(g.did));
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
        current={`${at}${asked[data.shown]}`}
        label={m.route_group_moderation_queue()}
      />{/if}
  {/snippet}
  {#if form?.failed}<p class="danger">{form.failed}</p>{/if}
  {#if data.subjects}
    <Section title={m.route_group_moderation_subjects()} count={data.subjects.length}>
      {#if finds}<FindAccount />{/if}
      <ModerationQueue
        subjects={data.subjects}
        shown={data.shown}
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
