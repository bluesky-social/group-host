<script lang="ts">
	import Dialog from '#lib/molecules/Dialog.svelte';
	import { SIGN_IN } from '#lib/signIn.ts';
	/**
	 * Signing in with an Atmosphere account, as oneself: the handle, sent to
	 * the group host's `/console/login`, which goes to the account's own
	 * server to approve it and back to `back`, the page it was opened on.
	 */
	let { back = '/' }: { back?: string } = $props();
</script>

<Dialog id={SIGN_IN} title="Sign in with your Atmosphere account">
	<p class="mute">The account you use on Bluesky, or any app of the Atmosphere. <a href="https://atmosphereaccount.com/" target="_blank" rel="noopener noreferrer">What is that?</a></p>
	<form class="stack" method="get" action="/console/login">
		<input type="hidden" name="back" value={back} />
		<label class="field">
			Your handle
			<input name="handle" required autocomplete="username" autocapitalize="none" spellcheck="false" placeholder="you.bsky.social" />
		</label>
		<div class="actions">
			<button class="quiet" type="button" popovertarget={SIGN_IN} popovertargetaction="hide">Cancel</button>
			<button type="submit">Continue</button>
		</div>
	</form>
</Dialog>
