<script lang="ts">
	/**
	 * Signing in to an account to move here, at its own server: its handle,
	 * sent to the group host's `/console/move/login`, which goes there and
	 * back to `back`.
	 */
	let { back = '/move' }: { back?: string } = $props();
</script>

<form class="stack" method="get" action="/console/move/login">
	<input type="hidden" name="back" value={back} />
	<label class="field">
		The account to move
		<input name="handle" required autocomplete="off" placeholder="riders.bsky.social" />
	</label>
	<p>Its server asks you to sign in to it, and to let this host move it.</p>
	<button type="submit">Sign in to it</button>
</form>
