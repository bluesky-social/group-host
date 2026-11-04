<script lang="ts">
	/**
	 * A group's roles: each, what it may do, to take away; and a new one to
	 * define, with what it may do, the roles its holders may give, and
	 * whether members get it on joining.
	 */
	type Role = { key: string; name?: string; description?: string };
	type Binding = { role: string; actions: string[] };
	let { roles = [], bindings = [], actions }: { roles?: Role[]; bindings?: Binding[]; actions: string[] } = $props();
	const may = (key: string) => bindings.find((b) => b.role === key)?.actions ?? [];
</script>

<div class="stack">
	<ul class="roles">
		{#each roles as r (r.key)}
			<li>
				<strong>{r.name ?? r.key}</strong> <span class="mute mono">{r.key}</span>
				<p class="mute">{may(r.key).join(', ') || 'Nothing beyond being a member'}</p>
				<form method="post" action="?/unrole"><input type="hidden" name="role" value={r.key} /><button class="quiet" type="submit">Take away {r.name ?? r.key}</button></form>
			</li>
		{/each}
	</ul>
	<form class="stack" method="post" action="?/role">
		<label class="field">Its key <input name="role" required pattern="[a-z0-9-]+" maxlength="64" /></label>
		<label class="field">Its name <input name="name" required maxlength="64" /></label>
		<fieldset>
			<legend>It may</legend>
			{#each actions as a (a)}<label><input type="checkbox" name="action" value={a} /> {a}</label>{/each}
		</fieldset>
		<fieldset>
			<legend>Its holders may give</legend>
			{#each roles as r (r.key)}<label><input type="checkbox" name="assignable" value={r.key} /> {r.name ?? r.key}</label>{/each}
		</fieldset>
		<label><input type="checkbox" name="default" /> Every new member gets it</label>
		<button type="submit">Define the role</button>
	</form>
</div>
