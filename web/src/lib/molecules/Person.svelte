<script lang="ts">
	import Avatar from '#lib/atoms/Avatar.svelte';
	/**
	 * Someone named in a line of text, as whose a thing is: their picture,
	 * small, and their handle; their DID only where no handle is known.
	 */
	let { actor }: { actor: { did: string; handle?: string } } = $props();
</script>

<span class="person"><Avatar size="sm" name={actor.handle ?? actor.did} /><span class="mono">{actor.handle ?? actor.did}</span></span>
