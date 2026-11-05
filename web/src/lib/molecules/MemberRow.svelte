<script lang="ts">
	import type { Snippet } from 'svelte';
	import Avatar from '#lib/atoms/Avatar.svelte';
	import Tag from '#lib/atoms/Tag.svelte';
	/**
	 * Someone in a list of a group's people: their picture, their handle
	 * (or DID, when no handle is known), the roles they hold, and then
	 * `children`, what may be done about them.
	 */
	let { actor, roles = [], children }: { actor: { did: string; handle?: string }; roles?: string[]; children?: Snippet } = $props();
</script>

<Avatar />
<span class="who mono">{actor.handle ?? actor.did}</span>
{#each roles as role (role)}<Tag text={role} />{/each}
{@render children?.()}
