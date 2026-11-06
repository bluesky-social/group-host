<script lang="ts">
  import SignIn from '#lib/molecules/SignIn.svelte';
  import SignInDialog from '#lib/organisms/SignInDialog.svelte';
  import UserMenu from '#lib/molecules/UserMenu.svelte';
  import { page } from '$app/state';
  /** The top of every page: the way home, and the menu of whoever is signed in, marking the invites waiting for them (`invites`) and offering the other accounts the browser is signed in to (`accounts`), or the way in; and where to make an account (`signUpAt`, a PDS), from the sign-in that adds one too. */
  let {
    viewer,
    invites = 0,
    accounts = [],
    signUpAt = 'https://bsky.social'
  }: {
    viewer: { did: string; handle?: string } | null;
    invites?: number;
    accounts?: { did: string; handle?: string }[];
    signUpAt?: string;
  } = $props();
</script>

<header class="top">
  <div class="wrap">
    <a class="brand" href="/">opensocial</a>
    {#if viewer}
      <UserMenu did={viewer.did} handle={viewer.handle} {invites} {accounts} />
    {:else}
      <div class="sign-in"><SignIn quiet /></div>
    {/if}
  </div>
</header>
<SignInDialog back={`${page.url.pathname}${page.url.search}`} {signUpAt} />
