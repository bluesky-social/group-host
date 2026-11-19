<script>
  import Post from '../molecules/Post.svelte';
  import ReplyForm from './ReplyForm.svelte';
  import ReplyTree from './ReplyTree.svelte';
  /**
   * A discussion opened: its topic, how many replies it has (`count`),
   * the replies, each under what it answers (as ReplyTree takes them),
   * and replying to the topic itself at the end (`reply`, given none to
   * answer, or a reply's URI, and the words).
   */
  let { topic, count, childrenOf, handleOf, judgedOf, sees, menu, reply } = $props();
  let replying = $state(null);
</script>

<Post
  post={topic}
  handle={handleOf(topic.repo)}
  judged={judgedOf(topic)}
  sees={sees(topic)}
  {menu}
/>
<h2 class="thread-count">
  {count ? `${count} ${count === 1 ? 'reply' : 'replies'}` : 'No replies yet'}
</h2>
<ReplyTree
  under={topic.uri}
  {childrenOf}
  {handleOf}
  {judgedOf}
  {sees}
  {menu}
  {reply}
  bind:replying
/>
<ReplyForm label="Add to the discussion" send={(text) => reply(null, text)} />
