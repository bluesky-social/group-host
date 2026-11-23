<script lang="ts">
  import { enhance } from '$app/forms';
  import { inPlace } from '#lib/forms.ts';
  import Empty from '#lib/molecules/Empty.svelte';
  import MemberRow from '#lib/molecules/MemberRow.svelte';
  import { m } from '#lib/paraglide/messages.js';
  /** The requests waiting to join, for a member whose roles admit people: each with its note, approved or declined. */
  let {
    requests
  }: {
    requests: { member: string; message?: string; actor?: { did: string; handle?: string } }[];
  } = $props();
</script>

{#if requests.length}
  <ul class="requests">
    {#each requests as r (r.member)}
      <li>
        <MemberRow actor={r.actor ?? { did: r.member }} note={r.message}>
          <form method="post" action="?/deny" use:enhance={inPlace}>
            <input type="hidden" name="did" value={r.member} /><button
              class="quiet small"
              type="submit">{m.common_decline()}</button
            >
          </form>
          <form method="post" action="?/admit" use:enhance={inPlace}>
            <input type="hidden" name="did" value={r.member} /><button class="small" type="submit"
              >{m.common_approve()}</button
            >
          </form>
        </MemberRow>
      </li>
    {/each}
  </ul>
{:else}
  <Empty title={m.join_requests_empty_title()}>{m.join_requests_empty_body()}</Empty>
{/if}
