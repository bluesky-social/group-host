<script lang="ts">
  import { ParaglideMessage } from '@inlang/paraglide-js-svelte';
  import Avatar from '#lib/atoms/Avatar.svelte';
  import Empty from '#lib/molecules/Empty.svelte';
  import Person from '#lib/molecules/Person.svelte';
  import { bare, byApp, doer, line, type Act } from '#lib/activity.ts';
  import { when } from '#lib/dates.ts';
  import { m } from '#lib/paraglide/messages.js';
  /**
   * What apps wrote as the group, newest first: a row for each, as a
   * sentence (who did what), and under it, quieter, the member who
   * approved the app and when.
   */
  let { activity }: { activity: Act[] } = $props();
  const writes = $derived(activity.filter((a) => a.write));
  // A page of them at a time, more as asked for.
  let shown = $state(10);
</script>

{#if writes.length}
  <ul class="activity">
    {#each writes.slice(0, shown) as a, i (i)}
      <li>
        <Avatar name={bare(doer(a))} />
        <span class="row-main">
          <span class="who">{line(a)}</span>
          <span class="what"
            >{#if byApp(a)}<ParaglideMessage
                message={m.group_activity_for}
                inputs={{ when: when(a.at) }}
              >
                {#snippet person()}<Person actor={a.actor ?? { did: a.member }} />{/snippet}
              </ParaglideMessage>{:else}{when(a.at)}{/if}</span
          >
        </span>
      </li>
    {/each}
  </ul>
  {#if writes.length > shown}<button class="quiet small" type="button" onclick={() => (shown += 10)}
      >{m.common_show_more()}</button
    >{/if}
{:else}
  <Empty title={m.group_activity_empty_title()}>{m.group_activity_empty_body()}</Empty>
{/if}
