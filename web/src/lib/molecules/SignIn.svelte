<script lang="ts">
	import { SIGN_IN } from '#lib/signIn.ts';
	/**
	 * The way in: a button opening the sign-in dialog, wherever it is
	 * wanted; `quiet` in the header, where a page's own is the one to press.
	 */
	let { quiet = false }: { quiet?: boolean } = $props();
</script>

<button class:quiet type="button" popovertarget={SIGN_IN}>Sign in</button>
