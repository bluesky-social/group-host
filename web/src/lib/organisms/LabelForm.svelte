<script lang="ts">
	/** Labelling someone, by their handle, as the group, with a value. */
</script>

<form class="stack" method="post" action="?/label">
	<label class="field">Their handle <input name="handle" required /></label>
	<label class="field">Label <input name="val" required maxlength="128" placeholder="spam" /></label>
	<button type="submit">Label them</button>
</form>
