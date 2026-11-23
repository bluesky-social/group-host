<script lang="ts">
  import { enhance } from '$app/forms';
  import { inPlace } from '#lib/forms.ts';
  import Empty from '#lib/molecules/Empty.svelte';
  import GroupRow from '#lib/molecules/GroupRow.svelte';
  import { image } from '#lib/images.ts';
  import { m } from '#lib/paraglide/messages.js';
  /** The groups that have invited the person looking, a row each with its note: each to join, or decline; or that none is waiting. */
  let {
    invitations
  }: {
    invitations: {
      group: { did: string; handle: string; displayName?: string; avatar?: string };
      message?: string;
    }[];
  } = $props();
</script>

{#if !invitations.length}
  <Empty title={m.invitations_empty_title()}>{m.invitations_empty_body()}</Empty>
{:else}
  <ul class="invitations">
    {#each invitations as i (i.group.did)}
      <li>
        <GroupRow
          name={i.group.displayName ?? i.group.handle}
          handle={i.group.handle}
          avatar={image(i.group.did, i.group.avatar)}
          note={i.message}
        />
        <span class="row-actions">
          <form method="post" action="?/decline" use:enhance={inPlace}>
            <input type="hidden" name="group" value={i.group.did} /><button
              class="quiet small"
              type="submit">{m.common_decline()}</button
            >
          </form>
          <form method="post" action="?/accept" use:enhance={inPlace}>
            <input type="hidden" name="group" value={i.group.did} /><button
              class="small"
              type="submit"
              aria-label={m.invitations_join_label({
                group: i.group.displayName ?? i.group.handle
              })}>{m.common_join()}</button
            >
          </form>
        </span>
      </li>
    {/each}
  </ul>
{/if}
