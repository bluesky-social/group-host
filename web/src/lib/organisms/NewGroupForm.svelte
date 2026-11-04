<script lang="ts">
	/**
	 * Starting a group: its handle, its name and words, and how people get
	 * in. `failed` is what the host said of the last try, if it refused it.
	 */
	let { failed }: { failed?: string } = $props();
</script>

<form class="stack" method="post">
	{#if failed}<p class="danger">{failed}</p>{/if}
	<label class="field">Handle <input name="handle" required placeholder="riders.example.com" /></label>
	<label class="field">Name <input name="displayName" required maxlength="64" /></label>
	<label class="field">What it is for <textarea name="description" maxlength="300"></textarea></label>
	<label class="field">
		How people join
		<select name="joinPolicy">
			<option value="approval">They ask, and a member who may admits them</option>
			<option value="open">Anyone signed in, at once</option>
			<option value="invite">Only by invitation</option>
		</select>
	</label>
	<button type="submit">Start the group</button>
</form>
