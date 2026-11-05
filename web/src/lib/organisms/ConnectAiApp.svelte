<script lang="ts">
	import Copy from '#lib/atoms/Copy.svelte';
	/**
	 * How to have an AI app act for the person in the group: the address
	 * to add it at (`mcp`), what it then asks, and what to ask of it, each
	 * a thing the host's tools do.
	 */
	let { name, mcp }: { name: string; mcp: string } = $props();
	const asks = ['Who is asking to join, and what did they say?', 'Invite dana.bsky.social', 'What is waiting in the moderation queue?'];
</script>

<div class="connect stack">
	<p class="mute">It acts for you in {name}, with what your roles let you do.</p>
	<div class="well stack">
		<strong>In your AI app, add a connector with this address</strong>
		<div class="address"><code>{mcp}</code><Copy text={mcp} /></div>
		<small class="mute">It will ask you to sign in here and approve it.</small>
	</div>
	<div class="stack">
		<strong>Then ask it things like</strong>
		<ul class="asks">
			{#each asks as ask (ask)}<li>{ask}</li>{/each}
		</ul>
	</div>
</div>
