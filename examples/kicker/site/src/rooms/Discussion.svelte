<script>
  import RoomHead from '../lib/molecules/RoomHead.svelte';
  import AppealDialog from '../lib/organisms/AppealDialog.svelte';
  import NewTopicDialog from '../lib/organisms/NewTopicDialog.svelte';
  import ReportDialog from '../lib/organisms/ReportDialog.svelte';
  import Thread from '../lib/organisms/Thread.svelte';
  import TopicList from '../lib/organisms/TopicList.svelte';
  import { follow, keep, kept } from '../lib/kept.svelte.js';
  import {
    LABEL,
    NS,
    appeal,
    create,
    handleOf,
    judged,
    keyOf,
    moderates,
    remove,
    report,
    reviewAt,
    rulesOf
  } from '../lib/place.js';
  import { go, hrefs, name, route, topicHref } from '../lib/route.svelte.js';
  import { labelsOn, thread, topicsByActivity, under as underOf } from '../lib/views.js';
  /**
   * Discussion: the place's discussions, the most lately active first,
   * each with how many replies it has; and one opened, its replies oldest
   * first, each under what it answers, replied to in place. All worked out
   * from what the room holds. What may be done to each post, as on the
   * board: reported, appealed, deleted by its author, reviewed in the
   * console by the moderators; one hidden is left out, but for them and
   * its author, or kept as a line where it has replies of its own.
   */
  let { ctx } = $props();
  let failed = $state('');

  const uriOf = (topic) => `${ctx.space('room')}/${topic.did}/${NS}.topic/${topic.rkey}`;
  const opened = $derived(route.topic ? uriOf(route.topic) : null);
  const collections = { topic: `${NS}.topic`, reply: `${NS}.reply` };

  const load = async () => {
    try {
      failed = '';
      await keep(ctx, 'room');
    } catch (e) {
      failed = e.message;
    }
  };
  $effect(() => {
    void ctx;
    void opened;
    load();
    return follow(ctx, 'room');
  });
  const then =
    (act) =>
    async (...args) => {
      await act(...args);
      await load();
    };

  const labels = $derived(kept.room ? labelsOn(kept.room, ctx.group.did, LABEL) : new Map());
  const judgedOf = (post) => judged(ctx, labels.get(post.uri));
  const mine = (post) => post.repo === ctx.me.did;
  const sees = (post) => mine(post) || moderates(ctx);
  const hidden = (post) => judgedOf(post).setting === 'hide' && !sees(post);

  // A member who does not moderate is shown no hidden reply, nor has it
  // counted or moving a discussion up; a moderator, all.
  const read = $derived(
    kept.room &&
      (opened
        ? thread(kept.room, collections, opened)
        : topicsByActivity(
            kept.room,
            collections,
            moderates(ctx) ? () => false : (uri) => judged(ctx, labels.get(uri)).setting === 'hide'
          ))
  );
  $effect(() => {
    if (opened && read?.topic) name(read.topic.value.title);
  });

  /** The replies under `uri`, each shown, or kept as a line if hidden with replies of its own. */
  const under = $derived(underOf(opened && read ? read.replies : []));
  const childrenOf = (uri) =>
    (under.get(uri) ?? [])
      .map((post) => ({ post, shown: !hidden(post) }))
      .filter(({ post, shown }) => shown || childrenOf(post.uri).length);

  const topicOf = (post) => ({ did: post.repo, rkey: keyOf(post.uri) });
  const start = then((topic) => create(ctx, 'room', 'topic', topic));
  const reply = then((parent, text) =>
    create(ctx, 'room', 'reply', { root: opened, ...(parent ? { parent } : {}), text })
  );

  /** The post a dialog is open for, and which. */
  let on = $state(null);
  let reporting = $state(false);
  let appealing = $state(false);
  const del = async (post) => {
    await remove(
      ctx,
      'room',
      post.uri.includes(`/${NS}.topic/`) ? 'topic' : 'reply',
      keyOf(post.uri)
    );
    if (post.uri === opened) {
      route.topic = null;
      history.pushState(null, '', hrefs.discussion);
      name();
    }
    await load();
  };
  const didTo = (j) =>
    j.names.length ? `marked your post ${j.names.join(', ')}` : 'hid your post from members';
</script>

{#snippet menu(post, done)}
  {#if mine(post)}
    {#if judgedOf(post).any}<button
        type="button"
        class="menu-item"
        role="menuitem"
        onclick={() => (done(), (on = post), (appealing = true))}>Appeal</button
      >{/if}
    <button
      type="button"
      class="menu-item danger"
      role="menuitem"
      onclick={() => (done(), del(post))}>Delete</button
    >
  {:else}
    <button
      type="button"
      class="menu-item"
      role="menuitem"
      onclick={() => (done(), (on = post), (reporting = true))}>Report</button
    >
  {/if}
  {#if moderates(ctx) && judgedOf(post).any}<a
      class="menu-item"
      role="menuitem"
      href={reviewAt(ctx, post.uri)}
      target="_blank"
      rel="noopener"
      onclick={done}>Review in the console</a
    >{/if}
{/snippet}

{#if opened}
  <p class="back">
    <a href={hrefs.discussion} onclick={(e) => go(e, 'discussion')}>All discussions</a>
  </p>
  {#if failed}<p class="failed">{failed}</p>{/if}
  {#if read?.topic && !hidden(read.topic)}
    <Thread
      topic={read.topic}
      count={read.replies.length}
      {childrenOf}
      handleOf={(did) => handleOf(ctx, did)}
      {judgedOf}
      {sees}
      {menu}
      {reply}
    />
  {:else if read}
    <p class="meta">This discussion is gone.</p>
  {:else}
    <p class="meta">Reading the discussion…</p>
  {/if}
{:else}
  <RoomHead title="Discussion">
    {#snippet act()}<NewTopicDialog {start} />{/snippet}
  </RoomHead>
  {#if failed}<p class="failed">{failed}</p>{/if}
  {#if read}
    <TopicList
      topics={read.topics.filter((t) => !hidden(t))}
      replies={read.replies}
      {judgedOf}
      handleOf={(did) => handleOf(ctx, did)}
      hrefOf={(t) => topicHref(t.repo, keyOf(t.uri))}
      open={(e, t) => go(e, 'discussion', topicOf(t))}
    />
  {:else}
    <p class="meta">Reading the discussions…</p>
  {/if}
{/if}
<ReportDialog
  bind:open={reporting}
  title="Report this post"
  report={(reasonType, more) => report(ctx, on, reasonType, more)}
  rules={rulesOf(ctx)}
  reviewAt={moderates(ctx) && on ? reviewAt(ctx, on.uri) : null}
/>
<AppealDialog
  bind:open={appealing}
  appeal={(reason) => appeal(ctx, on, reason)}
  did={on ? didTo(judgedOf(on)) : ''}
/>
