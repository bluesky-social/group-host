<script lang="ts">
	import Avatar from '#lib/atoms/Avatar.svelte';
	/** A group in a list: its picture, its name, and its handle. */
	let { name, handle, avatar }: { name: string; handle: string; avatar?: string | null } = $props();
</script>

<div class="group-row">
	<Avatar src={avatar} size="md" />
	<div>
		<div>{name}</div>
		<div class="mute mono">@{handle}</div>
	</div>
</div>
