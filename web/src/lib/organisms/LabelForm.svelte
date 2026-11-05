<script lang="ts">
	import HandleInput from '#lib/molecules/HandleInput.svelte';
	/** Labelling someone, by their handle, as the group, with a value. */
</script>

<form class="stack" method="post" action="?/label">
	<HandleInput id="label-handle" label="Their handle" />
	<label class="field">Label <input name="val" required maxlength="128" placeholder="spam" /></label>
	<button type="submit">Label them</button>
</form>
