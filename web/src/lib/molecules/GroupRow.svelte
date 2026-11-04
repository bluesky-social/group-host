<script lang="ts">
	import Avatar from '#lib/atoms/Avatar.svelte';
	/** A group in a list: its picture, its name (a link to its page), and its handle. */
	let { name, handle, avatar }: { name: string; handle: string; avatar?: string | null } = $props();
</script>

<div class="group-row">
	<Avatar src={avatar} size="md" />
	<div>
		<div><a href={`/g/${handle}`}>{name}</a></div>
		<div class="mute mono">@{handle}</div>
	</div>
</div>
