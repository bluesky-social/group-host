<script>
  import Handle from '../atoms/Handle.svelte';
  import Empty from '../molecules/Empty.svelte';
  import { ago } from '../dates.js';
  /**
   * The place's discussions, the most lately active first, as a forum lists
   * them: each its title, opened by `open` (given the topic and the
   * click), who started it and when, how many replies it has and when the
   * last came (`replies`, a Map by URI of `{count, latestAt}`); what the
   * moderators found, if anything (`judgedOf`). With none, what fills it;
   * with older ones, `more` reads them.
   */
  let { topics, replies, judgedOf, handleOf, hrefOf, open, more = null } = $props();
  const count = (n) => (n ? `${n} ${n === 1 ? 'reply' : 'replies'}` : 'No replies yet');
</script>

{#if topics.length}
  <ul class="topics">
    {#each topics as topic (topic.uri)}
      {@const judged = judgedOf(topic)}
      <li class="topic" class:judged={judged.any}>
        <a class="topic-title" href={hrefOf(topic)} onclick={(e) => open(e, topic)}
          >{topic.value.title}</a
        >
        <p class="meta">
          <Handle handle={handleOf(topic.repo)} />, {ago(topic.value.createdAt)}.
          <span class="topic-count">{count(replies.get(topic.uri)?.count ?? 0)}</span
          >{#if replies.get(topic.uri)?.latestAt}{`, last reply ${ago(replies.get(topic.uri).latestAt)}`}{/if}.
        </p>
        {#if judged.names.length}<p class="judgement">
            Marked by the moderators: <strong>{judged.names.join(', ')}</strong>
          </p>{/if}
      </li>
    {/each}
  </ul>
  {#if more}<button type="button" class="quiet" onclick={more}>Older discussions</button>{/if}
{:else}
  <Empty title="No discussions yet"
    >Start one: a question for everyone, a plan, or something worth talking over.</Empty
  >
{/if}
