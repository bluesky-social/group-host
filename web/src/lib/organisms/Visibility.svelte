<script lang="ts">
	/**
	 * Who sees a group's profile and rules: anyone, when it is also declared
	 * on the network for others to find, or its members alone, when it shows
	 * elsewhere by its handle alone. `listed` is which it is.
	 */
	let { listed }: { listed: boolean } = $props();
</script>

<form class="stack" method="post" action="?/visibility">
	<label><input type="radio" name="seen" value="public" checked={listed} /> Anyone, and found on the network</label>
	<label><input type="radio" name="seen" value="members" checked={!listed} /> Its members alone: to others, just its handle</label>
	<p class="mute">Hiding it stops it being found from now on. It cannot take back what someone has already seen.</p>
	<button class="quiet" type="submit">Save who sees it</button>
</form>
