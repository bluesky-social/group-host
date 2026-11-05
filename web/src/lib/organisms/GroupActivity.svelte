<script lang="ts">
  import Avatar from '#lib/atoms/Avatar.svelte';
  import Empty from '#lib/molecules/Empty.svelte';
  import Person from '#lib/molecules/Person.svelte';
  import { bare, byApp, did, doer, type Act } from '#lib/activity.ts';
  import { when } from '#lib/dates.ts';
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
          <span class="who">{doer(a)} {did(a)}</span>
          <span class="what"
            >{#if byApp(a)}For <Person actor={a.actor ?? { did: a.member }} />{', '}{/if}{when(
              a.at
            )}</span
          >
        </span>
      </li>
    {/each}
  </ul>
  {#if writes.length > shown}<button class="quiet small" type="button" onclick={() => (shown += 10)}
      >Show more</button
    >{/if}
{:else}
  <Empty title="No app has written anything yet"
    >When an app signed in as the group writes something, it shows up here.</Empty
  >
{/if}
