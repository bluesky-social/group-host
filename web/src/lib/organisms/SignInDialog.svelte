<script lang="ts">
	import Dialog from '#lib/molecules/Dialog.svelte';
	import HandleInput from '#lib/molecules/HandleInput.svelte';
	import { SIGN_IN } from '#lib/signIn.ts';
	/**
	 * Signing in with an Atmosphere account, as oneself: the handle, sent to
	 * the group host's `/console/login`, which goes to the account's own
	 * server to approve it and back to `back`, the page it was opened on.
	 * Picking an account the field suggests signs in as it at once.
	 */
	let { back = '/' }: { back?: string } = $props();
	// Apps on the Atmosphere, any of whose accounts signs in here: a row
	// running past, twice over so it wraps without a seam.
	const apps = ['bluesky', 'tangled', 'leaflet', 'semble', 'skylight', 'popfeed', 'anisota', 'flashes', 'spark', 'stream-place', 'blento', 'germ', 'gander', 'offprint', 'pckt', 'plyr', 'beacon-bits', 'eurosky', 'northsky', 'blacksky'];
	const icon = (app: string) => `/atmo/${app}.${app === 'spark' ? 'png' : 'jpg'}`;
</script>

<Dialog id={SIGN_IN} title="Sign in with your Atmosphere account">
	<div class="apps" aria-hidden="true">
		<div class="apps-run">
			{#each [...apps, ...apps] as app, i (i)}<img src={icon(app)} alt="" width="40" height="40" loading="lazy" />{/each}
		</div>
	</div>
	<p class="what"><a href="https://atmosphereaccount.com/" target="_blank" rel="noopener noreferrer">What is an Atmosphere account?</a></p>
	<form class="stack" method="get" action="/console/login">
		<input type="hidden" name="back" value={back} />
		<HandleInput id="sign-in-handle" label="Handle" autocomplete="username" placeholder="you.bsky.social" onpick={(f) => f.form?.requestSubmit()} />
		<div class="actions">
			<button class="quiet" type="button" popovertarget={SIGN_IN} popovertargetaction="hide">Cancel</button>
			<button type="submit">Sign in</button>
		</div>
	</form>
	<p class="create">Don't have an account? <a href="https://bsky.app/" target="_blank" rel="noopener noreferrer">Create one</a></p>
</Dialog>
