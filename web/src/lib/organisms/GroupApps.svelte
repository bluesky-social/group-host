<script lang="ts">
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  import Empty from '#lib/molecules/Empty.svelte';
  import Person from '#lib/molecules/Person.svelte';
  import { appName } from '#lib/activity.ts';
  import { day } from '#lib/dates.ts';
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
  const how: Record<string, string> = {
    oauth: 'Signed in with OAuth',
    getGroupAuth: 'From this console',
    appPassword: 'With an app password'
  };
  // A page of them at a time, more as asked for.
  let shown = $state(5);
</script>

{#if sessions.length}
  <ul class="sessions">
    {#each sessions.slice(0, shown) as s, i (s.tokenId)}
      <li>
        <span class="row-main">
          <strong>{appName(s.clientId)}</strong>
          <span class="what"
            >{how[s.source] ?? s.source}, for <Person
              actor={s.actor ?? { did: s.member }}
            />{s.lastUsedAt ? `; last used ${day(s.lastUsedAt)}` : ''}</span
          >
        </span>
        <span class="row-actions"
          ><button
            class="quiet small"
            type="button"
            popovertarget={`revoke-${i}`}
            aria-label={`Revoke ${s.clientId}'s access`}>Revoke</button
          ></span
        >
        <ConfirmDialog
          id={`revoke-${i}`}
          title="Revoke this app's access?"
          action="?/revoke"
          confirm="Revoke"
          fields={{ tokenId: s.tokenId }}
        >
          {appName(s.clientId)} can no longer act as the group. It can sign in again later.
        </ConfirmDialog>
      </li>
    {/each}
  </ul>
  {#if sessions.length > shown}<button
      class="quiet small"
      type="button"
      onclick={() => (shown += 5)}>Show more</button
    >{/if}
{:else}
  <Empty title="No apps are signed in as this group"
    >When a member signs an app in as the group, it shows up here.</Empty
  >
{/if}
