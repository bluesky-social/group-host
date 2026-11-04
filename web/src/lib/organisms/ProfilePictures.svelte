<script lang="ts">
	import Avatar from '#lib/atoms/Avatar.svelte';
	/**
	 * A group's pictures, to change: its avatar, round beside its name, and
	 * its banner, across the top of its page. Each is uploaded, replaced or
	 * taken away; `avatar` and `banner` are where they are served, if set.
	 */
	let { avatar, banner }: { avatar: string | null; banner: string | null } = $props();
	const slots = $derived([
		{ slot: 'avatar', name: 'Avatar', src: avatar },
		{ slot: 'banner', name: 'Banner', src: banner }
	]);
</script>

<div class="stack">
	{#each slots as s (s.slot)}
		<form class="stack" method="post" action="?/picture" enctype="multipart/form-data">
			<input type="hidden" name="slot" value={s.slot} />
			{#if s.slot === 'avatar'}<Avatar src={s.src} size="md" />{:else if s.src}<img class="banner" src={s.src} alt="" />{/if}
			<label class="field">{s.name} <input name="file" type="file" accept="image/png,image/jpeg,image/webp,image/gif,image/avif" required /></label>
			<button type="submit">Upload the {s.name.toLowerCase()}</button>
		</form>
		{#if s.src}
			<form method="post" action="?/unpicture"><input type="hidden" name="slot" value={s.slot} /><button type="submit">Take the {s.name.toLowerCase()} away</button></form>
		{/if}
	{/each}
</div>
