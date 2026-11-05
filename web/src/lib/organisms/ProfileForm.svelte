<script lang="ts">
	/**
	 * A group's profile, to change: its name, its words, how people join,
	 * and its own site. Its links are a list of their own (LinksEditor).
	 */
	let {
		displayName,
		description,
		joinPolicy,
		url
	}: { displayName?: string; description?: string; joinPolicy?: string; url?: string } = $props();
	const policy = $derived(joinPolicy ?? 'approval');
</script>

<form class="stack" method="post" action="?/profile">
	<label class="field">Name <input name="displayName" required maxlength="64" value={displayName ?? ''} /></label>
	<label class="field">What it is for <textarea name="description" maxlength="2000">{description ?? ''}</textarea></label>
	<label class="field">
		How people join
		<select name="joinPolicy" value={policy}>
			<option value="approval">People ask to join, and an admin approves them</option>
			<option value="open">Anyone can join right away</option>
			<option value="invite">Only people who are invited</option>
		</select>
	</label>
	<label class="field">Its own site <input name="url" type="url" value={url ?? ''} /></label>
	<button type="submit">Save</button>
</form>
