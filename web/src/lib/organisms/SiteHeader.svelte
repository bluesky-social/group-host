<script lang="ts">
  import SignIn from '#lib/molecules/SignIn.svelte';
  import SignInDialog from '#lib/organisms/SignInDialog.svelte';
  import UserMenu from '#lib/molecules/UserMenu.svelte';
  import { page } from '$app/state';
  /** The top of every page: the way home, and the menu of whoever is signed in, marking the invites waiting for them (`invites`), or the way in, and where to make an account (`signUpAt`, a PDS). */
  let {
    viewer,
    invites = 0,
    signUpAt = 'https://bsky.social'
  }: {
    viewer: { did: string; handle?: string } | null;
    invites?: number;
    signUpAt?: string;
  } = $props();
</script>

<header class="top">
  <div class="wrap">
    <a class="brand" href="/">opensocial</a>
    {#if viewer}
      <UserMenu did={viewer.did} handle={viewer.handle} {invites} />
    {:else}
      <div class="sign-in"><SignIn quiet /></div>
    {/if}
  </div>
</header>
{#if !viewer}<SignInDialog back={`${page.url.pathname}${page.url.search}`} {signUpAt} />{/if}
