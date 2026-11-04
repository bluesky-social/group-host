<script lang="ts">
	/**
	 * The way back to an app, once its request is answered: taken at once,
	 * or by hand without the page's script.
	 */
	let { to }: { to: string } = $props();
	$effect(() => {
		window.location.assign(to);
	});
</script>

<p>Taking you back to the app… <a href={to}>Go back to the app</a></p>
