<script lang="ts">
	/** A group's profile, to change: its name, its words, how people join, and its own site. */
	let {
		displayName,
		description,
		joinPolicy,
		url
	}: { displayName?: string; description?: string; joinPolicy?: string; url?: string } = $props();
	const policy = $derived(joinPolicy?.split('#').pop() ?? 'approval');
</script>

<form class="stack" method="post" action="?/profile">
	<label class="field">Name <input name="displayName" required maxlength="64" value={displayName ?? ''} /></label>
	<label class="field">What it is for <textarea name="description" maxlength="2000">{description ?? ''}</textarea></label>
	<label class="field">
		How people join
		<select name="joinPolicy" value={policy}>
			<option value="approval">They ask, and a member who may admits them</option>
			<option value="open">Anyone signed in, at once</option>
			<option value="invite">Only by invitation</option>
		</select>
	</label>
	<label class="field">Its own site <input name="url" type="url" value={url ?? ''} /></label>
	<button type="submit">Save the profile</button>
</form>
