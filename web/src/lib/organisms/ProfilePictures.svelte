<script lang="ts">
	import Avatar from '#lib/atoms/Avatar.svelte';
	import FormDialog from '#lib/molecules/FormDialog.svelte';
	/**
	 * A group's pictures, to change: its avatar, round beside its name, and
	 * its banner, across the top of its page, each as it is now; a new one
	 * uploaded from a dialog, or the one there removed. `avatar` and
	 * `banner` are where they are served, if set; `name`, the group's.
	 */
	let { name, avatar, banner }: { name: string; avatar: string | null; banner: string | null } = $props();
	const slots = $derived([
		{ slot: 'avatar', name: 'Avatar', src: avatar, act: avatar ? 'Change the avatar' : 'Add the avatar' },
		{ slot: 'banner', name: 'Banner', src: banner, act: banner ? 'Change the banner' : 'Add the banner' }
	]);
</script>

<ul class="pictures">
	{#each slots as s (s.slot)}
		<li>
			{#if s.slot === 'avatar'}
				<Avatar src={s.src} size="md" {name} />
			{:else if s.src}
				<img class="banner-thumb" src={s.src} alt="" />
			{:else}
				<span class="banner-thumb" aria-hidden="true"></span>
			{/if}
			<span class="row-main"><strong>{s.name}</strong><span class="what">{s.src ? 'Shown on its page' : 'None yet'}</span></span>
			<span class="row-actions">
				{#if s.src}
					<form method="post" action="?/unpicture"><input type="hidden" name="slot" value={s.slot} /><button class="quiet small" type="submit">Remove the {s.name.toLowerCase()}</button></form>
				{/if}
				<button class="quiet small" type="button" popovertarget={`picture-${s.slot}`}>{s.act}</button>
			</span>
			<FormDialog id={`picture-${s.slot}`} title={s.act} action="?/picture" submit={s.act} multipart>
				{#snippet intro()}{s.slot === 'avatar' ? 'A square picture, drawn round beside its name.' : 'A wide picture, three times as wide as it is high, across the top of its page.'}{/snippet}
				<input type="hidden" name="slot" value={s.slot} />
				<label class="field">{s.name} <input name="file" type="file" accept="image/png,image/jpeg,image/webp" required /></label>
			</FormDialog>
		</li>
	{/each}
</ul>
