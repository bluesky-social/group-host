<script lang="ts">
	import SignIn from '#lib/molecules/SignIn.svelte';
	import SignInDialog from '#lib/organisms/SignInDialog.svelte';
	import UserMenu from '#lib/molecules/UserMenu.svelte';
	import { page } from '$app/state';
	/** The top of every page: the way home, and the menu of whoever is signed in, or the way in. */
	let { viewer }: { viewer: { did: string; handle?: string } | null } = $props();
</script>

<header class="top">
	<div class="wrap">
		<a class="brand" href="/">opensocial</a>
		{#if viewer}
			<UserMenu did={viewer.did} handle={viewer.handle} />
		{:else}
			<div class="sign-in"><SignIn quiet /></div>
		{/if}
	</div>
</header>
{#if !viewer}<SignInDialog back={`${page.url.pathname}${page.url.search}`} />{/if}
