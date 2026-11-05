<script lang="ts">
	/**
	 * A group's profile, to change: its name, its words, how people join,
	 * its own site, and up to five links, each an address and what it is.
	 */
	let {
		displayName,
		description,
		joinPolicy,
		url,
		links = []
	}: { displayName?: string; description?: string; joinPolicy?: string; url?: string; links?: { url: string; title?: string }[] } = $props();
	const policy = $derived(joinPolicy ?? 'approval');
	// Each link it has, then empty rows up to five, for more.
	const rows = $derived([...links, ...Array.from({ length: Math.max(0, 5 - links.length) }, () => ({ url: '', title: '' }))]);
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
	<fieldset class="link-rows">
		<legend>Links, up to five</legend>
		{#each rows as link, i (i)}
			<label class="field">Link {i + 1} <input name="linkUrl" type="url" placeholder="https://" value={link.url} /></label>
			<label class="field">What link {i + 1} is <input name="linkTitle" maxlength="64" value={link.title ?? ''} /></label>
		{/each}
	</fieldset>
	<button type="submit">Save the profile</button>
</form>
