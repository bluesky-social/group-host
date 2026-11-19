<script>
  import JoinDialog from './lib/organisms/JoinDialog.svelte';
  import Landing from './lib/organisms/Landing.svelte';
  import Place from './lib/templates/Place.svelte';
  import Board from './rooms/Board.svelte';
  import Discussion from './rooms/Discussion.svelte';
  import Events from './rooms/Events.svelte';
  import People from './rooms/People.svelte';
  import { ABOUT } from './lib/about.js';
  import { forget } from './lib/kept.svelte.js';
  import { cancelRequest, context, join, onChange, signIn, signOut } from './lib/place.js';
  import { route } from './lib/route.svelte.js';
  /**
   * The site: the place, as whoever is looking may see it, read again as
   * that changes (signing in or out, in place; joining, asking, cancelling
   * or leaving, by `ctx.reread`). Its members, its rooms; anyone else, its
   * landing, with the way in as the place's join policy has it.
   */
  let ctx = $state(null);
  let failed = $state('');
  let refused = $state('');
  let busy = $state(false);
  const rooms = { board: Board, events: Events, discussion: Discussion, people: People };
  const Room = $derived(rooms[route.room]);
  const policy = $derived(ctx?.place?.joinPolicy ?? 'approval');

  async function look() {
    try {
      ctx = { ...(await context()), reread: look };
    } catch (e) {
      failed = e.message;
    }
  }
  look();
  onChange(() => {
    forget();
    look();
  });

  /** Does `act`, then reads the place again; refused, says why beside the way in. */
  async function then(act) {
    busy = true;
    refused = '';
    try {
      await act();
      await look();
    } catch (e) {
      if (e.name === 'JoinClosed') refused = "There's no invite waiting for you yet.";
      else if (e.name === 'AlreadyMember') await look();
      else refused = e.message;
    } finally {
      busy = false;
    }
  }
</script>

{#snippet standing(title, words)}<strong>{title}</strong><span
    >You're signed in as @{ctx.me.handle}. {words}</span
  >{#if refused}<span class="failed" role="status">{refused}</span>{/if}{/snippet}

{#if ctx?.viewer?.member}
  <Place name={ctx.place?.displayName ?? ctx.group.handle}>
    <Room {ctx} />
  </Place>
{:else if ctx?.me.signedIn}
  <Landing about={ABOUT}>
    {#snippet note()}
      {#if ctx.viewer?.request}{@render standing(
          'Your request is waiting',
          "You're in once an admin approves it."
        )}
      {:else if policy === 'open'}{@render standing('Not a member yet', 'Anyone can join.')}
      {:else if policy === 'invite'}{@render standing(
          'Joining is by invite',
          `If someone in ${ABOUT.name} invited you, join here.`
        )}
      {:else}{@render standing('Not a member yet', 'Ask to join, and an admin lets you in.')}
      {/if}
    {/snippet}
    {#snippet actions()}
      {#if ctx.viewer?.request}<button
          type="button"
          class="quiet"
          disabled={busy}
          onclick={() => then(cancelRequest)}>Cancel request</button
        >
      {:else if policy === 'open' || policy === 'invite'}<button
          type="button"
          class="big"
          disabled={busy}
          onclick={() => then(() => join())}>Join</button
        >
      {:else}<JoinDialog ask={(message) => then(() => join(message))} />
      {/if}
      <button type="button" class="quiet" onclick={signOut}>Sign out</button>
    {/snippet}
  </Landing>
{:else if ctx}
  <Landing about={ABOUT}>
    {#snippet actions()}<button type="button" class="big" onclick={signIn}>Sign in</button
      >{/snippet}
  </Landing>
{:else if failed}
  <p class="failed">{failed}</p>
{/if}
