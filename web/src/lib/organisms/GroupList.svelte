<script lang="ts">
  import Empty from '#lib/molecules/Empty.svelte';
  import GroupRow from '#lib/molecules/GroupRow.svelte';
  import { image } from '#lib/images.ts';
  import { m } from '#lib/paraglide/messages.js';
  /** The groups someone is in, a row each, or what it says when they are in none. */
  let {
    groups
  }: { groups: { did: string; handle: string; displayName?: string; avatar?: string }[] } =
    $props();
</script>

{#if groups.length}
  <ul class="group-rows">
    {#each groups as g (g.did)}<li>
        <GroupRow
          name={g.displayName ?? g.handle}
          handle={g.handle}
          avatar={image(g.did, g.avatar)}
        />
      </li>{/each}
  </ul>
{:else}
  <Empty title={m.group_list_empty_title()}>{m.group_list_empty_body()}</Empty>
{/if}
