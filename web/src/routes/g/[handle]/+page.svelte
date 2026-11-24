<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import ConnectAiApp from '#lib/organisms/ConnectAiApp.svelte';
  import ConnectedApps from '#lib/organisms/ConnectedApps.svelte';
  import GroupCard from '#lib/organisms/GroupCard.svelte';
  import InviteDialog from '#lib/organisms/InviteDialog.svelte';
  import Invites from '#lib/organisms/Invites.svelte';
  import JoinPanel from '#lib/organisms/JoinPanel.svelte';
  import JoinRequests from '#lib/organisms/JoinRequests.svelte';
  import MemberList from '#lib/organisms/MemberList.svelte';
  import Section from '#lib/molecules/Section.svelte';
  import Standing from '#lib/molecules/Standing.svelte';
  import SwitchedOff from '#lib/molecules/SwitchedOff.svelte';
  import { image } from '#lib/images.ts';
  import GroupPage from '#lib/templates/GroupPage.svelte';
  import Welcome from '#lib/templates/Welcome.svelte';
  let { data, form } = $props();
  const g = $derived(data.group);
  const may = $derived(g.viewer?.actions ?? []);
  const member = $derived(g.viewer?.member ?? false);
  // What the page calls the group: its name, or its handle where its profile is not to be read.
  const name = $derived(g.displayName ?? g.handle);
  // The roles the person looking may give: those their roles' grants say,
  // or every role, where one says `*`.
  const assignable = $derived.by(() => {
    if (!may.includes('role.assign')) return [];
    const given = new Set(
      (data.bindings ?? [])
        .filter((b) => g.viewer?.roles.some((r) => r.role === b.role))
        .flatMap((b) => b.assignable.map((a) => a.role))
    );
    return given.has('*') ? (data.roles ?? []).map((r) => r.key) : [...given];
  });
  const joining: Record<string, () => string> = {
    approval: m.route_group_joining_approval,
    open: m.route_group_joining_open,
    invite: m.route_group_joining_invite
  };
</script>

<svelte:head><title>{m.route_group_head({ group: name })}</title></svelte:head>

{#if !data.viewer}
  <Welcome title={g.handle} handle>{m.route_group_welcome_body()}</Welcome>
{:else}
  <GroupPage>
    {#snippet notice()}
      {#if form?.failed}<p class="danger">{form.failed}</p>{/if}
    {/snippet}
    {#snippet card()}
      <GroupCard
        handle={g.handle}
        name={g.displayName}
        description={g.description}
        avatar={image(g.did, g.avatar)}
        banner={image(g.did, g.banner)}
        rules={g.rules}
      >
        {#snippet nav()}
          {#if data.site || (member && (data.settings || may.includes('mod.read')))}
            <nav class="group-nav" aria-label={m.route_group_nav()}>
              {#if member && may.includes('mod.read')}<a
                  class="button quiet small"
                  href={`/g/${g.handle}/moderation`}>{m.route_group_moderation()}</a
                >{/if}
              {#if member && data.settings}<a
                  class="button quiet small"
                  href={`/g/${g.handle}/settings`}>{m.route_group_settings()}</a
                >{/if}
              {#if data.site}<a class="button quiet small" href={data.site.url}
                  >{m.route_group_website()}</a
                >{/if}
            </nav>
          {/if}
        {/snippet}
        {#if member}
          <Standing roles={g.viewer?.roles.map((r) => r.role) ?? []} />
        {:else if g.displayName}
          <p class="mute small">{(joining[g.joinPolicy ?? 'approval'] ?? joining.approval)()}</p>
        {/if}
      </GroupCard>
    {/snippet}
    {#if g.deactivated}
      <SwitchedOff steward={g.viewer?.steward ?? false} />
    {:else}
      {#if !member}
        <JoinPanel
          {name}
          joinPolicy={g.joinPolicy}
          asked={!!g.viewer?.request}
          invited={data.invited}
        />
      {:else if form?.joined}
        <JoinPanel {name} joined />
      {/if}
      {#if data.joinRequests}
        <Section title={m.route_group_asking()} count={data.joinRequests.length}>
          <JoinRequests requests={data.joinRequests} />
        </Section>
      {/if}
      {#if data.members}
        <Section title={m.route_group_members()} count={g.memberCount}>
          {#snippet actions()}
            {#if data.invites}<button class="quiet" type="button" popovertarget="invite"
                >{m.route_group_invite()}</button
              >{/if}
          {/snippet}
          <MemberList
            members={data.members}
            q={data.q}
            shown={data.shown}
            more={data.more}
            me={data.viewer?.did}
            {assignable}
            canEject={may.includes('eject')}
          />
        </Section>
      {/if}
      {#if data.invites}
        <InviteDialog id="invite" />
        {#if data.invites.length}
          <Section title={m.route_group_invites()} count={data.invites.length}>
            <Invites invites={data.invites} />
          </Section>
        {/if}
      {/if}
      {#if may.includes('group.configure')}
        <Section title={m.route_group_connect_ai()}>
          <ConnectAiApp {name} mcp={data.mcp} />
          <ConnectedApps apps={data.apps ?? []} />
        </Section>
      {:else if data.apps?.length}
        <Section title={m.route_group_your_ai_apps()}>
          <ConnectedApps apps={data.apps} />
        </Section>
      {/if}
    {/if}
  </GroupPage>
{/if}
