<script lang="ts">
	import Avatar from '#lib/atoms/Avatar.svelte';
	/**
	 * Who is signed in, as their picture and handle, opening a menu of
	 * what is theirs: their groups, starting one, moving an account here,
	 * and, last, signing out (`/console/logout`, the host's). It closes on
	 * a click elsewhere, on Escape, and on going to another page.
	 */
	let { handle, did }: { handle?: string; did: string } = $props();
	const name = $derived(handle ? `@${handle}` : did);
	let menu: HTMLDetailsElement;
	function away(e: Event) {
		if (menu?.open && !menu.contains(e.target as Node)) menu.open = false;
	}
	function key(e: KeyboardEvent) {
		if (e.key === 'Escape' && menu?.open) {
			menu.open = false;
			menu.querySelector('summary')?.focus();
		}
	}
	const close = () => (menu.open = false);
</script>

<svelte:document onclick={away} onkeydown={key} />

<details class="user-menu" bind:this={menu}>
	<summary aria-label={`${name}: your menu`}><Avatar name={handle ?? did} /><span class="handle mono">{name}</span></summary>
	<div class="menu">
		<a href="/" onclick={close}>Your groups</a>
		<a href="/new" onclick={close}>Start a group</a>
		<a href="/move" onclick={close}>Move an account here</a>
		<hr />
		<form method="post" action="/console/logout"><button class="quiet" type="submit">Sign out</button></form>
	</div>
</details>
