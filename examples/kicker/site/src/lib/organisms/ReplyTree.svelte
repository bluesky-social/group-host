<script>
  import Post from '../molecules/Post.svelte';
  import ReplyForm from './ReplyForm.svelte';
  import ReplyTree from './ReplyTree.svelte';
  /**
   * The replies to `under` (a topic's URI, or a reply's), oldest first,
   * each with the replies to it beneath, drawn the same way: `childrenOf`
   * a URI gives them, each with whether it is `shown`, or only a line
   * saying the moderators hid it, as it has replies of its own. Each
   * reply, replied to in place (`replying`, the URI being answered;
   * `reply`, to send), as `ctx` lets: `handleOf`, `judgedOf`, `sees` and
   * `menu`, as Post takes them.
   */
  let {
    under,
    childrenOf,
    handleOf,
    judgedOf,
    sees,
    menu,
    reply,
    replying = $bindable(null)
  } = $props();
  const children = $derived(childrenOf(under));
</script>

{#if children.length}
  <ul class="replies">
    {#each children as { post, shown } (post.uri)}
      <li class="reply">
        {#if shown}
          <Post
            {post}
            handle={handleOf(post.repo)}
            judged={judgedOf(post)}
            sees={sees(post)}
            {menu}
          >
            {#if replying === post.uri}
              <ReplyForm send={(text) => reply(post.uri, text)} cancel={() => (replying = null)} />
            {:else}
              <button type="button" class="text" onclick={() => (replying = post.uri)}>Reply</button
              >
            {/if}
          </Post>
        {:else}
          <p class="judgement">A reply hidden by the moderators.</p>
        {/if}
        <ReplyTree
          under={post.uri}
          {childrenOf}
          {handleOf}
          {judgedOf}
          {sees}
          {menu}
          {reply}
          bind:replying
        />
      </li>
    {/each}
  </ul>
{/if}
