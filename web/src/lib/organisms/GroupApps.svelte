<script lang="ts">
  import { ParaglideMessage } from '@inlang/paraglide-js-svelte';
  import Avatar from '#lib/atoms/Avatar.svelte';
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  import Empty from '#lib/molecules/Empty.svelte';
  import Person from '#lib/molecules/Person.svelte';
  import { appName, domain, type Client } from '#lib/activity.ts';
  import { day } from '#lib/dates.ts';
  import { appLogo } from '#lib/images.ts';
  import { m } from '#lib/paraglide/messages.js';
  import { reach } from '#lib/permissions.ts';
  /**
   * What acts as the group: each grant, by the app's name, icon and
   * domain as its client metadata gives them; what it may do, from its
   * scope; who approved it and how; to revoke, asked to be sure of.
   */
  type Session = {
    tokenId: string;
    clientId: string;
    client?: Client;
    member: string;
    actor?: { did: string; handle?: string };
    source: string;
    scope?: string;
    lastUsedAt?: string;
  };
  let { sessions }: { sessions: Session[] } = $props();
  // Who let it in, and how, said with when it was last used and without.
  const BY = {
    oauth: [m.group_apps_approved, m.group_apps_approved_used],
    appPassword: [m.group_apps_password, m.group_apps_password_used],
    getGroupAuth: [m.group_apps_console, m.group_apps_console_used]
  } as const;
  const by = (s: Session) => BY[s.source as keyof typeof BY] ?? BY.oauth;
  // A page of them at a time, more as asked for.
  let shown = $state(5);
</script>

{#if sessions.length}
  <ul class="sessions">
    {#each sessions.slice(0, shown) as s, i (s.tokenId)}
      {@const name = appName(s.clientId, s.client)}
      {@const at = s.client?.name ? domain(s.clientId) : undefined}
      <li>
        <Avatar app src={appLogo(s.client)} {name} />
        <span class="row-main">
          <span class="who"
            ><strong>{name}</strong>{#if at}<span class="mute domain">{at}</span>{/if}</span
          >
          {#if s.scope}<span class="what">{reach(s.scope)}</span>{/if}
          <span class="what"
            >{#if s.lastUsedAt}<ParaglideMessage
                message={by(s)[1]}
                inputs={{ used: day(s.lastUsedAt) }}
              >
                {#snippet person()}<Person actor={s.actor ?? { did: s.member }} />{/snippet}
              </ParaglideMessage>{:else}<ParaglideMessage message={by(s)[0]} inputs={{}}>
                {#snippet person()}<Person actor={s.actor ?? { did: s.member }} />{/snippet}
              </ParaglideMessage>{/if}</span
          >
        </span>
        <span class="row-actions"
          ><button
            class="quiet small"
            type="button"
            popovertarget={`revoke-${i}`}
            aria-label={m.group_apps_revoke_label({ app: name })}>{m.common_revoke()}</button
          ></span
        >
        <ConfirmDialog
          id={`revoke-${i}`}
          title={m.group_apps_revoke_title()}
          action="?/revoke"
          confirm={m.common_revoke()}
          fields={{ tokenId: s.tokenId }}
        >
          {m.group_apps_revoke_body({ app: name })}
        </ConfirmDialog>
      </li>
    {/each}
  </ul>
  {#if sessions.length > shown}<button
      class="quiet small"
      type="button"
      onclick={() => (shown += 5)}>{m.common_show_more()}</button
    >{/if}
{:else}
  <Empty title={m.group_apps_empty_title()}>{m.group_apps_empty_body()}</Empty>
{/if}
