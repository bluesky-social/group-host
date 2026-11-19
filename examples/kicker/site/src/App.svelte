<script>
  import Landing from './lib/organisms/Landing.svelte';
  import Place from './lib/templates/Place.svelte';
  import Board from './rooms/Board.svelte';
  import Discussion from './rooms/Discussion.svelte';
  import Events from './rooms/Events.svelte';
  import People from './rooms/People.svelte';
  import { ABOUT } from './lib/about.js';
  import { forget } from './lib/kept.svelte.js';
  import { context, onChange, signIn, signOut } from './lib/place.js';
  import { route } from './lib/route.svelte.js';
  /**
   * The site: the place, as whoever is looking may see it, read again as
   * that changes (signing in or out, in place). Its members, its rooms;
   * anyone else, its landing, with the way in.
   */
  let ctx = $state(null);
  let failed = $state('');
  const rooms = { board: Board, events: Events, discussion: Discussion, people: People };
  const Room = $derived(rooms[route.room]);

  async function look() {
    try {
      ctx = await context();
    } catch (e) {
      failed = e.message;
    }
  }
  look();
  onChange(() => {
    forget();
    look();
  });
</script>

{#if ctx?.viewer?.member}
  <Place name={ctx.place?.displayName ?? ctx.group.handle}>
    <Room {ctx} />
  </Place>
{:else if ctx?.me.signedIn}
  <Landing about={ABOUT} act={signOut} actLabel="Sign out">
    {#snippet note()}<strong>Not a member yet</strong><span
        >You're signed in as @{ctx.me.handle}. Only members come in: an admin lets you in.</span
      >{/snippet}
  </Landing>
{:else if ctx}
  <Landing about={ABOUT} act={signIn} actLabel="Sign in" />
{:else if failed}
  <p class="failed">{failed}</p>
{/if}
