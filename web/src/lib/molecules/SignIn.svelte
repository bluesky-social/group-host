<script lang="ts">
	/**
	 * Signing in, as oneself, at one's own PDS: a handle, sent to the group
	 * host's `/console/login`, which goes there and back to `back`.
	 */
	let { back = '/' }: { back?: string } = $props();
</script>

<form class="sign-in" method="get" action="/console/login">
	<input type="hidden" name="back" value={back} />
	<label>
		Your handle
		<input name="handle" required autocomplete="username" placeholder="you.bsky.social" />
	</label>
	<button type="submit">Sign in</button>
</form>
