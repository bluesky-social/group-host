<script lang="ts">
	import Avatar from '#lib/atoms/Avatar.svelte';
	/** A group in a list: its picture, its name (a link to its page, the whole row), its handle, and `note`, what it said, under them. */
	let { name, handle, avatar, note }: { name: string; handle: string; avatar?: string | null; note?: string } = $props();
</script>

<div class="group-row">
	<Avatar src={avatar} size="md" {name} />
	<div>
		<div><a href={`/g/${handle}`}>{name}</a></div>
		<div class="mute mono">@{handle}</div>
		{#if note}<div class="what">{note}</div>{/if}
	</div>
</div>
