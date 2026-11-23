<script lang="ts">
  import { ParaglideMessage } from '@inlang/paraglide-js-svelte';
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  import Empty from '#lib/molecules/Empty.svelte';
  import Person from '#lib/molecules/Person.svelte';
  import { appName } from '#lib/activity.ts';
  import { day } from '#lib/dates.ts';
  import { m } from '#lib/paraglide/messages.js';
  /** What acts as the group: each grant, whose it is and how it came, to revoke, asked to be sure of. */
  type Session = {
    tokenId: string;
    clientId: string;
    member: string;
    actor?: { did: string; handle?: string };
    source: string;
    scope?: string;
    lastUsedAt?: string;
  };
  let { sessions }: { sessions: Session[] } = $props();
  // How each came to act as the group, as a reader knows it.
  const how: Record<string, () => string> = {
    oauth: m.group_apps_how_oauth,
    getGroupAuth: m.group_apps_how_console,
    appPassword: m.group_apps_how_app_password
  };
  // A page of them at a time, more as asked for.
  let shown = $state(5);
</script>

{#if sessions.length}
  <ul class="sessions">
    {#each sessions.slice(0, shown) as s, i (s.tokenId)}
      {@const came = how[s.source]?.() ?? s.source}
      <li>
        <span class="row-main">
          <strong>{appName(s.clientId)}</strong>
          <span class="what"
            >{#if s.lastUsedAt}<ParaglideMessage
                message={m.group_apps_for_used}
                inputs={{ how: came, used: day(s.lastUsedAt) }}
              >
                {#snippet person()}<Person actor={s.actor ?? { did: s.member }} />{/snippet}
              </ParaglideMessage>{:else}<ParaglideMessage
                message={m.group_apps_for}
                inputs={{ how: came }}
              >
                {#snippet person()}<Person actor={s.actor ?? { did: s.member }} />{/snippet}
              </ParaglideMessage>{/if}</span
          >
        </span>
        <span class="row-actions"
          ><button
            class="quiet small"
            type="button"
            popovertarget={`revoke-${i}`}
            aria-label={m.group_apps_revoke_label({ app: s.clientId })}>{m.common_revoke()}</button
          ></span
        >
        <ConfirmDialog
          id={`revoke-${i}`}
          title={m.group_apps_revoke_title()}
          action="?/revoke"
          confirm={m.common_revoke()}
          fields={{ tokenId: s.tokenId }}
        >
          {m.group_apps_revoke_body({ app: appName(s.clientId) })}
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
