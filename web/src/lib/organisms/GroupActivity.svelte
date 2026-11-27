<script lang="ts">
  import { ParaglideMessage } from '@inlang/paraglide-js-svelte';
  import Avatar from '#lib/atoms/Avatar.svelte';
  import Empty from '#lib/molecules/Empty.svelte';
  import Person from '#lib/molecules/Person.svelte';
  import { about, bare, byApp, byPassword, line, type Act } from '#lib/activity.ts';
  import { when } from '#lib/dates.ts';
  import { appLogo } from '#lib/images.ts';
  import { m } from '#lib/paraglide/messages.js';
  /**
   * What was done as the group, newest first: a row for each write or
   * change to a space, as a sentence (who did what); under it, quieter,
   * what another app's record is, as its lexicon says; then, for an app,
   * the member who approved it or made its password, and when; for a
   * member in the console, that it was there, and when.
   */
  let { activity }: { activity: Act[] } = $props();
  const done = $derived(activity.filter((a) => a.write || a.space));
  // A page of them at a time, more as asked for.
  let shown = $state(10);
</script>

{#if done.length}
  <ul class="activity">
    {#each done.slice(0, shown) as a, i (i)}
      {@const described = about(a)}
      <li>
        <Avatar app={byApp(a)} src={appLogo(a.client)} name={bare(a)} />
        <span class="row-main">
          <span class="who">{line(a)}</span>
          {#if described}<span class="what">{described}</span>{/if}
          <span class="what"
            >{#if byApp(a)}<ParaglideMessage
                message={byPassword(a) ? m.group_activity_password : m.group_activity_approved}
                inputs={{ when: when(a.at) }}
              >
                {#snippet person()}<Person actor={a.actor ?? { did: a.member }} />{/snippet}
              </ParaglideMessage>{:else}{m.group_activity_console({ when: when(a.at) })}{/if}</span
          >
        </span>
      </li>
    {/each}
  </ul>
  {#if done.length > shown}<button class="quiet small" type="button" onclick={() => (shown += 10)}
      >{m.common_show_more()}</button
    >{/if}
{:else}
  <Empty title={m.group_activity_empty_title()}>{m.group_activity_empty_body()}</Empty>
{/if}
