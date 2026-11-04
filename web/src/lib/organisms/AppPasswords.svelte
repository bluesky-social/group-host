<script lang="ts">
	import Empty from '#lib/molecules/Empty.svelte';
	/**
	 * The group's app passwords, never the passwords, each to revoke; and a
	 * new one to make, shown once (`made`).
	 */
	type Password = { name: string; member: string; privileged: boolean; lastUsedAt?: string };
	let { passwords, made }: { passwords: Password[]; made?: { name: string; password: string } } = $props();
</script>

<div class="stack">
	{#if made}
		<p class="made">The app password “{made.name}” is <code class="password">{made.password}</code>. Copy it now: it is not shown again.</p>
	{/if}
	{#if passwords.length}
		<ul class="passwords">
			{#each passwords as p (p.name)}
				<li>
					<strong>{p.name}</strong> <span class="mute">by {p.member}{p.privileged ? ', with chat' : ''}{p.lastUsedAt ? `, last used ${p.lastUsedAt}` : ''}</span>
					<form method="post" action="?/unpassword"><input type="hidden" name="name" value={p.name} /><button type="submit">Revoke “{p.name}”</button></form>
				</li>
			{/each}
		</ul>
	{:else}
		<Empty title="No app passwords">An app that signs in with a handle and a password, as the Bluesky app does, signs in to the group with one.</Empty>
	{/if}
	<form class="stack" method="post" action="?/password">
		<label class="field">Where it is used <input name="name" required maxlength="64" placeholder="My phone" /></label>
		<label><input type="checkbox" name="privileged" /> It may use chat</label>
		<button type="submit">Make an app password</button>
	</form>
</div>
