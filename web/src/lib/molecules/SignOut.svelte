<script lang="ts">
	/** Who is signed in, and the way out: `/console/logout`, the host's. */
	let { handle, did }: { handle?: string; did: string } = $props();
</script>

<form class="sign-out" method="post" action="/console/logout">
	<span class="mute">Signed in as <span class="mono">{handle ? `@${handle}` : did}</span></span>
	<button type="submit">Sign out</button>
</form>
