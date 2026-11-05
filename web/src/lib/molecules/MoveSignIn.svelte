<script lang="ts">
	import HandleInput from '#lib/molecules/HandleInput.svelte';
	/**
	 * Signing in to an account to move here, at its own server: its handle,
	 * sent to the group host's `/console/move/login`, which goes there and
	 * back to `back`.
	 */
	let { back = '/move' }: { back?: string } = $props();
</script>

<form class="stack" method="get" action="/console/move/login">
	<input type="hidden" name="back" value={back} />
	<HandleInput id="move-handle" label="The account to move" placeholder="riders.bsky.social" />
	<p>Its server asks you to sign in to it, and to let this host move it.</p>
	<button type="submit">Sign in to it</button>
</form>
