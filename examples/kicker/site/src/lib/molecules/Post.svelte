<script>
  import Handle from '../atoms/Handle.svelte';
  import Menu from './Menu.svelte';
  import { ago } from '../dates.js';
  /**
   * A post in a discussion, its opening or a reply: who wrote it and when,
   * and the menu of what may be done to it (`menu`, given the post and
   * Menu's `done`), across the top; its title, if it opens one; its words;
   * then what follows it (`children`, such as replying). As the group's
   * labels on it say (`judged`), a line saying what the moderators found,
   * and folded behind it until opened, but for its author and the
   * moderators (`sees`).
   */
  let {
    post,
    handle,
    judged = { setting: 'show', names: [], any: false },
    sees = false,
    menu,
    children
  } = $props();
  let opened = $state(false);
  const marked = $derived(judged.names.join(', '));
  const folded = $derived(judged.setting === 'warn' && !sees && !opened);
</script>

<article class="post" class:judged={judged.any}>
  <div class="post-head">
    <span><Handle {handle} /><span class="meta">, {ago(post.value.createdAt)}</span></span>
    {#if menu}
      {#snippet items(done)}{@render menu(post, done)}{/snippet}
      <Menu label="What to do with this post" {items} />
    {/if}
  </div>
  {#if judged.setting === 'hide'}<p class="judgement">
      Hidden from members by the moderators{#if marked}: <strong>{marked}</strong>{:else}.{/if}
    </p>{:else if marked}<p class="judgement">
      Marked by the moderators: <strong>{marked}</strong>
    </p>{/if}
  {#if folded}
    <button type="button" class="quiet small" onclick={() => (opened = true)}>Show it</button>
  {:else}
    {#if post.value.title}<h2 class="post-title">{post.value.title}</h2>{/if}
    {#if post.value.text}<p class="post-text">{post.value.text}</p>{/if}
  {/if}
  {#if children}<div class="post-acts">{@render children()}</div>{/if}
</article>
